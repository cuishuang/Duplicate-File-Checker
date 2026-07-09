use crate::core::DuplicateGroup;
use std::fs;
use std::io;
use std::path::Path;

pub const DEFAULT_REPORT_PATH: &str = "duplicate_files_report.txt";

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

pub fn build_report(groups: &[DuplicateGroup]) -> String {
    let mut report = String::new();
    report.push_str("=== 重复文件报告 ===\n\n");

    let mut sorted_groups = groups.to_vec();
    sorted_groups.sort_by(|a, b| b.size.cmp(&a.size));

    let mut total_duplicates = 0usize;
    let mut total_size_saved = 0u64;

    for (idx, group) in sorted_groups.iter().enumerate() {
        if group.files.len() < 2 {
            continue;
        }

        let duplicate_count = group.files.len() - 1;
        let size_saved = group.size * duplicate_count as u64;
        total_duplicates += duplicate_count;
        total_size_saved += size_saved;

        report.push_str(&format!("重复组 #{}\n", idx + 1));
        report.push_str(&format!(
            "文件大小: {} ({})\n",
            group.size,
            format_bytes(group.size)
        ));
        report.push_str(&format!("文件数量: {}\n", group.files.len()));
        report.push_str(&format!(
            "可节省空间: {} ({})\n",
            size_saved,
            format_bytes(size_saved)
        ));
        report.push_str("文件路径:\n");

        for file in &group.files {
            report.push_str(&format!("  - [{}] {}\n", file.disk_id, file.path));
        }

        report.push_str("\n---\n\n");
    }

    report.push_str("总结:\n");
    report.push_str(&format!(
        "重复文件组: {} 组\n",
        sorted_groups.iter().filter(|g| g.files.len() > 1).count()
    ));
    report.push_str(&format!("可删除文件数: {} 个\n", total_duplicates));
    report.push_str(&format!(
        "可节省空间: {} ({})\n",
        total_size_saved,
        format_bytes(total_size_saved)
    ));

    report
}

pub fn export_report(groups: &[DuplicateGroup], path: impl AsRef<Path>) -> io::Result<()> {
    fs::write(path, build_report(groups))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::IndexedFile;

    fn file(disk_id: &str, path: &str, size: u64) -> IndexedFile {
        IndexedFile {
            path: path.into(),
            size,
            sample_hash: None,
            disk_id: disk_id.into(),
        }
    }

    #[test]
    fn formats_bytes_with_human_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn report_sorts_groups_and_summarizes_savings() {
        let groups = vec![
            DuplicateGroup {
                size: 10,
                files: vec![file("A", "/tmp/a", 10), file("B", "/tmp/b", 10)],
            },
            DuplicateGroup {
                size: 2048,
                files: vec![
                    file("A", "/tmp/c", 2048),
                    file("C", "/tmp/d", 2048),
                    file("D", "/tmp/e", 2048),
                ],
            },
        ];

        let report = build_report(&groups);

        assert!(report.contains("重复组 #1\n文件大小: 2048 (2.00 KB)"));
        assert!(report.contains("  - [C] /tmp/d"));
        assert!(report.contains("重复文件组: 2 组"));
        assert!(report.contains("可删除文件数: 3 个"));
        assert!(report.contains("可节省空间: 4106 (4.01 KB)"));
    }
}
