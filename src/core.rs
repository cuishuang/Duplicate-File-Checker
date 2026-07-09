use crate::config::AppConfig;
use chrono::Local;
// use rayon::prelude::*; // 暂时不使用并行处理
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;
use walkdir::{DirEntry, WalkDir};

const DISK_DB_PATH: &str = "disk_files.db";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub sample_hash: Option<String>,
    pub disk_id: String,
}

#[derive(Debug, Clone)]
pub struct IndexedFile {
    pub path: String,
    pub size: u64,
    pub sample_hash: Option<String>,
    pub disk_id: String,
}

#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub size: u64,
    pub files: Vec<IndexedFile>,
}

#[derive(Debug, Clone)]
pub struct DiskSummary {
    pub disk_id: String,
    pub display_name: String,
    pub root_path: String,
    pub last_scanned_at: String,
    pub file_count: i64,
    pub total_size: i64,
}

pub struct FileScanner {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    MoveToTrash,
    DirectRemove,
}

impl DeleteMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MoveToTrash => "trash",
            Self::DirectRemove => "direct",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "direct" => Self::DirectRemove,
            _ => Self::MoveToTrash,
        }
    }
}

impl FileScanner {
    pub fn new() -> Self {
        let db_path = default_database_path().expect("无法确定数据库路径");
        let conn = Self::init_database_at(&db_path).expect("无法初始化数据库");
        Self { conn }
    }

    pub fn with_database_path(path: impl AsRef<Path>) -> Result<Self, rusqlite::Error> {
        let conn = Self::init_database_at(path.as_ref())?;
        Ok(Self { conn })
    }

    fn init_database_at(path: &Path) -> Result<Connection, rusqlite::Error> {
        let conn = Connection::open(path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                path TEXT NOT NULL,
                size INTEGER NOT NULL,
                sample_hash TEXT,
                disk_id TEXT NOT NULL,
                PRIMARY KEY (disk_id, path)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS disks (
                disk_id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                root_path TEXT NOT NULL,
                last_scanned_at TEXT NOT NULL,
                file_count INTEGER NOT NULL,
                total_size INTEGER NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;
        Self::migrate_legacy_files_table(&conn)?;
        Self::ensure_default_settings(&conn)?;
        Self::backfill_disk_summaries(&conn)?;
        Self::repair_legacy_disk_summaries(&conn)?;

        conn.execute("CREATE INDEX IF NOT EXISTS idx_size ON files (size)", [])?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_size_hash ON files (size, sample_hash)",
            [],
        )?;

        Ok(conn)
    }

    fn ensure_default_settings(conn: &Connection) -> Result<(), rusqlite::Error> {
        let default_config =
            serde_json::to_string(&AppConfig::default()).expect("默认配置必须可以序列化");
        conn.execute(
            "INSERT OR IGNORE INTO app_settings (key, value)
             VALUES ('delete_mode', 'trash')",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO app_settings (key, value)
             VALUES ('app_config', ?)",
            params![default_config],
        )?;
        Ok(())
    }

    fn migrate_legacy_files_table(conn: &Connection) -> Result<(), rusqlite::Error> {
        let create_sql: String = conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'files'",
            [],
            |row| row.get(0),
        )?;

        if !create_sql.contains("path TEXT PRIMARY KEY") {
            return Ok(());
        }

        conn.execute_batch(
            "ALTER TABLE files RENAME TO files_legacy;
             CREATE TABLE files (
                path TEXT NOT NULL,
                size INTEGER NOT NULL,
                sample_hash TEXT,
                disk_id TEXT NOT NULL,
                PRIMARY KEY (disk_id, path)
             );
             INSERT OR REPLACE INTO files (path, size, sample_hash, disk_id)
             SELECT path, size, sample_hash, disk_id FROM files_legacy;
             DROP TABLE files_legacy;",
        )?;

        Ok(())
    }

    fn backfill_disk_summaries(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "INSERT OR IGNORE INTO disks
             (disk_id, display_name, root_path, last_scanned_at, file_count, total_size)
             SELECT disk_id, disk_id, '', '旧版本索引', COUNT(*), COALESCE(SUM(size), 0)
             FROM files
             GROUP BY disk_id",
            [],
        )?;
        Ok(())
    }

    fn repair_legacy_disk_summaries(conn: &Connection) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare("SELECT disk_id FROM disks WHERE root_path = ''")?;
        let legacy_disk_ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        for disk_id in legacy_disk_ids {
            let paths = Self::paths_for_disk(conn, &disk_id)?;
            let Some(inferred) = infer_common_disk_root(&paths) else {
                continue;
            };

            let should_rename = is_legacy_placeholder_id(&disk_id) && inferred.disk_id != disk_id;
            if should_rename && !Self::disk_id_exists(conn, &inferred.disk_id)? {
                conn.execute(
                    "UPDATE files SET disk_id = ? WHERE disk_id = ?",
                    params![inferred.disk_id, disk_id],
                )?;
                conn.execute(
                    "UPDATE disks
                     SET disk_id = ?,
                         display_name = ?,
                         root_path = ?,
                         file_count = (SELECT COUNT(*) FROM files WHERE disk_id = ?),
                         total_size = (SELECT COALESCE(SUM(size), 0) FROM files WHERE disk_id = ?)
                     WHERE disk_id = ?",
                    params![
                        inferred.disk_id,
                        inferred.disk_id,
                        inferred.root_path,
                        inferred.disk_id,
                        inferred.disk_id,
                        disk_id
                    ],
                )?;
            } else {
                conn.execute(
                    "UPDATE disks
                     SET root_path = ?,
                         file_count = (SELECT COUNT(*) FROM files WHERE disk_id = ?),
                         total_size = (SELECT COALESCE(SUM(size), 0) FROM files WHERE disk_id = ?)
                     WHERE disk_id = ?",
                    params![inferred.root_path, disk_id, disk_id, disk_id],
                )?;
            }
        }

        Ok(())
    }

    fn paths_for_disk(conn: &Connection, disk_id: &str) -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt =
            conn.prepare("SELECT path FROM files WHERE disk_id = ? ORDER BY path ASC")?;
        stmt.query_map(params![disk_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()
    }

    fn disk_id_exists(conn: &Connection, disk_id: &str) -> Result<bool, rusqlite::Error> {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM disks WHERE disk_id = ?)",
            params![disk_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|exists| exists != 0)
    }

    pub fn get_delete_mode(&self) -> Result<DeleteMode, rusqlite::Error> {
        let value = self
            .conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'delete_mode'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(value
            .as_deref()
            .map(DeleteMode::from_str)
            .unwrap_or(DeleteMode::MoveToTrash))
    }

    pub fn set_delete_mode(&mut self, delete_mode: DeleteMode) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO app_settings (key, value)
             VALUES ('delete_mode', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![delete_mode.as_str()],
        )?;
        Ok(())
    }

    pub fn get_app_config(&self) -> io::Result<AppConfig> {
        let value = self
            .conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'app_config'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| io::Error::other(format!("读取配置失败: {}", error)))?;

        match value {
            Some(value) => serde_json::from_str(&value).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("配置解析失败: {}", error),
                )
            }),
            None => Ok(AppConfig::default()),
        }
    }

    pub fn set_app_config(&mut self, config: &AppConfig) -> io::Result<()> {
        let value = serde_json::to_string(config).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("配置序列化失败: {}", error),
            )
        })?;
        self.conn
            .execute(
                "INSERT INTO app_settings (key, value)
                 VALUES ('app_config', ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![value],
            )
            .map_err(|error| io::Error::other(format!("保存配置失败: {}", error)))?;
        Ok(())
    }

    pub fn scan_disk(
        &mut self,
        disk_path: &Path,
        disk_id: &str,
        config: &AppConfig,
    ) -> io::Result<usize> {
        let start_time = Instant::now();
        let start_datetime = Local::now();
        let disk_id = sanitize_disk_id(disk_id);
        println!(
            "[{}] 开始扫描硬盘: {}",
            start_datetime.format("%Y-%m-%d %H:%M:%S"),
            disk_id
        );

        let mut files_to_process = Vec::new();

        // 遍历文件系统
        for entry in WalkDir::new(disk_path)
            .into_iter()
            .filter_entry(|e| !self.should_skip_dir(e, config))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let metadata = entry.metadata()?;
            let file_size = metadata.len();

            if config.should_process_file(entry.path(), file_size) {
                files_to_process.push(entry.path().to_path_buf());
            }
        }

        let file_count = files_to_process.len();
        println!("找到 {} 个符合条件的文件", file_count);

        // 处理文件信息 (不使用并行处理避免数据库连接问题)
        let files_info: Vec<FileInfo> = files_to_process
            .iter()
            .filter_map(|path| {
                let path_str = path.to_string_lossy().to_string();
                if let Ok(metadata) = fs::metadata(path) {
                    let size = metadata.len();
                    let sample_hash = if config.use_hash {
                        Self::calculate_sample_hash_static(path, config.sample_size).ok()
                    } else {
                        None
                    };

                    Some(FileInfo {
                        path: path_str,
                        size,
                        sample_hash,
                        disk_id: disk_id.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        // 保存到数据库
        let total_size: u64 = files_info.iter().map(|file| file.size).sum();
        let tx = self
            .conn
            .transaction()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("事务启动失败: {}", e)))?;

        tx.execute("DELETE FROM files WHERE disk_id = ?", params![disk_id])
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("清理旧索引失败: {}", e)))?;

        for file_info in &files_info {
            tx.execute(
                "INSERT OR REPLACE INTO files (path, size, sample_hash, disk_id) VALUES (?, ?, ?, ?)",
                params![file_info.path, file_info.size, file_info.sample_hash, file_info.disk_id],
            )
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("SQL执行失败: {}", e)))?;
        }

        tx.execute(
            "INSERT OR REPLACE INTO disks
             (disk_id, display_name, root_path, last_scanned_at, file_count, total_size)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                disk_id,
                disk_id,
                disk_path.to_string_lossy().to_string(),
                end_timestamp(),
                files_info.len() as i64,
                total_size as i64
            ],
        )
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("磁盘记录保存失败: {}", e)))?;

        tx.commit()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("事务提交失败: {}", e)))?;

        let elapsed = start_time.elapsed();
        let end_datetime = Local::now();
        println!(
            "[{}] 硬盘 {} 扫描完成，处理了 {} 个文件，耗时: {:.2?}",
            end_datetime.format("%Y-%m-%d %H:%M:%S"),
            disk_id,
            files_info.len(),
            elapsed
        );

        Ok(files_info.len())
    }

    fn should_skip_dir(&self, entry: &DirEntry, config: &AppConfig) -> bool {
        if let Some(name) = entry.file_name().to_str() {
            config.should_skip_dir(name)
        } else {
            false
        }
    }

    fn calculate_sample_hash_static(path: &Path, sample_size: usize) -> io::Result<String> {
        let mut file = File::open(path)?;
        let file_size = file.metadata()?.len();

        let mut hasher = Sha256::new();
        let sample_size = sample_size.min(file_size as usize / 3);

        if sample_size == 0 {
            return Ok("".to_string());
        }

        // 读取文件开头
        let mut buffer = vec![0; sample_size];
        file.read_exact(&mut buffer)?;
        hasher.update(&buffer);

        // 读取文件中间
        if file_size > sample_size as u64 * 2 {
            file.seek(SeekFrom::Start(file_size / 2))?;
            file.read_exact(&mut buffer)?;
            hasher.update(&buffer);
        }

        // 读取文件末尾
        if file_size > sample_size as u64 {
            file.seek(SeekFrom::End(-(sample_size as i64)))?;
            file.read_exact(&mut buffer)?;
            hasher.update(&buffer);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn find_duplicates(&mut self, use_hash: bool) -> io::Result<Vec<DuplicateGroup>> {
        println!("开始查找重复文件...");
        let start_time = Instant::now();

        let query = if use_hash {
            "SELECT size, sample_hash, path, disk_id
             FROM files
             WHERE sample_hash IS NOT NULL
             ORDER BY size DESC, sample_hash ASC, disk_id ASC, path ASC"
        } else {
            "SELECT size, NULL as sample_hash, path, disk_id
             FROM files
             ORDER BY size DESC, disk_id ASC, path ASC"
        };

        let mut stmt = self
            .conn
            .prepare(query)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("SQL准备失败: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("查询执行失败: {}", e)))?;

        let mut grouped: HashMap<(u64, Option<String>), Vec<IndexedFile>> = HashMap::new();
        for row in rows {
            let (size, sample_hash, path, disk_id) = row.map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("读取查询结果失败: {}", e))
            })?;
            grouped
                .entry((size, sample_hash.clone()))
                .or_default()
                .push(IndexedFile {
                    path,
                    size,
                    sample_hash,
                    disk_id,
                });
        }

        let mut duplicates: Vec<DuplicateGroup> = grouped
            .into_iter()
            .filter_map(|((size, _), files)| {
                if files.len() > 1 {
                    Some(DuplicateGroup { size, files })
                } else {
                    None
                }
            })
            .collect();
        duplicates.sort_by(|a, b| {
            b.size
                .cmp(&a.size)
                .then_with(|| a.files[0].disk_id.cmp(&b.files[0].disk_id))
                .then_with(|| a.files[0].path.cmp(&b.files[0].path))
        });

        let elapsed = start_time.elapsed();
        println!(
            "重复文件查找完成，找到 {} 组重复文件，耗时: {:.2?}",
            duplicates.len(),
            elapsed
        );

        Ok(duplicates)
    }

    pub fn get_statistics(&mut self) -> io::Result<(i64, i64, i64)> {
        let file_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap_or(0);

        let total_size: i64 = self
            .conn
            .query_row("SELECT SUM(size) FROM files", [], |row| row.get(0))
            .unwrap_or(0);

        let disk_count: i64 = self
            .conn
            .query_row("SELECT COUNT(DISTINCT disk_id) FROM files", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        Ok((file_count, total_size, disk_count))
    }

    pub fn delete_file_record(&mut self, file_path: &str) -> io::Result<()> {
        self.conn
            .execute("DELETE FROM files WHERE path = ?", params![file_path])
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("删除记录失败: {}", e)))?;
        Ok(())
    }

    pub fn delete_file_record_for_disk(
        &mut self,
        disk_id: &str,
        file_path: &str,
    ) -> io::Result<()> {
        self.conn
            .execute(
                "DELETE FROM files WHERE disk_id = ? AND path = ?",
                params![disk_id, file_path],
            )
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("删除记录失败: {}", e)))?;
        Ok(())
    }

    pub fn list_disks(&mut self) -> io::Result<Vec<DiskSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT disk_id, display_name, root_path, last_scanned_at, file_count, total_size
                 FROM disks
                 ORDER BY last_scanned_at DESC, disk_id ASC",
            )
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("SQL准备失败: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(DiskSummary {
                    disk_id: row.get(0)?,
                    display_name: row.get(1)?,
                    root_path: row.get(2)?,
                    last_scanned_at: row.get(3)?,
                    file_count: row.get(4)?,
                    total_size: row.get(5)?,
                })
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("查询执行失败: {}", e)))?;

        let mut disks = Vec::new();
        for row in rows {
            disks.push(row.map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("读取查询结果失败: {}", e))
            })?);
        }

        Ok(disks)
    }

    pub fn list_files_for_disk(&mut self, disk_id: &str) -> io::Result<Vec<IndexedFile>> {
        let disk_id = sanitize_disk_id(disk_id);
        let mut stmt = self
            .conn
            .prepare(
                "SELECT path, size, sample_hash, disk_id
                 FROM files
                 WHERE disk_id = ?
                 ORDER BY size DESC, path ASC",
            )
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("SQL准备失败: {}", e)))?;

        let rows = stmt
            .query_map(params![disk_id], |row| {
                Ok(IndexedFile {
                    path: row.get(0)?,
                    size: row.get(1)?,
                    sample_hash: row.get(2)?,
                    disk_id: row.get(3)?,
                })
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("查询执行失败: {}", e)))?;

        let mut files = Vec::new();
        for row in rows {
            files.push(row.map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("读取查询结果失败: {}", e))
            })?);
        }

        Ok(files)
    }

    pub fn clear_disk(&mut self, disk_id: &str) -> io::Result<()> {
        let disk_id = sanitize_disk_id(disk_id);
        let tx = self
            .conn
            .transaction()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("事务启动失败: {}", e)))?;

        tx.execute("DELETE FROM files WHERE disk_id = ?", params![disk_id])
            .map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("文件索引清除失败: {}", e))
            })?;
        tx.execute("DELETE FROM disks WHERE disk_id = ?", params![disk_id])
            .map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("磁盘记录清除失败: {}", e))
            })?;
        tx.commit()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("事务提交失败: {}", e)))?;

        Ok(())
    }

    #[cfg(test)]
    fn insert_file_info_for_test(&mut self, file_info: &FileInfo) -> io::Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO files (path, size, sample_hash, disk_id) VALUES (?, ?, ?, ?)",
                params![file_info.path, file_info.size, file_info.sample_hash, file_info.disk_id],
            )
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("测试数据插入失败: {}", e)))?;
        Ok(())
    }

    #[cfg(test)]
    fn upsert_disk_summary_for_test(
        &mut self,
        disk_id: &str,
        root_path: &str,
        file_count: i64,
        total_size: i64,
    ) -> io::Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO disks
                 (disk_id, display_name, root_path, last_scanned_at, file_count, total_size)
                 VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    disk_id,
                    disk_id,
                    root_path,
                    end_timestamp(),
                    file_count,
                    total_size
                ],
            )
            .map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("测试磁盘数据插入失败: {}", e))
            })?;
        Ok(())
    }
}

pub fn sanitize_disk_id(value: &str) -> String {
    let sanitized: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    if sanitized.is_empty() {
        "disk".to_string()
    } else {
        sanitized
    }
}

pub fn default_disk_id_for_path(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .or_else(|| {
            path.components()
                .next_back()
                .and_then(|component| component.as_os_str().to_str())
        })
        .unwrap_or("disk");
    sanitize_disk_id(name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InferredDiskRoot {
    disk_id: String,
    root_path: String,
}

fn infer_common_disk_root(paths: &[String]) -> Option<InferredDiskRoot> {
    let mut inferred = paths
        .iter()
        .filter_map(|path| infer_disk_root_from_path(path));
    let first = inferred.next()?;

    if inferred.all(|item| item.root_path == first.root_path) {
        Some(first)
    } else {
        None
    }
}

fn infer_disk_root_from_path(path: &str) -> Option<InferredDiskRoot> {
    let components = normal_path_components(path);
    if components.is_empty() {
        return None;
    }

    let (root_path, disk_name) = if components
        .first()
        .is_some_and(|component| component == "Volumes")
        && components.len() >= 2
    {
        (format!("/Volumes/{}", components[1]), components[1].clone())
    } else if components
        .first()
        .is_some_and(|component| component == "Users")
        && components.len() >= 3
    {
        (
            format!("/Users/{}/{}", components[1], components[2]),
            components[2].clone(),
        )
    } else if Path::new(path).is_absolute() && components.len() >= 2 {
        (format!("/{}", components[0]), components[0].clone())
    } else {
        return None;
    };

    Some(InferredDiskRoot {
        disk_id: sanitize_disk_id(&disk_name),
        root_path,
    })
}

fn normal_path_components(path: &str) -> Vec<String> {
    Path::new(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(ToOwned::to_owned),
            _ => None,
        })
        .collect()
}

fn is_legacy_placeholder_id(disk_id: &str) -> bool {
    matches!(disk_id, "main" | "disk")
}

fn default_database_path() -> io::Result<PathBuf> {
    let base_dir = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .or_else(dirs::home_dir)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法找到用户数据目录"))?;
    let db_dir = base_dir.join("FindDuplFile");
    let db_path = db_dir.join(DISK_DB_PATH);
    migrate_legacy_database_path(Path::new(DISK_DB_PATH), &db_path)?;
    Ok(db_path)
}

fn migrate_legacy_database_path(legacy_path: &Path, db_path: &Path) -> io::Result<()> {
    if db_path.exists() || !legacy_path.exists() {
        return Ok(());
    }

    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(legacy_path, db_path)?;
    Ok(())
}

fn end_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn duplicate_lookup_preserves_paths_containing_delimiters() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("files.db");
        let mut scanner = FileScanner::with_database_path(&db_path).unwrap();

        scanner
            .insert_file_info_for_test(&FileInfo {
                path: "/tmp/a|pipe/video.mp4".into(),
                size: 42,
                sample_hash: Some("same".into()),
                disk_id: "disk-a".into(),
            })
            .unwrap();
        scanner
            .insert_file_info_for_test(&FileInfo {
                path: "/tmp/plain/video.mp4".into(),
                size: 42,
                sample_hash: Some("same".into()),
                disk_id: "disk-b".into(),
            })
            .unwrap();

        let groups = scanner.find_duplicates(true).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0]
                .files
                .iter()
                .map(|file| (file.disk_id.as_str(), file.path.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("disk-a", "/tmp/a|pipe/video.mp4"),
                ("disk-b", "/tmp/plain/video.mp4")
            ],
        );
    }

    #[test]
    fn default_disk_id_uses_folder_name_without_spaces() {
        assert_eq!(
            default_disk_id_for_path(Path::new("/Volumes/Backup Drive")),
            "BackupDrive"
        );
        assert_eq!(sanitize_disk_id("  A Disk  "), "ADisk");
        assert_eq!(sanitize_disk_id("   "), "disk");
    }

    #[test]
    fn disk_summaries_and_clear_are_scoped_by_disk_id() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("files.db");
        let mut scanner = FileScanner::with_database_path(&db_path).unwrap();

        for file in [
            FileInfo {
                path: "/Volumes/A/123.MP4".into(),
                size: 100,
                sample_hash: Some("same".into()),
                disk_id: "A".into(),
            },
            FileInfo {
                path: "/Volumes/C/233.MP4".into(),
                size: 100,
                sample_hash: Some("same".into()),
                disk_id: "C".into(),
            },
            FileInfo {
                path: "/Volumes/C/unique.MP4".into(),
                size: 200,
                sample_hash: Some("unique".into()),
                disk_id: "C".into(),
            },
        ] {
            scanner.insert_file_info_for_test(&file).unwrap();
        }
        scanner
            .upsert_disk_summary_for_test("A", "/Volumes/A", 1, 100)
            .unwrap();
        scanner
            .upsert_disk_summary_for_test("C", "/Volumes/C", 2, 300)
            .unwrap();

        let duplicates = scanner.find_duplicates(true).unwrap();
        assert_eq!(duplicates.len(), 1);
        assert_eq!(
            duplicates[0]
                .files
                .iter()
                .map(|file| (file.disk_id.as_str(), file.path.as_str()))
                .collect::<Vec<_>>(),
            vec![("A", "/Volumes/A/123.MP4"), ("C", "/Volumes/C/233.MP4")]
        );

        let disks = scanner.list_disks().unwrap();
        assert_eq!(disks.len(), 2);
        assert!(
            disks
                .iter()
                .any(|disk| disk.disk_id == "A" && disk.file_count == 1)
        );
        assert!(
            disks
                .iter()
                .any(|disk| disk.disk_id == "C" && disk.file_count == 2)
        );

        scanner.clear_disk("A").unwrap();
        assert_eq!(scanner.list_disks().unwrap().len(), 1);
        assert!(scanner.find_duplicates(true).unwrap().is_empty());
    }

    #[test]
    fn lists_all_files_for_a_disk_without_other_disk_records() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("files.db");
        let mut scanner = FileScanner::with_database_path(&db_path).unwrap();

        for file in [
            FileInfo {
                path: "/Volumes/A/todo-mac资料中转/box/file/900万河流水系版.mp4".into(),
                size: 300,
                sample_hash: Some("hash-a".into()),
                disk_id: "A".into(),
            },
            FileInfo {
                path: "/Volumes/A/another.mp4".into(),
                size: 100,
                sample_hash: Some("hash-b".into()),
                disk_id: "A".into(),
            },
            FileInfo {
                path: "/Volumes/C/233.MP4".into(),
                size: 200,
                sample_hash: Some("hash-c".into()),
                disk_id: "C".into(),
            },
        ] {
            scanner.insert_file_info_for_test(&file).unwrap();
        }

        let files = scanner.list_files_for_disk("A").unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].disk_id, "A");
        assert_eq!(
            files[0].path,
            "/Volumes/A/todo-mac资料中转/box/file/900万河流水系版.mp4"
        );
        assert_eq!(files[0].size, 300);
        assert!(files.iter().all(|file| file.disk_id == "A"));
    }

    #[test]
    fn repairs_legacy_main_disk_id_from_recorded_paths() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("files.db");

        {
            let mut scanner = FileScanner::with_database_path(&db_path).unwrap();
            scanner
                .insert_file_info_for_test(&FileInfo {
                    path: "/Users/mac/Downloads/course/video-01.mp4".into(),
                    size: 100,
                    sample_hash: Some("same".into()),
                    disk_id: "main".into(),
                })
                .unwrap();
            scanner
                .insert_file_info_for_test(&FileInfo {
                    path: "/Users/mac/Downloads/course/video-02.mp4".into(),
                    size: 200,
                    sample_hash: Some("other".into()),
                    disk_id: "main".into(),
                })
                .unwrap();
            scanner
                .upsert_disk_summary_for_test("main", "", 2, 300)
                .unwrap();
        }

        let mut scanner = FileScanner::with_database_path(&db_path).unwrap();
        let disks = scanner.list_disks().unwrap();

        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].disk_id, "Downloads");
        assert_eq!(disks[0].root_path, "/Users/mac/Downloads");
        assert_eq!(disks[0].file_count, 2);
        assert_eq!(scanner.list_files_for_disk("Downloads").unwrap().len(), 2);
        assert!(scanner.list_files_for_disk("main").unwrap().is_empty());
    }

    #[test]
    fn delete_mode_defaults_to_trash_and_persists_in_database() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("files.db");

        {
            let mut scanner = FileScanner::with_database_path(&db_path).unwrap();
            assert_eq!(scanner.get_delete_mode().unwrap(), DeleteMode::MoveToTrash);
            scanner.set_delete_mode(DeleteMode::DirectRemove).unwrap();
        }

        let scanner = FileScanner::with_database_path(&db_path).unwrap();
        assert_eq!(scanner.get_delete_mode().unwrap(), DeleteMode::DirectRemove);
    }

    #[test]
    fn app_config_persists_in_database() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("files.db");
        let config = AppConfig {
            min_file_size: 512 * 1024 * 1024,
            file_extensions: vec!["mp4".into(), "mov".into()],
            ignore_extensions: vec!["tmp".into(), "part".into()],
            ignore_dirs: vec!["target".into(), "cache".into()],
            use_hash: true,
            sample_size: 8 * 1024 * 1024,
            ..AppConfig::default()
        };

        {
            let mut scanner = FileScanner::with_database_path(&db_path).unwrap();
            scanner.set_app_config(&config).unwrap();
        }

        let scanner = FileScanner::with_database_path(&db_path).unwrap();
        let loaded = scanner.get_app_config().unwrap();
        assert_eq!(loaded.min_file_size, config.min_file_size);
        assert_eq!(loaded.file_extensions, config.file_extensions);
        assert_eq!(loaded.ignore_extensions, config.ignore_extensions);
        assert_eq!(loaded.ignore_dirs, config.ignore_dirs);
        assert_eq!(loaded.use_hash, config.use_hash);
        assert_eq!(loaded.ui_language, config.ui_language);
        assert_eq!(loaded.sample_size, config.sample_size);
    }

    #[test]
    fn migrates_legacy_database_without_overwriting_existing_target() {
        let temp_dir = tempdir().unwrap();
        let legacy_path = temp_dir.path().join("disk_files.db");
        let target_path = temp_dir
            .path()
            .join("Application Support/FindDuplFile/disk_files.db");
        fs::write(&legacy_path, b"legacy").unwrap();

        migrate_legacy_database_path(&legacy_path, &target_path).unwrap();
        assert_eq!(fs::read(&target_path).unwrap(), b"legacy");

        fs::write(&legacy_path, b"changed").unwrap();
        migrate_legacy_database_path(&legacy_path, &target_path).unwrap();
        assert_eq!(fs::read(&target_path).unwrap(), b"legacy");
    }
}
