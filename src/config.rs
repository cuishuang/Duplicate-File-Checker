use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum UiLanguage {
    Zh,
    En,
}

impl Default for UiLanguage {
    fn default() -> Self {
        Self::Zh
    }
}

impl UiLanguage {
    pub fn toggled(self) -> Self {
        match self {
            Self::Zh => Self::En,
            Self::En => Self::Zh,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub min_file_size: u64,             // 最小文件大小（字节）
    pub file_extensions: Vec<String>,   // 要处理的文件扩展名
    pub ignore_extensions: Vec<String>, // 要忽略的文件扩展名
    pub ignore_dirs: Vec<String>,       // 要忽略的目录
    pub use_hash: bool,                 // 是否使用哈希值比较
    pub sample_size: usize,             // 采样大小
    #[serde(default)]
    pub ui_language: UiLanguage, // 界面语言
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            min_file_size: 100 * 1024 * 1024, // 默认100MB
            file_extensions: vec![
                "mp4".to_string(),
                "mkv".to_string(),
                "avi".to_string(),
                "mov".to_string(),
                "wmv".to_string(),
                "flv".to_string(),
                "webm".to_string(),
                "m4v".to_string(),
                "mpg".to_string(),
                "mpeg".to_string(),
                "jpeg".to_string(),
                "jpg".to_string(),
                "png".to_string(),
                "gif".to_string(),
                "bmp".to_string(),
                "tiff".to_string(),
                "webp".to_string(),
            ],
            ignore_extensions: vec![],
            ignore_dirs: vec![
                ".git".to_string(),
                ".svn".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
            ],
            use_hash: false,
            sample_size: 4 * 1024 * 1024, // 4MB
            ui_language: UiLanguage::Zh,
        }
    }
}

impl AppConfig {
    pub fn should_process_file(&self, path: &std::path::Path, size: u64) -> bool {
        // 检查文件大小
        if size < self.min_file_size {
            return false;
        }

        // 检查文件扩展名
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_lowercase();

            // 如果在忽略列表中，跳过
            if self.ignore_extensions.contains(&ext) {
                return false;
            }

            // 如果有指定扩展名列表，只处理列表中的文件
            if !self.file_extensions.is_empty() {
                return self.file_extensions.contains(&ext);
            }
        }

        true
    }

    pub fn should_skip_dir(&self, dir_name: &str) -> bool {
        self.ignore_dirs.iter().any(|ignore| dir_name == ignore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn rejects_too_small_or_ignored_files() {
        let mut config = AppConfig {
            min_file_size: 1024,
            file_extensions: vec!["mp4".into(), "jpg".into()],
            ignore_extensions: vec!["tmp".into()],
            ..AppConfig::default()
        };

        assert!(!config.should_process_file(Path::new("clip.mp4"), 512));
        assert!(!config.should_process_file(Path::new("cache.tmp"), 4096));
        assert!(!config.should_process_file(Path::new("notes.txt"), 4096));

        config.file_extensions.clear();
        assert!(config.should_process_file(Path::new("archive.bin"), 4096));
    }

    #[test]
    fn skips_exact_directory_names_only() {
        let config = AppConfig::default();

        assert!(config.should_skip_dir(".git"));
        assert!(config.should_skip_dir("node_modules"));
        assert!(!config.should_skip_dir("node_modules_backup"));
    }

    #[test]
    fn ui_language_defaults_to_chinese_for_legacy_config() {
        let config: AppConfig = serde_json::from_str(
            r#"{
                "min_file_size": 1024,
                "file_extensions": [],
                "ignore_extensions": [],
                "ignore_dirs": [],
                "use_hash": false,
                "sample_size": 4096
            }"#,
        )
        .unwrap();

        assert_eq!(config.ui_language, UiLanguage::Zh);
    }
}
