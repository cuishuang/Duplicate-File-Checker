# Duplicate File Checker / 重复文件清理器

一个用 Rust 编写的本地重复文件查找器，提供原生图形界面和命令行模式。它适合清理视频、图片、备份盘和外接硬盘中的重复文件，文件索引会保存在本地 SQLite 数据库中。

A local duplicate file checker written in Rust, with a native desktop GUI and a CLI mode. It is designed for cleaning duplicate videos, photos, backup folders, and external drives. File indexes are stored locally in SQLite.

## 功能亮点 / Highlights

- 原生桌面 GUI：基于 `egui/eframe`，支持中文和英文界面。
- Native desktop GUI: built with `egui/eframe`, with Chinese and English UI support.
- 多磁盘索引：移动硬盘不需要同时连接，先后扫描 A/B/C/D 后仍可跨磁盘查重。
- Multi-disk indexing: external drives do not need to be connected at the same time. Scan drives A/B/C/D over time and compare them later.
- 两种匹配策略：按文件大小快速匹配，或按文件大小 + 采样 SHA-256 哈希精确匹配。
- Two matching modes: fast size matching, or size + sampled SHA-256 hash matching for better accuracy.
- 可配置过滤：最小文件大小、处理扩展名、忽略扩展名、忽略目录。
- Configurable filters: minimum file size, included extensions, ignored extensions, and ignored folders.
- 安全清理：默认移到废纸篓，删除前必须手动勾选并确认。
- Safer cleanup: files are moved to Trash by default, and deletion requires manual selection and confirmation.
- 磁盘管理：查看每块磁盘的文件数量、占用空间、最后扫描时间，并可按磁盘标识移除索引。
- Disk management: view file count, indexed size, last scan time, and remove indexes by disk ID.
- 报告导出：可导出当前重复文件报告。
- Report export: export the current duplicate file report.

## 截图 / Screenshots

当前版本是原生桌面应用。运行 `cargo run -- --gui` 即可查看最新界面。

This is a native desktop app. Run `cargo run -- --gui` to view the latest interface.

## 安装 / Installation

要求 / Requirements:

- Rust 1.85+（项目使用 Rust 2024 edition）
- Rust 1.85+ (this project uses Rust 2024 edition)
- macOS、Windows 或 Linux
- macOS, Windows, or Linux

```bash
git clone <your-fork-or-repo-url>
cd find-dupl-file
cargo build --release
```

运行 GUI / Run GUI:

```bash
cargo run -- --gui
# 或直接运行 release 产物 / or run the release binary directly
./target/release/find-dupl-file --gui
```

运行 CLI / Run CLI:

```bash
cargo run -- --cli
```

## GUI 使用流程 / GUI Workflow

1. 选择扫描目录，也可以使用桌面、下载、文档快捷入口。
2. 设置硬盘标识。默认会使用所选目录或磁盘名称，并自动去掉空格，例如 `Backup Drive` 会变成 `BackupDrive`。
3. 选择比较模式：按大小快速匹配，或按采样哈希精确匹配。
4. 点击“开始扫描”写入文件索引。
5. 点击“查找重复文件”查看重复组。
6. 勾选要删除的副本。结果会显示文件来自哪块磁盘，例如 `[A] 123.MP4` 和 `[C] 233.MP4`。
7. 对不想处理的重复组，可以点击“本次忽略”，该组不会参与本轮选择和删除。
8. 点击“删除已选文件”并确认。
9. 需要留档时点击“导出报告”。

1. Choose a folder to scan, or use the Desktop, Downloads, and Documents shortcuts.
2. Set the disk ID. By default, the app uses the selected folder or disk name and removes spaces. For example, `Backup Drive` becomes `BackupDrive`.
3. Choose a comparison mode: fast size matching, or precise sampled hash matching.
4. Click `Start Scan` to write the file index.
5. Click `Find Duplicates` to view duplicate groups.
6. Select the copies you want to remove. Results show the source disk, such as `[A] 123.MP4` and `[C] 233.MP4`.
7. For duplicate groups you want to skip, click `Ignore`; the group will not be selected or deleted in this run.
8. Click `Delete` and confirm.
9. Click `Export` if you need a report.

## CLI 命令 / CLI Commands

进入 CLI 后可使用 / Available commands in CLI mode:

```text
scan [disk path] [optional disk ID]  Scan a folder and write records to the database
find                                Find duplicate files
export                              Export duplicate_files_report.txt
stats                               Show database statistics
disks                               Show recorded disk indexes
files [disk ID]                     Show all indexed files for a disk
clear [disk ID]                     Clear one disk index
gui                                 Switch to GUI mode
exit                                Exit
```

## 多移动硬盘工作流 / External Drive Workflow

这个应用的核心场景是：你有很多块移动硬盘，但它们无法同时连接电脑。

The core scenario is managing many external drives that cannot all be connected at the same time.

推荐流程 / Recommended workflow:

1. 连接移动硬盘 A，扫描它，保存为标识 `A`。
2. 拔掉 A，连接移动硬盘 B，扫描它，保存为标识 `B`。
3. 继续扫描 C、D 等硬盘。
4. 在任意时刻点击“查找重复文件”，应用会基于本地数据库跨磁盘比较文件。
5. 如果发现 `[A] /Movies/123.MP4` 和 `[C] /Videos/233.MP4` 重复，你可以决定下次连接哪块盘进行清理。
6. 在“磁盘索引”中点击“查看”，可以按硬盘标识查看这块盘曾经扫描记录下来的全部文件信息。
7. 如果某块盘重新整理过，可以在“磁盘索引”里移除它，再重新扫描。

1. Connect external drive A, scan it, and save it as disk ID `A`.
2. Disconnect A, connect external drive B, scan it, and save it as disk ID `B`.
3. Continue with drives C, D, and so on.
4. At any time, click `Find Duplicates`; the app compares files across all indexed disks in the local database.
5. If `[A] /Movies/123.MP4` and `[C] /Videos/233.MP4` are duplicates, you can decide which drive to connect next for cleanup.
6. In `Disk Index`, click `View` to browse all recorded files for a disk ID.
7. If a drive has been reorganized, remove its index and scan it again.

注意：如果某块盘当前没有连接，应用仍能展示它的历史索引，但无法直接删除那块盘上的真实文件。

Note: if a drive is not currently connected, the app can still show its historical index, but it cannot delete real files from that drive.

## 配置说明 / Configuration

默认只处理较大的常见视频和图片文件，最小文件大小为 100 MB。可以在 GUI 的“设置”窗口中调整：

By default, the app only processes larger common video and image files, with a minimum file size of 100 MB. You can adjust these in the GUI `Settings` window:

- 最小文件大小 / Minimum file size
- 要处理的扩展名 / Included extensions
- 要忽略的扩展名 / Ignored extensions
- 要忽略的目录 / Ignored folders
- 是否使用采样哈希比较 / Whether to use sampled hash comparison
- 删除方式：移到废纸篓或直接删除 / Delete mode: move to Trash or remove directly
- 界面语言：中文或英文 / UI language: Chinese or English

采样哈希会读取文件头部、中部和尾部片段，比纯大小匹配更可靠，但扫描速度会慢一些。

Sampled hashing reads portions from the beginning, middle, and end of a file. It is more reliable than size-only matching, but scanning may be slower.

## 项目结构 / Project Structure

```text
src/
  config.rs   扫描过滤配置 / scan filter configuration
  core.rs     SQLite 索引、磁盘记录、扫描、重复分组、删除记录 / SQLite index, disk records, scanning, duplicate grouping, record removal
  gui.rs      egui 桌面界面与交互状态 / egui desktop UI and interaction state
  lib.rs      可复用库入口 / reusable library entry
  main.rs     CLI/GUI 二进制入口 / CLI/GUI binary entry
  report.rs   报告格式化和导出 / report formatting and export
```

## 开发 / Development

```bash
cargo fmt
cargo test
cargo check
cargo build
```

本地运行产生的 `disk_files.db`、`duplicate_files_report.txt` 和 `target/` 已在 `.gitignore` 中忽略。

Local runtime artifacts such as `disk_files.db`, `duplicate_files_report.txt`, and `target/` are ignored by `.gitignore`.

## macOS 打包 / macOS Packaging

可以生成一个不依赖 Rust 环境的 `.dmg`：

You can generate a `.dmg` that does not require Rust on the target machine:

```bash
./scripts/package-macos.sh
```

输出位置 / Output:

```text
target/macos-package-checker/Duplicate File Checker-0.1.0.dmg
```

把 `.dmg` 发给没有安装 Rust 的 Mac 用户即可。用户打开后，把 `Duplicate File Checker.app` 拖到 `Applications` 即可使用。如果没有 Apple Developer ID 证书，首次打开时可能需要在“系统设置 > 隐私与安全性”中允许。

Send the `.dmg` to Mac users who do not have Rust installed. After opening it, drag `Duplicate File Checker.app` into `Applications`. Without an Apple Developer ID certificate, users may need to allow the app in `System Settings > Privacy & Security` the first time they open it.

如果你有 Developer ID Application 证书，可以签名打包：

If you have a Developer ID Application certificate, you can sign the app while packaging:

```bash
DEVELOPER_ID_APP="Developer ID Application: Your Name (TEAMID)" \
./scripts/package-macos.sh
```

公开分发时建议在签名后再做 notarization。

For public distribution, notarization is recommended after signing.

## 安全提示 / Safety Notes

默认删除方式是移到废纸篓，但仍建议先导出报告并确认路径，尤其是在清理外接硬盘、备份盘或同步目录时。

The default delete mode moves files to Trash, but it is still recommended to export a report and verify paths first, especially when cleaning external drives, backup disks, or synced folders.

## 贡献 / Contributing

欢迎提交 Issue 和 Pull Request。开始前建议阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。

Issues and pull requests are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before getting started.

## 许可证 / License

MIT，详见 [LICENSE](LICENSE)。

MIT. See [LICENSE](LICENSE).
