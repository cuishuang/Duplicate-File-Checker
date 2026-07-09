use eframe::egui;
use find_dupl_file::config::{AppConfig, UiLanguage};
use find_dupl_file::core::{
    DeleteMode, DiskSummary, DuplicateGroup, FileScanner, IndexedFile, default_disk_id_for_path,
};
use find_dupl_file::report::{self, format_bytes};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Palette {
    bg: egui::Color32,
    sidebar_bg: egui::Color32,
    surface: egui::Color32,
    surface_2: egui::Color32,
    accent_soft: egui::Color32,
    border: egui::Color32,
    text: egui::Color32,
    muted: egui::Color32,
    blue: egui::Color32,
    green: egui::Color32,
    amber: egui::Color32,
    red: egui::Color32,
}

const LIGHT: Palette = Palette {
    bg: egui::Color32::from_rgb(245, 245, 247),
    sidebar_bg: egui::Color32::from_rgb(250, 250, 252),
    surface: egui::Color32::from_rgb(255, 255, 255),
    surface_2: egui::Color32::from_rgb(239, 242, 247),
    accent_soft: egui::Color32::from_rgb(229, 241, 255),
    border: egui::Color32::from_rgb(226, 231, 238),
    text: egui::Color32::from_rgb(29, 29, 31),
    muted: egui::Color32::from_rgb(106, 118, 138),
    blue: egui::Color32::from_rgb(0, 113, 227),
    green: egui::Color32::from_rgb(52, 199, 89),
    amber: egui::Color32::from_rgb(255, 149, 0),
    red: egui::Color32::from_rgb(255, 59, 48),
};

const DARK: Palette = Palette {
    bg: egui::Color32::from_rgb(15, 23, 42),
    sidebar_bg: egui::Color32::from_rgb(21, 30, 48),
    surface: egui::Color32::from_rgb(30, 41, 59),
    surface_2: egui::Color32::from_rgb(39, 53, 76),
    accent_soft: egui::Color32::from_rgb(31, 56, 104),
    border: egui::Color32::from_rgb(71, 85, 105),
    text: egui::Color32::from_rgb(241, 245, 249),
    muted: egui::Color32::from_rgb(148, 163, 184),
    blue: egui::Color32::from_rgb(96, 165, 250),
    green: egui::Color32::from_rgb(52, 211, 153),
    amber: egui::Color32::from_rgb(251, 191, 36),
    red: egui::Color32::from_rgb(248, 113, 113),
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppTheme {
    Light,
    Dark,
}

#[derive(Debug, Clone)]
pub enum ScanStatus {
    Ready,
    Scanning(String),
    Analyzing,
    Complete(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum AppMessage {
    ScanComplete(Result<ScanSummary, String>),
    DuplicatesFound(Result<Vec<DuplicateGroup>, String>),
}

#[derive(Debug, Clone)]
pub struct ScanSummary {
    count: usize,
    elapsed: Duration,
}

#[derive(Clone)]
struct SupportTextures {
    official_account: egui::TextureHandle,
    wechat_pay: egui::TextureHandle,
    alipay: egui::TextureHandle,
}

pub struct DuplicateFinderApp {
    theme: AppTheme,
    theme_icon: Option<egui::TextureHandle>,
    support_textures: Option<SupportTextures>,
    config: AppConfig,
    delete_mode: DeleteMode,
    scan_status: ScanStatus,
    duplicate_groups: Vec<DuplicateGroup>,
    duplicate_size_filter_mb: f64,
    disk_summaries: Vec<DiskSummary>,
    disk_summaries_loaded: bool,
    selected_disk_id: Option<String>,
    selected_disk_files: Vec<IndexedFile>,
    selected_files: HashMap<String, bool>,
    message_receiver: Receiver<AppMessage>,
    message_sender: Sender<AppMessage>,
    show_config: bool,
    show_support: bool,
    show_delete_confirm: bool,
    alert_message: Option<String>,
    new_extension: String,
    new_ignore_extension: String,
    new_ignore_dir: String,
    scan_path: String,
    disk_id: String,
    last_scan_count: usize,
    total_candidates: usize,
    total_duplicates: usize,
    potential_savings: u64,
    notice: String,
}

impl Default for DuplicateFinderApp {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        let config = load_app_config();
        let language = config.ui_language;
        Self {
            theme: AppTheme::Light,
            theme_icon: None,
            support_textures: None,
            config,
            delete_mode: load_delete_mode(),
            scan_status: ScanStatus::Ready,
            duplicate_groups: Vec::new(),
            duplicate_size_filter_mb: 0.0,
            disk_summaries: Vec::new(),
            disk_summaries_loaded: false,
            selected_disk_id: None,
            selected_disk_files: Vec::new(),
            selected_files: HashMap::new(),
            message_receiver: receiver,
            message_sender: sender,
            show_config: false,
            show_support: false,
            show_delete_confirm: false,
            alert_message: None,
            new_extension: String::new(),
            new_ignore_extension: String::new(),
            new_ignore_dir: String::new(),
            scan_path: default_scan_path(),
            disk_id: default_disk_id_for_path(&PathBuf::from(default_scan_path())),
            last_scan_count: 0,
            total_candidates: 0,
            total_duplicates: 0,
            potential_savings: 0,
            notice: tr(
                language,
                "选择目录后开始扫描，或直接分析已有数据库。",
                "Choose a folder to scan, or analyze the existing database.",
            )
            .to_string(),
        }
    }
}

impl eframe::App for DuplicateFinderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        if !self.disk_summaries_loaded {
            self.refresh_disk_summaries();
        }
        self.handle_messages();

        if self.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        self.show_top_bar(ctx);
        self.show_body(ctx);
        self.show_config_window(ctx);
        self.show_support_window(ctx);
        self.show_delete_window(ctx);
        self.show_alert_window(ctx);
    }
}

impl DuplicateFinderApp {
    fn palette(&self) -> Palette {
        match self.theme {
            AppTheme::Light => LIGHT,
            AppTheme::Dark => DARK,
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let p = self.palette();
        let mut style = (*ctx.style()).clone();
        style.visuals = if self.theme == AppTheme::Light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        style.visuals.panel_fill = p.bg;
        style.visuals.window_fill = p.surface;
        style.visuals.extreme_bg_color = p.bg;
        style.visuals.faint_bg_color = p.surface_2;
        style.visuals.widgets.noninteractive.bg_fill = p.surface;
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, p.border);
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, p.text);
        style.visuals.widgets.inactive.bg_fill = p.surface;
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, p.border);
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, p.text);
        style.visuals.widgets.hovered.bg_fill = p.surface_2;
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, p.blue);
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, p.text);
        style.visuals.widgets.active.bg_fill = p.accent_soft;
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, p.blue);
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, p.blue);
        style.visuals.selection.bg_fill = p.accent_soft;
        style.visuals.selection.stroke = egui::Stroke::new(1.0, p.blue);
        style.visuals.hyperlink_color = p.blue;
        style.visuals.window_shadow = egui::Shadow::NONE;
        style.visuals.popup_shadow = egui::Shadow::NONE;
        style.visuals.window_rounding = egui::Rounding::same(12.0);
        style.visuals.menu_rounding = egui::Rounding::same(10.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        ctx.set_style(style);
    }

    fn theme_icon(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        if self.theme_icon.is_none() {
            self.theme_icon = load_theme_icon(ctx);
        }
        self.theme_icon.as_ref()
    }

    fn support_textures(&mut self, ctx: &egui::Context) -> Option<SupportTextures> {
        if self.support_textures.is_none() {
            self.support_textures = load_support_textures(ctx);
        }
        self.support_textures.clone()
    }

    fn open_support_window(&mut self) {
        self.show_support = true;
    }

    fn language(&self) -> UiLanguage {
        self.config.ui_language
    }

    fn toggle_language(&mut self) {
        self.toggle_language_with_persistence(true);
    }

    fn toggle_language_with_persistence(&mut self, persist: bool) {
        self.config.ui_language = self.config.ui_language.toggled();
        if persist {
            self.save_app_config();
        }
        self.notice = tr(
            self.language(),
            "已切换为中文界面。",
            "Switched to English.",
        )
        .to_string();
    }

    fn language_toggle_button(&mut self, ui: &mut egui::Ui, p: Palette) {
        let tooltip = tr(self.language(), "Switch to English", "切换到中文");
        if ui
            .add_sized([42.0, 34.0], secondary_button("文/A", p))
            .on_hover_text(tooltip)
            .clicked()
        {
            self.toggle_language();
        }
    }

    fn theme_toggle_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, p: Palette) {
        let tooltip = match (self.theme, self.language()) {
            (AppTheme::Light, UiLanguage::Zh) => "切换到深色主题",
            (AppTheme::Dark, UiLanguage::Zh) => "切换到浅色主题",
            (AppTheme::Light, UiLanguage::En) => "Switch to dark theme",
            (AppTheme::Dark, UiLanguage::En) => "Switch to light theme",
        };

        if let Some(texture) = self.theme_icon(ctx).cloned() {
            let image = egui::Image::new((texture.id(), egui::vec2(30.0, 30.0)));
            let response = ui
                .add_sized(
                    [38.0, 34.0],
                    egui::ImageButton::new(image)
                        .rounding(egui::Rounding::same(9.0))
                        .frame(false),
                )
                .on_hover_text(tooltip);

            if response.clicked() {
                self.theme = match self.theme {
                    AppTheme::Light => AppTheme::Dark,
                    AppTheme::Dark => AppTheme::Light,
                };
            }
        } else if ui
            .add_sized(
                [38.0, 34.0],
                secondary_button(tr(self.language(), "主题", "Theme"), p),
            )
            .on_hover_text(tooltip)
            .clicked()
        {
            self.theme = match self.theme {
                AppTheme::Light => AppTheme::Dark,
                AppTheme::Dark => AppTheme::Light,
            };
        }
    }

    fn handle_messages(&mut self) {
        while let Ok(message) = self.message_receiver.try_recv() {
            let language = self.language();
            match message {
                AppMessage::ScanComplete(result) => match result {
                    Ok(summary) => {
                        self.last_scan_count = summary.count;
                        let message = scan_complete_message_for_language(&summary, language);
                        self.scan_status = ScanStatus::Complete(message.clone());
                        self.notice = tr_format(
                            language,
                            format!("{} 现在可以查找重复文件。", message),
                            format!("{} You can find duplicates now.", message),
                        );
                        self.refresh_disk_summaries();
                    }
                    Err(error) => {
                        self.scan_status = ScanStatus::Error(error.clone());
                        self.notice = tr_format(
                            language,
                            format!("扫描失败：{}", error),
                            format!("Scan failed: {}", error),
                        );
                    }
                },
                AppMessage::DuplicatesFound(result) => match result {
                    Ok(duplicates) => {
                        self.duplicate_groups = duplicates;
                        self.selected_files.clear();
                        self.calculate_statistics();
                        self.scan_status = ScanStatus::Complete(duplicate_group_found_message(
                            self.duplicate_groups.len(),
                            language,
                        ));
                        self.notice = if self.duplicate_groups.is_empty() {
                            tr(language, "没有发现重复文件。", "No duplicate files found.")
                                .to_string()
                        } else {
                            tr_format(
                                language,
                                format!(
                                    "发现 {} 个可删除副本，预计可释放 {}。",
                                    self.total_duplicates,
                                    format_bytes(self.potential_savings)
                                ),
                                format!(
                                    "Found {} removable copies, estimated savings {}.",
                                    self.total_duplicates,
                                    format_bytes(self.potential_savings)
                                ),
                            )
                        };
                    }
                    Err(error) => {
                        self.scan_status = ScanStatus::Error(error.clone());
                        self.notice = tr_format(
                            language,
                            format!("分析失败：{}", error),
                            format!("Analysis failed: {}", error),
                        );
                    }
                },
            }
        }
    }

    fn show_top_bar(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        let language = self.language();
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::none()
                    .fill(p.surface)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .inner_margin(egui::Margin::symmetric(24.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(tr(
                                language,
                                "重复文件清理器",
                                "Duplicate File Cleaner",
                            ))
                            .size(22.0)
                            .strong()
                            .color(p.text),
                        );
                        ui.label(
                            egui::RichText::new(tr(
                                language,
                                "本地扫描、精确分析、安全清理",
                                "Local scan, precise analysis, safe cleanup",
                            ))
                            .size(13.0)
                            .color(p.muted),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.theme_toggle_button(ui, ctx, p);
                        self.language_toggle_button(ui, p);

                        if ui
                            .add_sized(
                                [86.0, 34.0],
                                secondary_button(tr(language, "设置", "Settings"), p),
                            )
                            .clicked()
                        {
                            self.show_config = true;
                        }

                        if ui
                            .add_sized(
                                [122.0, 34.0],
                                primary_button(
                                    tr(language, "关注和赞助", "Follow & Donate"),
                                    p.blue,
                                ),
                            )
                            .clicked()
                        {
                            self.open_support_window();
                        }
                    });
                });
            });
    }

    fn show_body(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let sidebar_width = 348.0;
                let separator_width = 1.0;
                let sidebar_rect = egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.min.x + sidebar_width, rect.max.y),
                );
                let separator_rect = egui::Rect::from_min_max(
                    egui::pos2(sidebar_rect.max.x, rect.min.y),
                    egui::pos2(sidebar_rect.max.x + separator_width, rect.max.y),
                );
                let content_rect = egui::Rect::from_min_max(
                    egui::pos2(separator_rect.max.x, rect.min.y),
                    rect.max,
                );

                ui.painter().rect_filled(sidebar_rect, 0.0, p.sidebar_bg);
                ui.painter().rect_filled(separator_rect, 0.0, p.border);
                ui.painter().rect_filled(content_rect, 0.0, p.bg);

                ui.allocate_ui_at_rect(sidebar_rect, |ui| {
                    egui::Frame::none()
                        .fill(p.sidebar_bg)
                        .inner_margin(egui::Margin::same(18.0))
                        .show(ui, |ui| {
                            self.show_sidebar(ui, p);
                        });
                });

                ui.allocate_ui_at_rect(content_rect, |ui| {
                    egui::Frame::none()
                        .fill(p.bg)
                        .inner_margin(egui::Margin::same(24.0))
                        .show(ui, |ui| {
                            self.show_results(ui, p);
                        });
                });
            });
    }

    fn show_sidebar(&mut self, ui: &mut egui::Ui, p: Palette) {
        let language = self.language();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.heading(
                    egui::RichText::new(tr(language, "任务控制", "Controls"))
                        .color(p.text)
                        .size(19.0),
                );
                ui.add_space(8.0);

                self.status_strip(ui, p);
                ui.add_space(12.0);

                self.disk_index_panel(ui, p);
                ui.add_space(16.0);

                ui.label(
                    egui::RichText::new(tr(language, "扫描目录", "Scan folder")).color(p.muted),
                );
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [226.0, 32.0],
                        egui::TextEdit::singleline(&mut self.scan_path).hint_text(tr(
                            language,
                            "选择要扫描的文件夹",
                            "Choose a folder to scan",
                        )),
                    );
                    if ui
                        .add_sized(
                            [60.0, 32.0],
                            secondary_button(tr(language, "浏览", "Browse"), p),
                        )
                        .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.scan_path = path.to_string_lossy().to_string();
                            self.disk_id = default_disk_id_for_path(&path);
                        }
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(tr(language, "快捷：", "Quick:")).color(p.muted));
                    self.quick_path_button(ui, tr(language, "桌面", "Desktop"), "Desktop", p);
                    self.quick_path_button(ui, tr(language, "下载", "Downloads"), "Downloads", p);
                    self.quick_path_button(ui, tr(language, "文档", "Documents"), "Documents", p);
                });

                ui.add_space(12.0);
                ui.label(egui::RichText::new(tr(language, "硬盘标识", "Disk ID")).color(p.muted));
                ui.add_sized(
                    [ui.available_width(), 32.0],
                    egui::TextEdit::singleline(&mut self.disk_id).hint_text(tr(
                        language,
                        "例如 main、backup、usb-1",
                        "e.g. main, backup, usb-1",
                    )),
                );

                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new(tr(language, "比较模式", "Compare mode")).color(p.muted),
                );
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut self.config.use_hash,
                        false,
                        tr(language, "按大小快速匹配", "Fast size match"),
                    );
                    ui.radio_value(
                        &mut self.config.use_hash,
                        true,
                        tr(language, "按采样哈希精确匹配", "Precise sample hash"),
                    );
                });

                ui.add_space(16.0);
                let can_scan = !self.scan_path.trim().is_empty()
                    && !self.disk_id.trim().is_empty()
                    && !self.is_busy();
                ui.add_enabled_ui(can_scan, |ui| {
                    if ui
                        .add_sized(
                            [ui.available_width(), 36.0],
                            primary_button(tr(language, "开始扫描", "Start Scan"), p.green),
                        )
                        .clicked()
                    {
                        self.start_scan();
                    }
                });

                ui.add_enabled_ui(!self.is_busy(), |ui| {
                    if ui
                        .add_sized(
                            [ui.available_width(), 36.0],
                            primary_button(tr(language, "查找重复文件", "Find Duplicates"), p.blue),
                        )
                        .clicked()
                    {
                        self.find_duplicates();
                    }
                });

                if !can_scan && !self.is_busy() {
                    ui.label(
                        egui::RichText::new(tr(
                            language,
                            "请填写扫描目录和硬盘标识。",
                            "Please enter a scan folder and disk ID.",
                        ))
                        .color(p.amber)
                        .size(12.0),
                    );
                }

                ui.add_space(16.0);
                self.metrics(ui, p);
                ui.add_space(12.0);
                ui.label(egui::RichText::new(&self.notice).color(p.muted).size(12.0));
            });
    }

    fn show_results(&mut self, ui: &mut egui::Ui, p: Palette) {
        let language = self.language();
        if self.selected_disk_id.is_some() {
            self.show_disk_files(ui, p);
            return;
        }

        let visible_groups = self.visible_duplicate_groups();
        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new(tr(language, "重复文件", "Duplicate Files"))
                    .color(p.text)
                    .size(21.0),
            );
            ui.label(
                egui::RichText::new(format_count(visible_groups.len(), language, "组", "groups"))
                    .color(p.muted),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let selected_count = self.selected_count();
                if selected_count > 0 {
                    if ui
                        .add_sized(
                            [118.0, 32.0],
                            primary_button(
                                &tr_format(
                                    language,
                                    format!("删除已选 {}", selected_count),
                                    format!("Delete {}", selected_count),
                                ),
                                p.red,
                            ),
                        )
                        .clicked()
                    {
                        self.show_delete_confirm = true;
                    }
                }

                ui.add_enabled_ui(!visible_groups.is_empty(), |ui| {
                    if ui
                        .add_sized(
                            [86.0, 32.0],
                            secondary_button(tr(language, "清空选择", "Clear"), p),
                        )
                        .clicked()
                    {
                        self.clear_selection();
                    }
                    if ui
                        .add_sized(
                            [128.0, 32.0],
                            secondary_button(tr(language, "选择所有副本", "Select Copies"), p),
                        )
                        .clicked()
                    {
                        self.select_all_duplicates();
                    }
                    if ui
                        .add_sized(
                            [96.0, 32.0],
                            secondary_button(tr(language, "导出报告", "Export"), p),
                        )
                        .clicked()
                    {
                        self.export_report();
                    }
                });
            });
        });

        ui.add_space(8.0);
        self.duplicate_size_filter_control(ui, p);
        ui.add_space(10.0);

        if visible_groups.is_empty() {
            self.empty_state(ui, p);
            return;
        }

        let groups = visible_groups;
        let mut selection_updates = Vec::new();
        let mut files_to_reveal = Vec::new();
        let mut ignored_groups = Vec::new();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (group_index, group) in groups.iter().enumerate() {
                    self.result_group(
                        ui,
                        group_index,
                        group,
                        &mut selection_updates,
                        &mut files_to_reveal,
                        &mut ignored_groups,
                        p,
                    );
                    ui.add_space(10.0);
                }
            });

        for (path, selected) in selection_updates {
            self.selected_files.insert(path, selected);
        }

        for path in files_to_reveal {
            self.show_in_explorer(&path);
        }

        for group_key in ignored_groups {
            self.ignore_duplicate_group(&group_key);
        }
    }

    fn show_config_window(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        let language = self.language();
        let mut open = self.show_config;
        if open {
            egui::Window::new(tr(language, "扫描设置", "Scan Settings"))
                .open(&mut open)
                .default_width(560.0)
                .resizable(true)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(tr(language, "文件过滤", "File Filters")).strong(),
                        );
                        ui.add_space(8.0);

                        let mut size_mb = self.config.min_file_size as f64 / (1024.0 * 1024.0);
                        let mut config_changed = false;
                        ui.horizontal(|ui| {
                            ui.label(tr(language, "最小文件大小（MB）", "Minimum file size (MB)"));
                            if ui
                                .add(
                                    egui::DragValue::new(&mut size_mb)
                                        .range(0.0..=102400.0)
                                        .speed(1.0),
                                )
                                .changed()
                            {
                                self.config.min_file_size = (size_mb * 1024.0 * 1024.0) as u64;
                                config_changed = true;
                            }
                        });

                        ui.add_space(12.0);
                        config_changed |= tag_editor(
                            ui,
                            tr(language, "处理的扩展名", "Included extensions"),
                            &mut self.new_extension,
                            &mut self.config.file_extensions,
                            language,
                            p,
                        );
                        ui.add_space(12.0);
                        config_changed |= tag_editor(
                            ui,
                            tr(language, "忽略的扩展名", "Ignored extensions"),
                            &mut self.new_ignore_extension,
                            &mut self.config.ignore_extensions,
                            language,
                            p,
                        );
                        ui.add_space(12.0);
                        config_changed |= tag_editor(
                            ui,
                            tr(language, "忽略的目录", "Ignored folders"),
                            &mut self.new_ignore_dir,
                            &mut self.config.ignore_dirs,
                            language,
                            p,
                        );

                        ui.add_space(12.0);
                        if ui
                            .checkbox(
                                &mut self.config.use_hash,
                                tr(
                                    language,
                                    "默认使用采样哈希比较",
                                    "Use sample hash by default",
                                ),
                            )
                            .changed()
                        {
                            config_changed = true;
                        }
                        ui.label(
                            egui::RichText::new(tr(
                                language,
                                "哈希模式更准确，但扫描大文件时会产生更多 I/O。",
                                "Hash mode is more accurate but reads more data from large files.",
                            ))
                            .size(12.0)
                            .color(p.muted),
                        );

                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new(tr(language, "删除方式", "Delete Mode")).strong(),
                        );
                        let mut delete_mode = self.delete_mode;
                        let changed = ui
                            .radio_value(
                                &mut delete_mode,
                                DeleteMode::MoveToTrash,
                                tr(
                                    language,
                                    "移到废纸篓（推荐）",
                                    "Move to Trash (recommended)",
                                ),
                            )
                            .changed()
                            | ui.radio_value(
                                &mut delete_mode,
                                DeleteMode::DirectRemove,
                                tr(
                                    language,
                                    "直接删除（不可恢复）",
                                    "Remove directly (irreversible)",
                                ),
                            )
                            .changed();
                        ui.label(
                            egui::RichText::new(tr(
                                language,
                                "默认移到废纸篓，确认无误后可再清空废纸篓。",
                                "Default is Trash; empty it later after checking.",
                            ))
                            .size(12.0)
                            .color(p.muted),
                        );
                        if changed {
                            self.save_delete_mode(delete_mode);
                        }
                        if config_changed {
                            self.save_app_config();
                        }
                    });
                });
        }
        self.show_config = open;
    }

    fn show_support_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_support;
        if !open {
            return;
        }

        let p = self.palette();
        let language = self.language();
        let textures = self.support_textures(ctx);
        let mut close_window = false;
        egui::Window::new(tr(language, "关注和赞助", "Follow & Donate"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(760.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(tr(
                        language,
                        "感谢支持这个开源工具。",
                        "Thanks for supporting this open-source tool.",
                    ))
                    .strong()
                    .color(p.text),
                );
                ui.add_space(12.0);

                if let Some(textures) = &textures {
                    ui.horizontal(|ui| {
                        support_qr_card(
                            ui,
                            tr(language, "公众号", "Official Account"),
                            &textures.official_account,
                            p,
                        );
                        support_qr_card(
                            ui,
                            tr(language, "微信赞助", "WeChat"),
                            &textures.wechat_pay,
                            p,
                        );
                        support_qr_card(
                            ui,
                            tr(language, "支付宝赞助", "Alipay"),
                            &textures.alipay,
                            p,
                        );
                    });
                } else {
                    ui.label(
                        egui::RichText::new(tr(
                            language,
                            "二维码资源加载失败。",
                            "Failed to load QR code assets.",
                        ))
                        .color(p.red),
                    );
                }

                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [96.0, 34.0],
                            secondary_button(tr(language, "关闭", "Close"), p),
                        )
                        .clicked()
                    {
                        close_window = true;
                    }
                });
            });

        self.show_support = open && !close_window;
    }

    fn show_delete_window(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        let language = self.language();
        let mut open = self.show_delete_confirm;
        let mut close_window = false;
        if open {
            egui::Window::new(tr(language, "确认删除", "Confirm Delete"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(440.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    let selected_count = self.selected_count();
                    let selected_size = self.selected_size();

                    egui::Frame::none()
                        .fill(egui::Color32::from_rgb(254, 242, 242))
                        .stroke(egui::Stroke::new(1.0, p.red))
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(self.delete_confirm_text()).color(p.text));
                        });

                    ui.add_space(12.0);
                    ui.label(tr_format(
                        language,
                        format!("将处理 {} 个文件", selected_count),
                        format!("{} files will be processed", selected_count),
                    ));
                    ui.label(tr_format(
                        language,
                        format!("预计释放 {}", format_bytes(selected_size)),
                        format!("Estimated savings {}", format_bytes(selected_size)),
                    ));

                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized(
                                [130.0, 36.0],
                                primary_button(tr(language, "确认删除", "Delete"), p.red),
                            )
                            .clicked()
                        {
                            self.delete_selected_files();
                            close_window = true;
                        }
                        if ui
                            .add_sized(
                                [96.0, 36.0],
                                secondary_button(tr(language, "取消", "Cancel"), p),
                            )
                            .clicked()
                        {
                            close_window = true;
                        }
                    });
                });
        }
        self.show_delete_confirm = open && !close_window;
    }

    fn show_alert_window(&mut self, ctx: &egui::Context) {
        let Some(message) = self.alert_message.clone() else {
            return;
        };

        let p = self.palette();
        let language = self.language();
        let mut close_window = false;
        egui::Window::new(tr(language, "提示", "Notice"))
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(message).color(p.text));
                ui.add_space(16.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [96.0, 34.0],
                            primary_button(tr(language, "知道了", "OK"), p.blue),
                        )
                        .clicked()
                    {
                        close_window = true;
                    }
                });
            });

        if close_window {
            self.alert_message = None;
        }
    }

    fn show_disk_files(&mut self, ui: &mut egui::Ui, p: Palette) {
        let disk_id = self.selected_disk_id.clone().unwrap_or_default();
        let language = self.language();
        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new(tr_format(
                    language,
                    format!("磁盘文件：{}", disk_id),
                    format!("Disk Files: {}", disk_id),
                ))
                .color(p.text)
                .size(21.0),
            );
            ui.label(
                egui::RichText::new(format_count(
                    self.selected_disk_files.len(),
                    language,
                    "个文件",
                    "files",
                ))
                .color(p.muted),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized(
                        [128.0, 32.0],
                        secondary_button(tr(language, "返回重复文件", "Back to Duplicates"), p),
                    )
                    .clicked()
                {
                    self.selected_disk_id = None;
                    self.selected_disk_files.clear();
                }
            });
        });

        ui.add_space(10.0);

        egui::Frame::none()
            .fill(p.surface)
            .stroke(egui::Stroke::new(1.0, p.border))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(14.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr(language, "大小", "Size"))
                            .strong()
                            .color(p.muted),
                    );
                    ui.add_space(72.0);
                    ui.label(
                        egui::RichText::new(tr(language, "文件路径", "File Path"))
                            .strong()
                            .color(p.muted),
                    );
                });
                ui.separator();

                let files = self.selected_disk_files.clone();
                let mut files_to_reveal = Vec::new();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for file in files {
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [86.0, 24.0],
                                    egui::Label::new(
                                        egui::RichText::new(format_bytes(file.size))
                                            .color(p.blue)
                                            .size(12.0),
                                    ),
                                );

                                let path_width = (ui.available_width() - 62.0).max(0.0);
                                ui.add_sized(
                                    [path_width, 24.0],
                                    egui::Label::new(
                                        egui::RichText::new(shorten_path(&file.path, 112))
                                            .color(p.text)
                                            .size(12.0),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::hover()),
                                )
                                .on_hover_text(format!(
                                    "{}: {}\n{}: {}\n{}: {}",
                                    tr(language, "磁盘", "Disk"),
                                    file.disk_id,
                                    tr(language, "路径", "Path"),
                                    file.path,
                                    tr(language, "哈希", "Hash"),
                                    file.sample_hash.as_deref().unwrap_or(tr(
                                        language,
                                        "未记录",
                                        "Not recorded"
                                    ))
                                ));

                                if ui
                                    .add_sized(
                                        [62.0, 24.0],
                                        secondary_button(tr(language, "定位", "Reveal"), p),
                                    )
                                    .clicked()
                                {
                                    files_to_reveal.push(file.path.clone());
                                }
                            });
                            ui.add_space(3.0);
                        }
                    });

                for path in files_to_reveal {
                    self.show_in_explorer(&path);
                }
            });
    }

    fn result_group(
        &self,
        ui: &mut egui::Ui,
        group_index: usize,
        group: &DuplicateGroup,
        selection_updates: &mut Vec<(String, bool)>,
        files_to_reveal: &mut Vec<String>,
        ignored_groups: &mut Vec<String>,
        p: Palette,
    ) {
        let language = self.language();
        egui::Frame::none()
            .fill(p.surface)
            .stroke(egui::Stroke::new(1.0, p.border))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr_format(
                            language,
                            format!("重复组 {}", group_index + 1),
                            format!("Group {}", group_index + 1),
                        ))
                        .strong()
                        .color(p.text),
                    );
                    ui.separator();
                    ui.label(egui::RichText::new(format_bytes(group.size)).color(p.blue));
                    ui.label(
                        egui::RichText::new(format_count(
                            group.files.len(),
                            language,
                            "个文件",
                            "files",
                        ))
                        .color(p.muted),
                    );
                    ui.label(
                        egui::RichText::new(tr_format(
                            language,
                            format!(
                                "可释放 {}",
                                format_bytes(group.size * (group.files.len() - 1) as u64)
                            ),
                            format!(
                                "Save {}",
                                format_bytes(group.size * (group.files.len() - 1) as u64)
                            ),
                        ))
                        .color(p.green),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_sized(
                                [96.0, 26.0],
                                secondary_button(tr(language, "本次忽略", "Ignore"), p),
                            )
                            .on_hover_text(tr(
                                language,
                                "本轮隐藏该重复组，不参与选择和删除",
                                "Hide this group for this run; it will not be selected or deleted",
                            ))
                            .clicked()
                        {
                            ignored_groups.push(group_key(group));
                        }
                    });
                });

                ui.add_space(8.0);

                for (file_index, file_path) in group.files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if file_index == 0 {
                            ui.add_sized(
                                [64.0, 24.0],
                                egui::Label::new(
                                    egui::RichText::new(tr_format(
                                        language,
                                        format!("{} 保留", file_path.disk_id),
                                        format!("{} Keep", file_path.disk_id),
                                    ))
                                    .color(p.green),
                                ),
                            );
                        } else {
                            let key = file_key(file_path);
                            let mut selected =
                                self.selected_files.get(&key).copied().unwrap_or(false);
                            if ui
                                .checkbox(&mut selected, tr(language, "删除", "Delete"))
                                .changed()
                            {
                                selection_updates.push((key, selected));
                            }
                        }

                        let path_width = (ui.available_width() - 72.0).max(0.0);
                        ui.add_sized(
                            [path_width, 24.0],
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "[{}] {}",
                                    file_path.disk_id,
                                    shorten_path(&file_path.path, 88)
                                ))
                                .size(12.0)
                                .color(p.text),
                            )
                            .truncate()
                            .sense(egui::Sense::hover()),
                        )
                        .on_hover_text(format!(
                            "{}: {}\n{}",
                            tr(language, "磁盘", "Disk"),
                            file_path.disk_id,
                            file_path.path
                        ));

                        if ui
                            .add_sized(
                                [62.0, 24.0],
                                secondary_button(tr(language, "定位", "Reveal"), p),
                            )
                            .clicked()
                        {
                            files_to_reveal.push(file_path.path.clone());
                        }
                    });
                }
            });
    }

    fn duplicate_size_filter_control(&mut self, ui: &mut egui::Ui, p: Palette) {
        let language = self.language();
        egui::Frame::none()
            .fill(p.surface)
            .stroke(egui::Stroke::new(1.0, p.border))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr(
                            language,
                            "本轮只处理不小于",
                            "This run only handles duplicates at least",
                        ))
                        .color(p.muted),
                    );
                    let changed = ui
                        .add(
                            egui::DragValue::new(&mut self.duplicate_size_filter_mb)
                                .range(0.0..=1024.0 * 1024.0)
                                .speed(10.0)
                                .suffix(" MB"),
                        )
                        .changed();
                    ui.label(egui::RichText::new(tr(language, "的重复文件", "")).color(p.muted));

                    if self.duplicate_size_filter_mb > 0.0
                        && ui
                            .add_sized(
                                [62.0, 26.0],
                                secondary_button(tr(language, "重置", "Reset"), p),
                            )
                            .clicked()
                    {
                        self.duplicate_size_filter_mb = 0.0;
                        self.apply_duplicate_size_filter();
                    }

                    if changed {
                        self.duplicate_size_filter_mb = self.duplicate_size_filter_mb.max(0.0);
                        self.apply_duplicate_size_filter();
                    }
                });
            });
    }

    fn status_strip(&self, ui: &mut egui::Ui, p: Palette) {
        let language = self.language();
        let (label, color) = match &self.scan_status {
            ScanStatus::Ready => (tr(language, "就绪", "Ready").to_string(), p.muted),
            ScanStatus::Scanning(path) => (
                tr_format(
                    language,
                    format!("正在扫描 {}", shorten_path(path, 28)),
                    format!("Scanning {}", shorten_path(path, 28)),
                ),
                p.blue,
            ),
            ScanStatus::Analyzing => (
                tr(language, "正在分析重复文件", "Analyzing duplicates").to_string(),
                p.amber,
            ),
            ScanStatus::Complete(message) => (message.clone(), p.green),
            ScanStatus::Error(message) => (
                tr_format(
                    language,
                    format!("错误：{}", message),
                    format!("Error: {}", message),
                ),
                p.red,
            ),
        };

        egui::Frame::none()
            .fill(p.accent_soft)
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgb(199, 218, 255),
            ))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.is_busy() {
                        ui.spinner();
                    }
                    ui.label(egui::RichText::new(label).color(color).size(13.0));
                });
            });
    }

    fn metrics(&self, ui: &mut egui::Ui, p: Palette) {
        let language = self.language();
        ui.heading(
            egui::RichText::new(tr(language, "概览", "Overview"))
                .color(p.text)
                .size(20.0),
        );
        ui.add_space(6.0);
        metric(
            ui,
            tr(language, "最近扫描", "Last Scan"),
            &format_count(self.last_scan_count, language, "个文件", "files"),
            p.blue,
            p,
        );
        metric(
            ui,
            tr(language, "重复候选", "Candidates"),
            &format_count(self.total_candidates, language, "个文件", "files"),
            p.amber,
            p,
        );
        metric(
            ui,
            tr(language, "可删除副本", "Removable"),
            &format_count(self.total_duplicates, language, "个", "items"),
            p.red,
            p,
        );
        metric(
            ui,
            tr(language, "预计释放", "Estimated Savings"),
            &format_bytes(self.potential_savings),
            p.green,
            p,
        );
    }

    fn disk_index_panel(&mut self, ui: &mut egui::Ui, p: Palette) {
        let language = self.language();
        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new(tr(language, "磁盘索引", "Disk Index"))
                    .color(p.text)
                    .size(20.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized(
                        [62.0, 26.0],
                        secondary_button(tr(language, "刷新", "Refresh"), p),
                    )
                    .clicked()
                {
                    self.refresh_disk_summaries();
                }
            });
        });

        if self.disk_summaries.is_empty() {
            egui::Frame::none()
                .fill(p.surface)
                .stroke(egui::Stroke::new(1.0, p.border))
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(tr(
                            language,
                            "还没有记录任何移动磁盘。",
                            "No disk indexes recorded yet.",
                        ))
                        .color(p.muted)
                        .size(12.0),
                    );
                });
            return;
        }

        let disks = self.disk_summaries.clone();
        for disk in disks {
            egui::Frame::none()
                .fill(p.surface)
                .stroke(egui::Stroke::new(1.0, p.border))
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&disk.disk_id)
                            .strong()
                            .color(p.text)
                            .size(13.0),
                    );
                    ui.add_space(3.0);
                    ui.label(
                        egui::RichText::new(tr_format(
                            language,
                            format!(
                                "{} 个文件 · {}",
                                disk.file_count,
                                format_bytes(disk.total_size.max(0) as u64)
                            ),
                            format!(
                                "{} files · {}",
                                disk.file_count,
                                format_bytes(disk.total_size.max(0) as u64)
                            ),
                        ))
                        .color(p.muted)
                        .size(12.0),
                    )
                    .on_hover_text(tr_format(
                        language,
                        format!(
                            "路径：{}\n最后扫描：{}",
                            disk.root_path, disk.last_scanned_at
                        ),
                        format!(
                            "Path: {}\nLast scanned: {}",
                            disk.root_path, disk.last_scanned_at
                        ),
                    ));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized(
                                [52.0, 26.0],
                                secondary_button(tr(language, "查看", "View"), p),
                            )
                            .clicked()
                        {
                            self.load_disk_files(&disk.disk_id);
                        }
                        if ui
                            .add_sized(
                                [62.0, 26.0],
                                secondary_button(tr(language, "重扫", "Rescan"), p),
                            )
                            .clicked()
                        {
                            self.rescan_disk(&disk);
                        }
                        if ui
                            .add_sized(
                                [62.0, 26.0],
                                secondary_button(tr(language, "移除", "Remove"), p),
                            )
                            .clicked()
                        {
                            self.clear_disk_index(&disk.disk_id);
                        }
                    });
                });
            ui.add_space(6.0);
        }
    }

    fn empty_state(&self, ui: &mut egui::Ui, p: Palette) {
        let language = self.language();
        egui::Frame::none()
            .fill(p.surface)
            .stroke(egui::Stroke::new(1.0, p.border))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(28.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    egui::Frame::none()
                        .fill(p.accent_soft)
                        .rounding(egui::Rounding::same(999.0))
                        .inner_margin(egui::Margin::symmetric(20.0, 10.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(tr(
                                    language,
                                    "本地文件索引",
                                    "Local File Index",
                                ))
                                .size(14.0)
                                .strong()
                                .color(p.blue),
                            );
                        });
                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new(tr(language, "准备开始清理", "Ready to Clean"))
                            .size(25.0)
                            .strong()
                            .color(p.text),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(tr(
                            language,
                            "选择一个目录进行扫描，或直接分析已有数据库中的文件索引。",
                            "Choose a folder to scan, or analyze the file index already in the database.",
                        ))
                        .color(p.muted),
                    );
                    ui.add_space(50.0);
                });
            });
    }

    fn quick_path_button(&mut self, ui: &mut egui::Ui, label: &str, folder: &str, p: Palette) {
        if ui
            .add_sized([54.0, 24.0], secondary_button(label, p))
            .clicked()
        {
            if let Some(home) = dirs::home_dir() {
                let path = home.join(folder);
                self.scan_path = path.to_string_lossy().to_string();
                self.disk_id = default_disk_id_for_path(&path);
            }
        }
    }

    fn start_scan(&mut self) {
        let language = self.language();
        if self.scan_path.trim().is_empty() || self.disk_id.trim().is_empty() {
            self.scan_status = ScanStatus::Error(
                tr(
                    language,
                    "请输入扫描目录和硬盘标识",
                    "Enter a scan folder and disk ID",
                )
                .to_string(),
            );
            return;
        }

        let path = PathBuf::from(self.scan_path.trim());
        if !path.exists() {
            self.scan_status = ScanStatus::Error(
                tr(language, "扫描目录不存在", "Scan folder does not exist").to_string(),
            );
            return;
        }

        let disk_id = self.disk_id.trim().to_string();
        let config = self.config.clone();
        let sender = self.message_sender.clone();

        self.scan_status = ScanStatus::Scanning(path.to_string_lossy().to_string());
        self.notice = tr(
            language,
            "正在扫描文件系统，请保持应用打开。",
            "Scanning the file system. Please keep the app open.",
        )
        .to_string();

        thread::spawn(move || {
            let started_at = Instant::now();
            let mut scanner = FileScanner::new();
            let result = scanner
                .scan_disk(&path, &disk_id, &config)
                .map(|count| ScanSummary {
                    count,
                    elapsed: started_at.elapsed(),
                });
            let _ = sender.send(AppMessage::ScanComplete(result.map_err(|e| e.to_string())));
        });
    }

    fn rescan_disk(&mut self, disk: &DiskSummary) {
        let language = self.language();
        if disk.root_path.trim().is_empty() {
            let message = tr_format(
                language,
                format!(
                    "磁盘 {} 没有可重扫的路径，请重新选择目录扫描。",
                    disk.disk_id
                ),
                format!(
                    "Disk {} has no path to rescan. Please choose a folder and scan again.",
                    disk.disk_id
                ),
            );
            self.notice = message.clone();
            self.alert_message = Some(message);
            return;
        }

        let path = PathBuf::from(disk.root_path.trim());
        if !path.exists() {
            let message = tr_format(
                language,
                format!("磁盘未连接，无法重扫：{}", disk.root_path),
                format!("Disk is not connected, cannot rescan: {}", disk.root_path),
            );
            self.notice = message.clone();
            self.alert_message = Some(message);
            return;
        }

        self.scan_path = disk.root_path.clone();
        self.disk_id = disk.disk_id.clone();
        self.start_scan();
    }

    fn refresh_disk_summaries(&mut self) {
        let mut scanner = FileScanner::new();
        match scanner.list_disks() {
            Ok(disks) => {
                self.disk_summaries = disks;
                self.disk_summaries_loaded = true;
            }
            Err(error) => {
                self.disk_summaries_loaded = true;
                self.notice = tr_format(
                    self.language(),
                    format!("读取磁盘索引失败：{}", error),
                    format!("Failed to read disk indexes: {}", error),
                );
            }
        }
    }

    fn load_disk_files(&mut self, disk_id: &str) {
        let language = self.language();
        let mut scanner = FileScanner::new();
        match scanner.list_files_for_disk(disk_id) {
            Ok(files) => {
                self.selected_disk_id = Some(disk_id.to_string());
                self.selected_disk_files = files;
                self.notice = tr_format(
                    language,
                    format!("已加载磁盘 {} 的全部文件索引。", disk_id),
                    format!("Loaded all file indexes for disk {}.", disk_id),
                );
            }
            Err(error) => {
                self.notice = tr_format(
                    language,
                    format!("读取磁盘 {} 文件索引失败：{}", disk_id, error),
                    format!("Failed to read file index for disk {}: {}", disk_id, error),
                );
                self.scan_status = ScanStatus::Error(error.to_string());
            }
        }
    }

    fn clear_disk_index(&mut self, disk_id: &str) {
        let language = self.language();
        let mut scanner = FileScanner::new();
        match scanner.clear_disk(disk_id) {
            Ok(()) => {
                self.notice = tr_format(
                    language,
                    format!("已移除磁盘 {} 的文件索引。", disk_id),
                    format!("Removed file index for disk {}.", disk_id),
                );
                if self.selected_disk_id.as_deref() == Some(disk_id) {
                    self.selected_disk_id = None;
                    self.selected_disk_files.clear();
                }
                for group in &mut self.duplicate_groups {
                    group.files.retain(|file| file.disk_id != disk_id);
                }
                self.duplicate_groups.retain(|group| group.files.len() > 1);
                self.selected_files.clear();
                self.calculate_statistics();
                self.refresh_disk_summaries();
            }
            Err(error) => {
                self.notice = tr_format(
                    language,
                    format!("移除磁盘 {} 失败：{}", disk_id, error),
                    format!("Failed to remove disk {}: {}", disk_id, error),
                );
                self.scan_status = ScanStatus::Error(error.to_string());
            }
        }
    }

    fn save_delete_mode(&mut self, delete_mode: DeleteMode) {
        self.delete_mode = delete_mode;
        let mut scanner = FileScanner::new();
        if let Err(error) = scanner.set_delete_mode(delete_mode) {
            self.notice = tr_format(
                self.language(),
                format!("保存删除方式失败：{}", error),
                format!("Failed to save delete mode: {}", error),
            );
            self.alert_message = Some(self.notice.clone());
        }
    }

    fn save_app_config(&mut self) {
        let mut scanner = FileScanner::new();
        if let Err(error) = scanner.set_app_config(&self.config) {
            self.notice = tr_format(
                self.language(),
                format!("保存扫描配置失败：{}", error),
                format!("Failed to save scan settings: {}", error),
            );
            self.alert_message = Some(self.notice.clone());
        }
    }

    fn delete_confirm_text(&self) -> &'static str {
        match (self.delete_mode, self.language()) {
            (DeleteMode::MoveToTrash, UiLanguage::Zh) => "文件将移到废纸篓，可在清空废纸篓前恢复。",
            (DeleteMode::MoveToTrash, UiLanguage::En) => {
                "Files will be moved to Trash and can be restored before Trash is emptied."
            }
            (DeleteMode::DirectRemove, UiLanguage::Zh) => "删除不可撤销，请确认这些副本不再需要。",
            (DeleteMode::DirectRemove, UiLanguage::En) => {
                "Deletion cannot be undone. Confirm these copies are no longer needed."
            }
        }
    }

    fn find_duplicates(&mut self) {
        let sender = self.message_sender.clone();
        let use_hash = self.config.use_hash;

        self.scan_status = ScanStatus::Analyzing;
        self.notice = tr(
            self.language(),
            "正在从数据库分析重复文件。",
            "Analyzing duplicates from the database.",
        )
        .to_string();

        thread::spawn(move || {
            let mut scanner = FileScanner::new();
            let result = scanner.find_duplicates(use_hash);
            let _ = sender.send(AppMessage::DuplicatesFound(
                result.map_err(|e| e.to_string()),
            ));
        });
    }

    fn calculate_statistics(&mut self) {
        let groups = self.visible_duplicate_groups();
        self.total_candidates = groups.iter().map(|group| group.files.len()).sum();
        self.total_duplicates = groups
            .iter()
            .map(|group| group.files.len().saturating_sub(1))
            .sum();
        self.potential_savings = groups
            .iter()
            .map(|group| group.size * group.files.len().saturating_sub(1) as u64)
            .sum();
    }

    fn duplicate_size_filter_bytes(&self) -> u64 {
        (self.duplicate_size_filter_mb.max(0.0) * 1024.0 * 1024.0) as u64
    }

    fn group_passes_duplicate_size_filter(&self, group: &DuplicateGroup) -> bool {
        group.size >= self.duplicate_size_filter_bytes()
    }

    fn visible_duplicate_groups(&self) -> Vec<DuplicateGroup> {
        self.duplicate_groups
            .iter()
            .filter(|group| self.group_passes_duplicate_size_filter(group))
            .cloned()
            .collect()
    }

    fn apply_duplicate_size_filter(&mut self) {
        let visible_file_keys = self
            .visible_duplicate_groups()
            .iter()
            .flat_map(|group| group.files.iter().map(file_key))
            .collect::<HashSet<_>>();
        self.selected_files
            .retain(|file_key, _| visible_file_keys.contains(file_key));
        self.calculate_statistics();
    }

    fn select_all_duplicates(&mut self) {
        for group in self.visible_duplicate_groups() {
            for file_path in group.files.iter().skip(1) {
                self.selected_files.insert(file_key(file_path), true);
            }
        }
    }

    fn clear_selection(&mut self) {
        self.selected_files.clear();
    }

    fn ignore_duplicate_group(&mut self, target_group_key: &str) {
        if let Some(group) = self
            .duplicate_groups
            .iter()
            .find(|group| group_key(group) == target_group_key)
        {
            for file in &group.files {
                self.selected_files.remove(&file_key(file));
            }
        }

        self.duplicate_groups
            .retain(|group| group_key(group) != target_group_key);
        self.calculate_statistics();
        self.notice = tr(
            self.language(),
            "已在本轮忽略该重复组。重新查找重复文件后会再次显示。",
            "Ignored this duplicate group for this run. It will reappear after finding duplicates again.",
        )
        .to_string();
    }

    fn selected_count(&self) -> usize {
        self.selected_files
            .values()
            .filter(|selected| **selected)
            .count()
    }

    fn selected_indexed_files(&self) -> Vec<IndexedFile> {
        self.visible_duplicate_groups()
            .iter()
            .flat_map(|group| group.files.iter().skip(1))
            .filter(|file| {
                self.selected_files
                    .get(&file_key(file))
                    .copied()
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    fn selected_size(&self) -> u64 {
        self.selected_indexed_files()
            .iter()
            .map(|file| file.size)
            .sum()
    }

    fn delete_selected_files(&mut self) {
        let selected_files = self.selected_indexed_files();
        if selected_files.is_empty() {
            return;
        }

        let mut scanner = FileScanner::new();
        let mut deleted_paths = Vec::new();
        let mut errors = Vec::new();
        let mut missing_files = Vec::new();

        for file in &selected_files {
            match delete_file_with_mode(file, self.delete_mode) {
                Ok(()) => {
                    if let Err(error) =
                        scanner.delete_file_record_for_disk(&file.disk_id, &file.path)
                    {
                        errors.push(tr_format(
                            self.language(),
                            format!("数据库记录删除失败：{} ({})", file.path, error),
                            format!(
                                "Failed to remove database record: {} ({})",
                                file.path, error
                            ),
                        ));
                    }
                    deleted_paths.push(file.clone());
                }
                Err(error) => {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        missing_files.push(file.path.clone());
                    }
                    errors.push(delete_failure_message(file, &error, self.language()));
                }
            }
        }

        for deleted_file in &deleted_paths {
            self.selected_files.remove(&file_key(deleted_file));
            for group in &mut self.duplicate_groups {
                group.files.retain(|file| {
                    file.disk_id != deleted_file.disk_id || file.path != deleted_file.path
                });
            }
        }

        self.duplicate_groups.retain(|group| group.files.len() > 1);
        self.calculate_statistics();

        self.notice = if errors.is_empty() {
            match self.delete_mode {
                DeleteMode::MoveToTrash => tr_format(
                    self.language(),
                    format!("已将 {} 个文件移到废纸篓。", deleted_paths.len()),
                    format!("Moved {} files to Trash.", deleted_paths.len()),
                ),
                DeleteMode::DirectRemove => tr_format(
                    self.language(),
                    format!("已删除 {} 个文件。", deleted_paths.len()),
                    format!("Deleted {} files.", deleted_paths.len()),
                ),
            }
        } else {
            tr_format(
                self.language(),
                format!(
                    "已删除 {} 个文件，{} 个操作失败。",
                    deleted_paths.len(),
                    errors.len()
                ),
                format!(
                    "Processed {} files, {} operations failed.",
                    deleted_paths.len(),
                    errors.len()
                ),
            )
        };
        if let Some(first_real_error) = errors.iter().find(|error| {
            !error.starts_with("文件不存在，请重新扫描：")
                && !error.starts_with("File does not exist, please rescan:")
        }) {
            self.scan_status = ScanStatus::Error(first_real_error.clone());
        }
        if !missing_files.is_empty() {
            self.alert_message = Some(tr_format(
                self.language(),
                format!(
                    "文件不存在，请重新扫描。\n\n{}",
                    missing_files
                        .iter()
                        .take(3)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                format!(
                    "File does not exist. Please rescan.\n\n{}",
                    missing_files
                        .iter()
                        .take(3)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            ));
        } else if deleted_paths.is_empty() {
            self.alert_message = errors.first().cloned();
        }
    }

    fn export_report(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(report::DEFAULT_REPORT_PATH)
            .save_file()
        else {
            self.notice = tr(
                self.language(),
                "已取消导出报告。",
                "Report export cancelled.",
            )
            .to_string();
            return;
        };
        self.export_report_to_path(path);
    }

    fn export_report_to_path(&mut self, path: impl AsRef<Path>) {
        let groups = self.visible_duplicate_groups();
        let path = path.as_ref();
        match report::export_report(&groups, path) {
            Ok(()) => {
                let message = tr_format(
                    self.language(),
                    format!("报告已导出：{}", path.display()),
                    format!("Report exported: {}", path.display()),
                );
                self.notice = message.clone();
                self.alert_message = Some(message);
            }
            Err(error) => {
                self.notice = tr_format(
                    self.language(),
                    format!("导出报告失败：{}", error),
                    format!("Failed to export report: {}", error),
                );
                self.alert_message = Some(self.notice.clone());
                self.scan_status = ScanStatus::Error(error.to_string());
            }
        }
    }

    fn show_in_explorer(&mut self, file_path: &str) {
        if !std::path::Path::new(file_path).exists() {
            let message = tr(
                self.language(),
                "文件不存在，请重新扫描。",
                "File does not exist. Please rescan.",
            )
            .to_string();
            self.notice = message.clone();
            self.alert_message = Some(format!("{}\n\n{}", message, file_path));
            return;
        }

        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .args(["/select,", file_path])
                .spawn()
                .ok();
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .args(["-R", file_path])
                .spawn()
                .ok();
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(parent) = std::path::Path::new(file_path).parent() {
                std::process::Command::new("xdg-open")
                    .arg(parent)
                    .spawn()
                    .ok();
            }
        }
    }

    fn is_busy(&self) -> bool {
        matches!(
            self.scan_status,
            ScanStatus::Scanning(_) | ScanStatus::Analyzing
        )
    }
}

fn default_scan_path() -> String {
    dirs::home_dir()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
fn scan_complete_message(summary: &ScanSummary) -> String {
    scan_complete_message_for_language(summary, UiLanguage::Zh)
}

fn scan_complete_message_for_language(summary: &ScanSummary, language: UiLanguage) -> String {
    match language {
        UiLanguage::Zh => format!(
            "扫描完成，记录了 {} 个文件，耗时 {}。",
            summary.count,
            format_duration(summary.elapsed)
        ),
        UiLanguage::En => format!(
            "Scan complete, indexed {} files in {}.",
            summary.count,
            format_duration(summary.elapsed)
        ),
    }
}

fn duplicate_group_found_message(count: usize, language: UiLanguage) -> String {
    tr_format(
        language,
        format!("找到 {} 组重复文件。", count),
        format!("Found {} duplicate groups.", count),
    )
}

fn tr(language: UiLanguage, zh: &'static str, en: &'static str) -> &'static str {
    match language {
        UiLanguage::Zh => zh,
        UiLanguage::En => en,
    }
}

fn tr_format(language: UiLanguage, zh: String, en: String) -> String {
    match language {
        UiLanguage::Zh => zh,
        UiLanguage::En => en,
    }
}

fn format_count(count: usize, language: UiLanguage, zh_unit: &str, en_unit: &str) -> String {
    match language {
        UiLanguage::Zh => format!("{} {}", count, zh_unit),
        UiLanguage::En => format!("{} {}", count, en_unit),
    }
}

fn format_duration(duration: Duration) -> String {
    let total_millis = duration.as_millis();
    if total_millis < 1_000 {
        format!("{}ms", total_millis)
    } else if total_millis < 60_000 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        let minutes = duration.as_secs() / 60;
        let seconds = duration.as_secs() % 60;
        format!("{}m{}s", minutes, seconds)
    }
}

fn load_delete_mode() -> DeleteMode {
    FileScanner::new()
        .get_delete_mode()
        .unwrap_or(DeleteMode::MoveToTrash)
}

fn load_app_config() -> AppConfig {
    FileScanner::new()
        .get_app_config()
        .unwrap_or_else(|_| AppConfig::default())
}

fn delete_file_with_mode(file: &IndexedFile, delete_mode: DeleteMode) -> std::io::Result<()> {
    let path = std::path::Path::new(&file.path);
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file does not exist",
        ));
    }

    match delete_mode {
        DeleteMode::MoveToTrash => trash::delete(path)
            .map_err(|error| std::io::Error::other(format!("移到废纸篓失败: {}", error))),
        DeleteMode::DirectRemove => std::fs::remove_file(path),
    }
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str, color: egui::Color32, p: Palette) {
    egui::Frame::none()
        .fill(p.surface)
        .stroke(egui::Stroke::new(1.0, p.border))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).color(p.muted).size(12.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(value).strong().color(color));
                });
            });
        });
    ui.add_space(6.0);
}

fn tag_editor(
    ui: &mut egui::Ui,
    title: &str,
    input: &mut String,
    values: &mut Vec<String>,
    language: UiLanguage,
    p: Palette,
) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new(title).strong());
    ui.horizontal(|ui| {
        ui.add_sized(
            [360.0, 28.0],
            egui::TextEdit::singleline(input).hint_text(tr(
                language,
                "输入后点击添加",
                "Type and click Add",
            )),
        );
        if ui
            .add_sized(
                [64.0, 28.0],
                secondary_button(tr(language, "添加", "Add"), p),
            )
            .clicked()
        {
            let value = input.trim().trim_start_matches('.').to_lowercase();
            if !value.is_empty() && !values.contains(&value) {
                values.push(value);
                changed = true;
            }
            input.clear();
        }
    });

    let mut remove_index = None;
    ui.horizontal_wrapped(|ui| {
        for (index, value) in values.iter().enumerate() {
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(format!("{}  x", value)).color(p.text))
                        .fill(p.surface_2),
                )
                .clicked()
            {
                remove_index = Some(index);
            }
        }
    });

    if let Some(index) = remove_index {
        values.remove(index);
        changed = true;
    }

    changed
}

fn primary_button(label: &str, fill: egui::Color32) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label.to_string())
            .strong()
            .color(egui::Color32::WHITE),
    )
    .fill(fill)
    .rounding(egui::Rounding::same(10.0))
    .stroke(egui::Stroke::new(1.0, fill))
}

fn secondary_button(label: &str, p: Palette) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.to_string()).color(p.text))
        .fill(p.surface_2)
        .rounding(egui::Rounding::same(10.0))
        .stroke(egui::Stroke::new(1.0, p.border))
}

fn delete_failure_message(
    file: &IndexedFile,
    error: &std::io::Error,
    language: UiLanguage,
) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        tr_format(
            language,
            format!("文件不存在，请重新扫描：{}", file.path),
            format!("File does not exist, please rescan: {}", file.path),
        )
    } else {
        tr_format(
            language,
            format!("文件删除失败：{} ({})", file.path, error),
            format!("Failed to delete file: {} ({})", file.path, error),
        )
    }
}

fn load_theme_icon(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    load_texture_from_bytes(
        ctx,
        "theme-toggle-icon",
        include_bytes!("../assets/theme-toggle.png"),
    )
}

fn load_support_textures(ctx: &egui::Context) -> Option<SupportTextures> {
    Some(SupportTextures {
        official_account: load_texture_from_bytes(
            ctx,
            "support-official-account",
            include_bytes!("../assets/qrcode-official-account.jpg"),
        )?,
        wechat_pay: load_texture_from_bytes(
            ctx,
            "support-wechat-pay",
            include_bytes!("../assets/qrcode-wechat-pay.png"),
        )?,
        alipay: load_texture_from_bytes(
            ctx,
            "support-alipay",
            include_bytes!("../assets/qrcode-alipay.png"),
        )?,
    })
}

fn load_texture_from_bytes(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Some(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}

fn support_qr_card(ui: &mut egui::Ui, title: &str, texture: &egui::TextureHandle, p: Palette) {
    egui::Frame::none()
        .fill(p.surface_2)
        .stroke(egui::Stroke::new(1.0, p.border))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(220.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(title).strong().color(p.text));
                ui.add_space(8.0);
                let image_size = fit_texture_size(texture.size_vec2(), egui::vec2(196.0, 212.0));
                ui.add(egui::Image::new((texture.id(), image_size)));
            });
        });
}

fn fit_texture_size(size: egui::Vec2, max_size: egui::Vec2) -> egui::Vec2 {
    if size.x <= 0.0 || size.y <= 0.0 {
        return max_size;
    }

    let scale = (max_size.x / size.x).min(max_size.y / size.y);
    egui::vec2(size.x * scale, size.y * scale)
}

fn file_key(file: &IndexedFile) -> String {
    format!("{}\u{1f}{}", file.disk_id, file.path)
}

fn group_key(group: &DuplicateGroup) -> String {
    let mut file_keys = group.files.iter().map(file_key).collect::<Vec<_>>();
    file_keys.sort();
    file_keys.join("\u{1e}")
}

fn shorten_path(path: &str, max_chars: usize) -> String {
    let char_count = path.chars().count();
    if char_count <= max_chars {
        return path.to_string();
    }

    let tail: String = path
        .chars()
        .rev()
        .take(max_chars.saturating_sub(3))
        .collect();
    format!("...{}", tail.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn missing_file_delete_message_asks_for_rescan() {
        let file = IndexedFile {
            path: "/Volumes/Offline/example.mp4".to_string(),
            size: 100,
            sample_hash: None,
            disk_id: "Offline".to_string(),
        };
        let error = io::Error::new(io::ErrorKind::NotFound, "missing");

        assert_eq!(
            delete_failure_message(&file, &error, UiLanguage::Zh),
            "文件不存在，请重新扫描：/Volumes/Offline/example.mp4"
        );
    }

    #[test]
    fn missing_file_delete_does_not_set_global_error_status() {
        let missing_file = IndexedFile {
            path: "/tmp/find-dupl-file-missing-delete-test.mp4".to_string(),
            size: 100,
            sample_hash: None,
            disk_id: "Offline".to_string(),
        };
        let mut app = DuplicateFinderApp {
            duplicate_groups: vec![DuplicateGroup {
                size: 100,
                files: vec![
                    IndexedFile {
                        path: "/tmp/find-dupl-file-keeper-test.mp4".to_string(),
                        size: 100,
                        sample_hash: None,
                        disk_id: "Local".to_string(),
                    },
                    missing_file.clone(),
                ],
            }],
            scan_status: ScanStatus::Complete("找到 1 组重复文件。".to_string()),
            ..DuplicateFinderApp::default()
        };
        app.config.ui_language = UiLanguage::Zh;
        app.selected_files.insert(file_key(&missing_file), true);

        app.delete_selected_files();

        assert!(matches!(app.scan_status, ScanStatus::Complete(_)));
        assert!(
            app.alert_message
                .as_deref()
                .unwrap_or_default()
                .contains("文件不存在，请重新扫描。")
        );
    }

    #[test]
    fn scan_complete_message_includes_count_and_elapsed_time() {
        let summary = ScanSummary {
            count: 389,
            elapsed: Duration::from_millis(1_250),
        };

        assert_eq!(
            scan_complete_message(&summary),
            "扫描完成，记录了 389 个文件，耗时 1.2s。"
        );
    }

    #[test]
    fn ignoring_duplicate_group_removes_it_from_current_selection_only() {
        let file_a = IndexedFile {
            path: "/tmp/a.mp4".to_string(),
            size: 100,
            sample_hash: None,
            disk_id: "A".to_string(),
        };
        let file_b = IndexedFile {
            path: "/tmp/b.mp4".to_string(),
            size: 100,
            sample_hash: None,
            disk_id: "B".to_string(),
        };
        let file_c = IndexedFile {
            path: "/tmp/c.mp4".to_string(),
            size: 200,
            sample_hash: None,
            disk_id: "C".to_string(),
        };
        let file_d = IndexedFile {
            path: "/tmp/d.mp4".to_string(),
            size: 200,
            sample_hash: None,
            disk_id: "D".to_string(),
        };
        let ignored_key = group_key(&DuplicateGroup {
            size: 100,
            files: vec![file_a.clone(), file_b.clone()],
        });

        let mut app = DuplicateFinderApp {
            duplicate_groups: vec![
                DuplicateGroup {
                    size: 100,
                    files: vec![file_a, file_b.clone()],
                },
                DuplicateGroup {
                    size: 200,
                    files: vec![file_c, file_d.clone()],
                },
            ],
            ..DuplicateFinderApp::default()
        };
        app.select_all_duplicates();

        app.ignore_duplicate_group(&ignored_key);

        assert_eq!(app.duplicate_groups.len(), 1);
        assert!(!app.selected_files.contains_key(&file_key(&file_b)));
        assert!(app.selected_files.contains_key(&file_key(&file_d)));
        assert_eq!(app.total_duplicates, 1);
        assert_eq!(app.potential_savings, 200);
    }

    #[test]
    fn duplicate_size_filter_limits_selection_and_statistics() {
        let mb = 1024 * 1024;
        let small_duplicate = IndexedFile {
            path: "/tmp/small-copy.mp4".to_string(),
            size: 100 * mb,
            sample_hash: None,
            disk_id: "A".to_string(),
        };
        let large_duplicate = IndexedFile {
            path: "/tmp/large-copy.mp4".to_string(),
            size: 200 * mb,
            sample_hash: None,
            disk_id: "B".to_string(),
        };
        let mut app = DuplicateFinderApp {
            duplicate_groups: vec![
                DuplicateGroup {
                    size: 100 * mb,
                    files: vec![
                        IndexedFile {
                            path: "/tmp/small-original.mp4".to_string(),
                            size: 100 * mb,
                            sample_hash: None,
                            disk_id: "A".to_string(),
                        },
                        small_duplicate.clone(),
                    ],
                },
                DuplicateGroup {
                    size: 200 * mb,
                    files: vec![
                        IndexedFile {
                            path: "/tmp/large-original.mp4".to_string(),
                            size: 200 * mb,
                            sample_hash: None,
                            disk_id: "B".to_string(),
                        },
                        large_duplicate.clone(),
                    ],
                },
            ],
            duplicate_size_filter_mb: 150.0,
            ..DuplicateFinderApp::default()
        };

        app.apply_duplicate_size_filter();
        app.select_all_duplicates();

        assert_eq!(app.visible_duplicate_groups().len(), 1);
        assert!(!app.selected_files.contains_key(&file_key(&small_duplicate)));
        assert!(app.selected_files.contains_key(&file_key(&large_duplicate)));
        assert_eq!(app.selected_indexed_files().len(), 1);
        assert_eq!(app.total_duplicates, 1);
        assert_eq!(app.potential_savings, 200 * mb);
    }

    #[test]
    fn support_window_starts_closed_and_opens_on_action() {
        let mut app = DuplicateFinderApp::default();

        assert!(!app.show_support);

        app.open_support_window();

        assert!(app.show_support);
    }

    #[test]
    fn language_toggle_switches_between_chinese_and_english() {
        let mut app = DuplicateFinderApp::default();
        app.config.ui_language = UiLanguage::Zh;

        app.toggle_language_with_persistence(false);

        assert_eq!(app.config.ui_language, UiLanguage::En);
        assert_eq!(
            scan_complete_message_for_language(
                &ScanSummary {
                    count: 2,
                    elapsed: Duration::from_millis(10)
                },
                app.config.ui_language,
            ),
            "Scan complete, indexed 2 files in 10ms."
        );
    }

    #[test]
    fn support_qr_assets_are_decodable() {
        for bytes in [
            include_bytes!("../assets/qrcode-official-account.jpg").as_slice(),
            include_bytes!("../assets/qrcode-wechat-pay.png").as_slice(),
            include_bytes!("../assets/qrcode-alipay.png").as_slice(),
        ] {
            assert!(image::load_from_memory(bytes).is_ok());
        }
    }

    #[test]
    fn exporting_report_shows_visible_success_alert() {
        let report_path = tempfile::NamedTempFile::new().unwrap();
        let mut app = DuplicateFinderApp {
            duplicate_groups: vec![DuplicateGroup {
                size: 100,
                files: vec![
                    IndexedFile {
                        path: "/tmp/a.mp4".to_string(),
                        size: 100,
                        sample_hash: None,
                        disk_id: "A".to_string(),
                    },
                    IndexedFile {
                        path: "/tmp/b.mp4".to_string(),
                        size: 100,
                        sample_hash: None,
                        disk_id: "B".to_string(),
                    },
                ],
            }],
            ..DuplicateFinderApp::default()
        };
        app.config.ui_language = UiLanguage::Zh;

        app.export_report_to_path(report_path.path());

        let alert = app.alert_message.as_deref().unwrap_or_default();
        assert!(alert.contains("报告已导出"));
        assert!(alert.contains(report_path.path().to_string_lossy().as_ref()));
    }
}
