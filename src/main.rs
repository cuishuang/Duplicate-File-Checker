mod gui;

use find_dupl_file::config::AppConfig;
use find_dupl_file::core::{FileScanner, default_disk_id_for_path};
use find_dupl_file::report;
use gui::DuplicateFinderApp;
use std::env;
use std::io;
use std::path::Path;
use std::time::Instant;

fn run_gui() -> Result<(), eframe::Error> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([800.0, 600.0]);

    if let Some(icon) = app_icon_data() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "重复文件清理器",
        options,
        Box::new(|cc| {
            // 设置中文字体
            setup_custom_fonts(&cc.egui_ctx);
            Ok(Box::new(DuplicateFinderApp::default()))
        }),
    )
}

fn app_icon_data() -> Option<egui::IconData> {
    let bytes = include_bytes!("../assets/theme-toggle.png");
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 添加系统中文字体
    #[cfg(target_os = "macos")]
    {
        // macOS 系统字体
        if let Ok(font_data) = std::fs::read("/System/Library/Fonts/PingFang.ttc") {
            fonts
                .font_data
                .insert("PingFang".to_owned(), egui::FontData::from_owned(font_data));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "PingFang".to_owned());
        } else if let Ok(font_data) = std::fs::read("/System/Library/Fonts/STHeiti Light.ttc") {
            fonts
                .font_data
                .insert("STHeiti".to_owned(), egui::FontData::from_owned(font_data));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "STHeiti".to_owned());
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows 系统字体
        if let Ok(font_data) = std::fs::read("C:/Windows/Fonts/msyh.ttc") {
            fonts.font_data.insert(
                "Microsoft YaHei".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "Microsoft YaHei".to_owned());
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Linux 系统字体
        if let Ok(font_data) = std::fs::read("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc") {
            fonts.font_data.insert(
                "WenQuanYi Micro Hei".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "WenQuanYi Micro Hei".to_owned());
        }
    }

    ctx.set_fonts(fonts);

    // 设置自定义主题
    setup_custom_theme(ctx);
}

fn setup_custom_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = egui::Color32::from_rgb(246, 248, 252);
    style.visuals.window_fill = egui::Color32::from_rgb(255, 255, 255);
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(241, 245, 249);
    style.visuals.faint_bg_color = egui::Color32::from_rgb(248, 250, 252);

    style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(255, 255, 255);
    style.visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(218, 226, 236));
    style.visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(31, 41, 55));

    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(255, 255, 255);
    style.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(203, 213, 225));
    style.visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85));

    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(241, 245, 249);
    style.visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(148, 163, 184));
    style.visuals.widgets.hovered.fg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(15, 23, 42));

    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(225, 237, 255);
    style.visuals.widgets.active.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(37, 99, 235));
    style.visuals.widgets.active.fg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(29, 78, 216));

    // 选择框颜色
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(219, 234, 254);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(37, 99, 235));

    // 超链接颜色
    style.visuals.hyperlink_color = egui::Color32::from_rgb(37, 99, 235);

    // 窗口圆角
    style.visuals.window_rounding = egui::Rounding::same(8.0);
    style.visuals.menu_rounding = egui::Rounding::same(6.0);

    // 按钮圆角
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(6.0);

    // 间距
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.menu_margin = egui::Margin::same(8.0);
    style.spacing.indent = 20.0;

    ctx.set_style(style);
}

fn run_cli() -> io::Result<()> {
    let mut scanner = FileScanner::new();
    let config = AppConfig::default();

    println!("欢迎使用重复文件查找器 (命令行模式)！");
    println!("连接硬盘后，输入硬盘路径和标识符进行扫描。");

    loop {
        println!("\n请输入命令：");
        println!("  scan [硬盘路径] [硬盘标识符可选] - 扫描硬盘");
        println!("  find - 查找所有重复文件");
        println!("  export - 导出重复文件报告");
        println!("  stats - 显示数据库统计信息");
        println!("  disks - 显示已记录的磁盘索引");
        println!("  files [硬盘标识符] - 显示某块磁盘的全部文件索引");
        println!("  clear [硬盘标识符] - 清除某块磁盘的索引");
        println!("  gui - 启动图形界面");
        println!("  exit - 退出程序");

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let args: Vec<&str> = input.trim().split_whitespace().collect();

        if args.is_empty() {
            continue;
        }

        match args[0] {
            "scan" => {
                if args.len() < 2 {
                    println!("用法: scan [硬盘路径] [硬盘标识符可选]");
                    continue;
                }

                let disk_path = Path::new(args[1]);
                let disk_id = args
                    .get(2)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| default_disk_id_for_path(disk_path));

                if !disk_path.exists() {
                    println!("错误: 路径不存在");
                    continue;
                }

                let started_at = Instant::now();
                match scanner.scan_disk(disk_path, &disk_id, &config) {
                    Ok(count) => println!(
                        "硬盘 {} 扫描完成，处理了 {} 个文件，耗时 {:.2?}",
                        disk_id,
                        count,
                        started_at.elapsed()
                    ),
                    Err(e) => println!("扫描出错: {}", e),
                }
            }
            "find" => match scanner.find_duplicates(config.use_hash) {
                Ok(duplicates) => {
                    println!("找到 {} 组重复文件", duplicates.len());
                    println!("运行 'export' 命令导出详细报告");
                }
                Err(e) => println!("查找重复文件出错: {}", e),
            },
            "export" => match scanner.find_duplicates(config.use_hash) {
                Ok(duplicates) => {
                    match report::export_report(&duplicates, report::DEFAULT_REPORT_PATH) {
                        Ok(_) => println!("重复文件报告已导出"),
                        Err(e) => println!("导出报告出错: {}", e),
                    }
                }
                Err(e) => println!("查找重复文件出错: {}", e),
            },
            "stats" => match scanner.get_statistics() {
                Ok((file_count, total_size, disk_count)) => {
                    println!("数据库统计信息:");
                    println!("  已扫描硬盘数: {}", disk_count);
                    println!("  记录文件总数: {}", file_count);
                    println!(
                        "  记录文件总大小: {:.2} GB",
                        total_size as f64 / (1024.0 * 1024.0 * 1024.0)
                    );
                }
                Err(e) => println!("获取统计信息出错: {}", e),
            },
            "disks" => match scanner.list_disks() {
                Ok(disks) => {
                    if disks.is_empty() {
                        println!("还没有记录任何磁盘索引");
                    } else {
                        println!("已记录磁盘:");
                        for disk in disks {
                            println!(
                                "  {} - {} 个文件，{}，最后扫描: {}，路径: {}",
                                disk.disk_id,
                                disk.file_count,
                                report::format_bytes(disk.total_size.max(0) as u64),
                                disk.last_scanned_at,
                                disk.root_path
                            );
                        }
                    }
                }
                Err(e) => println!("读取磁盘索引出错: {}", e),
            },
            "files" => {
                if args.len() < 2 {
                    println!("用法: files [硬盘标识符]");
                    continue;
                }
                match scanner.list_files_for_disk(args[1]) {
                    Ok(files) => {
                        if files.is_empty() {
                            println!("硬盘 {} 没有文件索引", args[1]);
                        } else {
                            println!("硬盘 {} 的文件索引:", args[1]);
                            for file in files {
                                println!(
                                    "  [{}] {}  {}",
                                    file.disk_id,
                                    report::format_bytes(file.size),
                                    file.path
                                );
                            }
                        }
                    }
                    Err(e) => println!("读取文件索引出错: {}", e),
                }
            }
            "clear" => {
                if args.len() < 2 {
                    println!("用法: clear [硬盘标识符]");
                    continue;
                }
                match scanner.clear_disk(args[1]) {
                    Ok(()) => println!("已清除硬盘 {} 的索引", args[1]),
                    Err(e) => println!("清除磁盘索引出错: {}", e),
                }
            }
            "gui" => {
                println!("启动图形界面...");
                if let Err(e) = run_gui() {
                    println!("启动图形界面失败: {}", e);
                }
                return Ok(());
            }
            "exit" => break,
            _ => println!("未知命令"),
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    // 检查是否有 --gui 参数或者没有参数时默认启动 GUI
    if args.len() == 1 || (args.len() > 1 && args[1] == "--gui") {
        println!("启动图形界面...");
        if let Err(e) = run_gui() {
            eprintln!("启动图形界面失败: {}", e);
            eprintln!("回退到命令行模式...");
            run_cli()
        } else {
            Ok(())
        }
    } else if args.len() > 1 && args[1] == "--cli" {
        run_cli()
    } else {
        println!("用法:");
        println!("  {} --gui    启动图形界面 (默认)", args[0]);
        println!("  {} --cli    启动命令行模式", args[0]);
        Ok(())
    }
}
