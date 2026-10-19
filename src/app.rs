use std::{path::PathBuf, sync::Arc, time::Duration};

use egui::{
    Color32, FontId, Frame, Margin, RichText, Rounding, Stroke, Ui, Vec2,
};
use egui_extras::{Column, TableBuilder};
use tokio::runtime::Handle;

use crate::download::{
    engine::start_download,
    types::{DownloadItem, Downloads, Status},
};

// ── Palette ───────────────────────────────────────────────────────────────────
const BG:       Color32 = Color32::from_rgb(18, 18, 26);
const PANEL:    Color32 = Color32::from_rgb(24, 24, 34);
const TOOLBAR:  Color32 = Color32::from_rgb(20, 20, 30);
const SIDEBAR:  Color32 = Color32::from_rgb(22, 22, 32);
const ROW_ODD:  Color32 = Color32::from_rgb(26, 26, 38);
const ROW_EVEN: Color32 = Color32::from_rgb(30, 30, 44);
const ROW_SEL:  Color32 = Color32::from_rgba_premultiplied(0, 140, 186, 60);
const ACCENT:   Color32 = Color32::from_rgb(0, 195, 220);
const ACCENT2:  Color32 = Color32::from_rgb(25, 118, 210);
const GREEN:    Color32 = Color32::from_rgb(72, 199, 142);
const RED:      Color32 = Color32::from_rgb(240, 80, 80);
const ORANGE:   Color32 = Color32::from_rgb(255, 165, 0);
const GRAY:     Color32 = Color32::from_rgb(110, 115, 135);
const DIMGRAY:  Color32 = Color32::from_rgb(70, 74, 90);
const TEXT:     Color32 = Color32::from_rgb(210, 215, 230);
const TEXT_DIM: Color32 = Color32::from_rgb(140, 145, 165);
const BORDER:   Color32 = Color32::from_rgb(38, 40, 55);

// ── Categories ────────────────────────────────────────────────────────────────
#[derive(PartialEq, Clone, Copy)]
enum Category {
    All, Downloading, Completed, Paused, Failed,
}
impl Category {
    fn icon(self) -> &'static str {
        match self {
            Self::All         => "📁",
            Self::Downloading => "⬇",
            Self::Completed   => "✔",
            Self::Paused      => "⏸",
            Self::Failed      => "✖",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::All         => "All Downloads",
            Self::Downloading => "Downloading",
            Self::Completed   => "Finished",
            Self::Paused      => "Paused",
            Self::Failed      => "Failed",
        }
    }
    fn matches(self, i: &DownloadItem) -> bool {
        match self {
            Self::All         => true,
            Self::Downloading => matches!(i.status, Status::Queued|Status::Probing|Status::Downloading),
            Self::Completed   => matches!(i.status, Status::Completed),
            Self::Paused      => matches!(i.status, Status::Paused),
            Self::Failed      => matches!(i.status, Status::Failed|Status::Cancelled),
        }
    }
    fn count(self, v: &[DownloadItem]) -> usize { v.iter().filter(|i| self.matches(i)).count() }
}

// ── Add dialog ────────────────────────────────────────────────────────────────
struct AddDialog { url: String, save_dir: String, filename: String, segments: usize }
impl AddDialog {
    fn new(dir: &str, segs: usize) -> Self {
        AddDialog { url: String::new(), save_dir: dir.to_owned(), filename: String::new(), segments: segs }
    }
}

// ── App ───────────────────────────────────────────────────────────────────────
pub struct IDGApp {
    rt:          Handle,
    client:      Arc<reqwest::Client>,
    downloads:   Downloads,
    category:    Category,
    selected_id: Option<String>,
    show_add:    bool,
    dialog:      AddDialog,
    show_settings: bool,
    default_save_dir: String,
    default_segments: usize,
}

impl IDGApp {
    pub fn new(cc: &eframe::CreationContext, rt: Handle, client: Arc<reqwest::Client>) -> Self {
        setup_visuals(&cc.egui_ctx);

        let default_save_dir = dirs::download_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into());

        IDGApp {
            rt, client,
            downloads: Arc::new(std::sync::Mutex::new(Vec::new())),
            category: Category::All,
            selected_id: None,
            show_add: false,
            dialog: AddDialog::new(&default_save_dir, 8),
            show_settings: false,
            default_save_dir,
            default_segments: 8,
        }
    }

    fn enqueue(&self, url: String, save_path: PathBuf, segments: usize) {
        let item = DownloadItem::new(url, save_path, segments);
        let id   = item.id.clone();
        self.downloads.lock().unwrap().push(item);
        let dl = self.downloads.clone();
        let cl = self.client.clone();
        self.rt.spawn(async move {
            if let Err(e) = start_download(cl, dl.clone(), id.clone()).await {
                crate::download::types::with_item(&dl, &id, |i| {
                    i.status = Status::Failed;
                    i.error  = Some(e.to_string());
                });
            }
        });
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing = Vec2::new(2.0, 0.0);

            self.tool_btn(ui, "⬇", "Add URL",    Color32::from_rgb(40, 130, 240));
            ui.add_space(2.0);
            self.tool_btn(ui, "▶", "Resume",     Color32::from_rgb(40, 185, 100));
            self.tool_btn(ui, "⏹", "Stop",       Color32::from_rgb(90, 95, 120));
            self.tool_btn(ui, "⏹", "Stop All",   Color32::from_rgb(90, 95, 120));
            self.tool_btn(ui, "🗑", "Delete",     Color32::from_rgb(210, 60, 60));
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
            self.tool_btn(ui, "⚙", "Options",    Color32::from_rgb(80, 85, 115));
            self.tool_btn(ui, "⏰", "Scheduler",  Color32::from_rgb(80, 85, 115));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                let speed: f64 = self.downloads.lock().unwrap().iter().map(|i| i.speed_bps).sum();
                let active = self.downloads.lock().unwrap().iter().filter(|i| i.status.is_active()).count();
                if active > 0 {
                    ui.colored_label(GREEN, format!("⬇ {}", fmt_speed(speed)));
                } else {
                    ui.colored_label(GRAY, "Idle");
                }
            });
        });
    }

    fn tool_btn(&mut self, ui: &mut Ui, icon: &str, label: &str, _col: Color32) -> egui::Response {
        let w = 52.0_f32;
        let h = 50.0_f32;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::click());

        if ui.is_rect_visible(rect) {
            let hov = resp.hovered();
            let clk = resp.is_pointer_button_down_on();
            let bg = if clk { Color32::from_rgb(40, 44, 64) }
                     else if hov { Color32::from_rgb(34, 38, 56) }
                     else { Color32::TRANSPARENT };

            if bg != Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, Rounding::same(4.0), bg);
            }

            let icon_rect = egui::Rect::from_min_size(rect.min, Vec2::new(w, h * 0.62));
            let label_rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y + h * 0.6),
                rect.max,
            );

            ui.painter().text(
                icon_rect.center(),
                egui::Align2::CENTER_CENTER,
                icon,
                FontId::proportional(18.0),
                if hov { ACCENT } else { TEXT },
            );
            ui.painter().text(
                label_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::proportional(9.5),
                TEXT_DIM,
            );
        }

        // Handle actual actions
        if resp.clicked() {
            match label {
                "Add URL"   => { self.dialog = AddDialog::new(&self.default_save_dir, self.default_segments); self.show_add = true; }
                "Options"   => { self.show_settings = true; }
                "Delete"    => { self.delete_selected(); }
                "Resume"    => { self.resume_selected(); }
                "Stop"      => { self.pause_selected(); }
                "Stop All"  => { self.pause_all(); }
                _ => {}
            }
        }
        resp
    }

    fn sidebar(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        let cats = [Category::All, Category::Downloading, Category::Completed,
                    Category::Paused, Category::Failed];

        // Header
        Frame::none()
            .fill(Color32::from_rgb(16, 18, 28))
            .inner_margin(Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Categories").size(10.0).color(GRAY).strong());
            });

        ui.add_space(2.0);

        for &cat in &cats {
            let n   = cat.count(items);
            let sel = self.category == cat;

            let fill = if sel { Color32::from_rgb(30, 45, 72) } else { Color32::TRANSPARENT };
            let text_col = if sel { ACCENT } else { TEXT };

            Frame::none()
                .fill(fill)
                .inner_margin(Margin { left: 10.0, right: 6.0, top: 3.0, bottom: 3.0 })
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    let resp = ui.add(
                        egui::Label::new(
                            RichText::new(format!("{} {}  {}", cat.icon(), cat.label(), if n > 0 { format!("({})", n) } else { String::new() }))
                                .size(11.5)
                                .color(text_col),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if resp.clicked() { self.category = cat; }
                    if sel {
                        ui.painter().line_segment(
                            [egui::pos2(ui.min_rect().left(), ui.min_rect().center().y),
                             egui::pos2(ui.min_rect().left() + 3.0, ui.min_rect().center().y)],
                            Stroke::new(14.0, ACCENT),
                        );
                    }
                });
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        let done = items.iter().filter(|i| matches!(i.status, Status::Completed)).count();
        let active = items.iter().filter(|i| i.status.is_active()).count();

        Frame::none()
            .inner_margin(Margin::symmetric(10.0, 2.0))
            .show(ui, |ui| {
                ui.label(RichText::new(format!("✔ {done} finished")).size(10.5).color(GRAY));
                if active > 0 {
                    ui.label(RichText::new(format!("⬇ {active} active")).size(10.5).color(GREEN));
                }
            });
    }

    fn download_table(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        let filtered: Vec<usize> = items.iter().enumerate()
            .filter(|(_, i)| self.category.matches(i))
            .map(|(idx, _)| idx)
            .collect();

        if filtered.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No downloads").size(14.0).color(DIMGRAY));
            });
            return;
        }

        let mut clicked: Option<String>                   = None;
        let mut action:  Option<(String, &'static str)>   = None;
        let sel = self.selected_id.clone();

        // Header row
        Frame::none()
            .fill(Color32::from_rgb(22, 24, 36))
            .inner_margin(Margin { left: 6.0, right: 6.0, top: 3.0, bottom: 3.0 })
            .show(ui, |ui| {
                TableBuilder::new(ui)
                    .striped(false)
                    .sense(egui::Sense::click())
                    .column(Column::exact(20.0))      // #
                    .column(Column::remainder())      // File Name
                    .column(Column::exact(16.0))      // Q
                    .column(Column::exact(72.0))      // Size
                    .column(Column::exact(86.0))      // Status
                    .column(Column::exact(58.0))      // Time left
                    .column(Column::exact(82.0))      // Transfer rate
                    .header(22.0, |mut h| {
                        for hdr in ["#", "File Name", "Q", "Size", "Status", "Time left", "Transfer rate"] {
                            h.col(|ui| {
                                ui.add_space(2.0);
                                ui.label(RichText::new(hdr).size(10.5).color(TEXT_DIM).strong());
                            });
                        }
                    })
                    .body(|mut body| {
                        for (pos, &idx) in filtered.iter().enumerate() {
                            let item   = &items[idx];
                            let is_sel = sel.as_deref() == Some(&item.id);
                            let row_bg = if is_sel { ROW_SEL }
                                         else if pos % 2 == 0 { ROW_EVEN } else { ROW_ODD };

                            body.row(28.0, |mut row| {
                                row.set_selected(is_sel);

                                // # ──────────────────────
                                let (_, r) = row.col(|ui| {
                                    ui.painter().rect_filled(ui.max_rect(), Rounding::ZERO, row_bg);
                                    ui.add_space(2.0);
                                    ui.label(RichText::new(format!("{}", pos+1)).size(10.5).color(DIMGRAY));
                                });
                                if r.clicked() { clicked = Some(item.id.clone()); }

                                // File Name ───────────────
                                let iid = item.id.clone();
                                let is_done = matches!(item.status, Status::Completed);
                                let (_, r) = row.col(|ui| {
                                    ui.painter().rect_filled(ui.max_rect(), Rounding::ZERO, row_bg);
                                    ui.add_space(2.0);
                                    ui.add(egui::Label::new(
                                        RichText::new(&item.filename).size(11.5).color(TEXT)
                                    ).truncate()).on_hover_text(&item.url);
                                });
                                if r.clicked() { clicked = Some(item.id.clone()); }
                                r.context_menu(|ui| {
                                    ui.set_min_width(150.0);
                                    if ui.button("▶  Resume").clicked()      { action = Some((iid.clone(), "Resume")); ui.close_menu(); }
                                    if ui.button("⏸  Pause").clicked()       { action = Some((iid.clone(), "Pause")); ui.close_menu(); }
                                    ui.separator();
                                    if ui.button("📂  Open folder").clicked() { action = Some((iid.clone(), "OpenFolder")); ui.close_menu(); }
                                    if is_done && ui.button("📄  Open file").clicked() { action = Some((iid.clone(), "OpenFile")); ui.close_menu(); }
                                    ui.separator();
                                    if ui.button("🗑  Delete").clicked()      { action = Some((iid.clone(), "Delete")); ui.close_menu(); }
                                });

                                // Q ───────────────────────
                                let (_, r) = row.col(|ui| {
                                    ui.painter().rect_filled(ui.max_rect(), Rounding::ZERO, row_bg);
                                    ui.label(RichText::new(format!("{}", item.num_segments)).size(10.5).color(DIMGRAY));
                                });
                                if r.clicked() { clicked = Some(item.id.clone()); }

                                // Size ────────────────────
                                let (_, r) = row.col(|ui| {
                                    ui.painter().rect_filled(ui.max_rect(), Rounding::ZERO, row_bg);
                                    ui.label(RichText::new(
                                        item.total_bytes.map(fmt_size).unwrap_or_else(|| "—".into())
                                    ).size(11.0).color(TEXT_DIM));
                                });
                                if r.clicked() { clicked = Some(item.id.clone()); }

                                // Status + mini-bar ───────
                                let (_, r) = row.col(|ui| {
                                    ui.painter().rect_filled(ui.max_rect(), Rounding::ZERO, row_bg);
                                    let (lbl, col) = status_info(&item.status);
                                    ui.vertical(|ui| {
                                        ui.add_space(3.0);
                                        ui.label(RichText::new(lbl).size(10.0).color(col).strong());
                                        let p = item.progress();
                                        if p > 0.0 && !matches!(item.status, Status::Completed) {
                                            let bar_rect = egui::Rect::from_min_size(
                                                egui::pos2(ui.min_rect().left(), ui.min_rect().bottom() - 4.0),
                                                Vec2::new(80.0, 3.0),
                                            );
                                            ui.painter().rect_filled(bar_rect, Rounding::same(1.5), BORDER);
                                            let mut filled = bar_rect;
                                            filled.set_right(bar_rect.left() + bar_rect.width() * p);
                                            ui.painter().rect_filled(filled, Rounding::same(1.5), col);
                                        }
                                    });
                                });
                                if r.clicked() { clicked = Some(item.id.clone()); }

                                // Time left ───────────────
                                let (_, r) = row.col(|ui| {
                                    ui.painter().rect_filled(ui.max_rect(), Rounding::ZERO, row_bg);
                                    let eta = item.eta_secs().map(fmt_eta).unwrap_or_else(|| "—".into());
                                    ui.label(RichText::new(eta).size(11.0).color(TEXT_DIM));
                                });
                                if r.clicked() { clicked = Some(item.id.clone()); }

                                // Transfer rate ───────────
                                let (_, r) = row.col(|ui| {
                                    ui.painter().rect_filled(ui.max_rect(), Rounding::ZERO, row_bg);
                                    if matches!(item.status, Status::Downloading) {
                                        ui.label(RichText::new(fmt_speed(item.speed_bps)).size(11.0).color(GREEN));
                                    } else {
                                        ui.label(RichText::new("—").size(11.0).color(DIMGRAY));
                                    }
                                });
                                if r.clicked() { clicked = Some(item.id.clone()); }
                            });
                        }
                    });
            });

        if let Some(id) = clicked { self.selected_id = Some(id); }
        if let Some((id, act)) = action { self.handle_action(&id, act, items); }
    }

    fn add_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_add;
        egui::Window::new("Enter new address to download")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .fixed_size(Vec2::new(440.0, 150.0))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.style_mut().spacing.item_spacing = Vec2::new(8.0, 8.0);

                // Address row
                ui.horizontal(|ui| {
                    ui.add_sized([62.0, 22.0], egui::Label::new(
                        RichText::new("Address").size(12.0).color(TEXT_DIM)
                    ));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.dialog.url)
                            .desired_width(300.0)
                            .font(FontId::proportional(12.0))
                            .hint_text("https://example.com/file.zip"),
                    );
                    if r.changed() {
                        self.dialog.filename = filename_from_url(&self.dialog.url);
                    }
                    // dropdown indicator
                    ui.label(RichText::new("▼").size(10.0).color(GRAY));
                });

                // Save dir row
                ui.horizontal(|ui| {
                    ui.add_sized([62.0, 22.0], egui::Label::new(
                        RichText::new("Save to").size(12.0).color(TEXT_DIM)
                    ));
                    ui.add(egui::TextEdit::singleline(&mut self.dialog.save_dir)
                        .desired_width(250.0)
                        .font(FontId::proportional(12.0)));
                    if ui.small_button("Browse…").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .set_directory(&self.dialog.save_dir)
                            .pick_folder()
                        {
                            self.dialog.save_dir = p.to_string_lossy().into_owned();
                        }
                    }
                });

                // Segments row
                ui.horizontal(|ui| {
                    ui.add_sized([62.0, 22.0], egui::Label::new(
                        RichText::new("Segments").size(12.0).color(TEXT_DIM)
                    ));
                    ui.add(egui::Slider::new(&mut self.dialog.segments, 1..=32)
                        .text("connections")
                        .text_color(TEXT_DIM));
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(" Cancel ").clicked() { self.show_add = false; }
                        let can = !self.dialog.url.trim().is_empty();
                        ui.add_enabled_ui(can, |ui| {
                            if ui.add(
                                egui::Button::new(RichText::new("   OK   ").color(Color32::WHITE))
                                    .fill(ACCENT2)
                                    .rounding(3.0)
                            ).clicked() {
                                let url  = self.dialog.url.trim().to_string();
                                let dir  = PathBuf::from(&self.dialog.save_dir);
                                let name = if self.dialog.filename.is_empty() {
                                    filename_from_url(&url)
                                } else {
                                    self.dialog.filename.trim().to_string()
                                };
                                self.enqueue(url, dir.join(name), self.dialog.segments);
                                self.show_add = false;
                            }
                        });
                    });
                });
            });
        self.show_add = open;
    }

    fn settings_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        egui::Window::new("⚙  Options")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .fixed_size(Vec2::new(380.0, 130.0))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.style_mut().spacing.item_spacing = Vec2::new(8.0, 8.0);

                ui.horizontal(|ui| {
                    ui.add_sized([100.0, 22.0], egui::Label::new(
                        RichText::new("Save directory").size(12.0).color(TEXT_DIM)
                    ));
                    ui.add(egui::TextEdit::singleline(&mut self.default_save_dir)
                        .desired_width(200.0));
                    if ui.small_button("…").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .set_directory(&self.default_save_dir).pick_folder()
                        {
                            self.default_save_dir = p.to_string_lossy().into_owned();
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.add_sized([100.0, 22.0], egui::Label::new(
                        RichText::new("Default segments").size(12.0).color(TEXT_DIM)
                    ));
                    ui.add(egui::Slider::new(&mut self.default_segments, 1..=32));
                });

                ui.add_space(4.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(
                            egui::Button::new(RichText::new("  Save  ").color(Color32::WHITE))
                                .fill(ACCENT2).rounding(3.0)
                        ).clicked() { self.show_settings = false; }
                    });
                });
            });
        self.show_settings = open;
    }

    fn status_bar(&self, ui: &mut Ui, items: &[DownloadItem]) {
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = 8.0;
            let active    = items.iter().filter(|i| i.status.is_active()).count();
            let completed = items.iter().filter(|i| matches!(i.status, Status::Completed)).count();
            let failed    = items.iter().filter(|i| matches!(i.status, Status::Failed)).count();
            let speed: f64 = items.iter().map(|i| i.speed_bps).sum();

            if active > 0 {
                ui.colored_label(GREEN, format!("⬇ {}", fmt_speed(speed)));
                ui.label(RichText::new("|").color(BORDER));
                ui.label(RichText::new(format!("{active} active")).size(11.0).color(TEXT_DIM));
                ui.label(RichText::new("|").color(BORDER));
            }
            ui.label(RichText::new(format!("✔ {completed}")).size(11.0).color(TEXT_DIM));
            if failed > 0 {
                ui.label(RichText::new("|").color(BORDER));
                ui.colored_label(RED, format!("✖ {failed}"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("IDG v1.0 — Internet Download GO").size(10.5).color(DIMGRAY));
            });
        });
    }

    // ── Actions ───────────────────────────────────────────────────────────────
    fn handle_action(&mut self, id: &str, action: &str, items: &[DownloadItem]) {
        use std::sync::atomic::Ordering::Relaxed;
        match action {
            "Resume" => {
                let mut lock = self.downloads.lock().unwrap();
                if let Some(i) = lock.iter_mut().find(|i| i.id == id) {
                    i.pause_flag.store(false, Relaxed);
                    if matches!(i.status, Status::Paused) { i.status = Status::Downloading; }
                }
            }
            "Pause" => {
                let lock = self.downloads.lock().unwrap();
                if let Some(i) = lock.iter().find(|i| i.id == id) {
                    i.pause_flag.store(true, Relaxed);
                }
            }
            "Delete" => {
                {
                    let lock = self.downloads.lock().unwrap();
                    if let Some(i) = lock.iter().find(|i| i.id == id) {
                        i.cancel_flag.store(true, Relaxed);
                    }
                }
                self.downloads.lock().unwrap().retain(|i| i.id != id);
                if self.selected_id.as_deref() == Some(id) { self.selected_id = None; }
            }
            "OpenFile" => {
                if let Some(i) = items.iter().find(|i| i.id == id) { open_path(&i.save_path); }
            }
            "OpenFolder" => {
                if let Some(i) = items.iter().find(|i| i.id == id) {
                    if let Some(d) = i.save_path.parent() { open_path(d); }
                }
            }
            _ => {}
        }
    }

    fn pause_all(&self) {
        let lock = self.downloads.lock().unwrap();
        for i in lock.iter() {
            if matches!(i.status, Status::Downloading) {
                i.pause_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    fn resume_selected(&self) {
        if let Some(ref id) = self.selected_id.clone() {
            let mut lock = self.downloads.lock().unwrap();
            if let Some(i) = lock.iter_mut().find(|i| &i.id == id) {
                i.pause_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                if matches!(i.status, Status::Paused) { i.status = Status::Downloading; }
            }
        }
    }

    fn pause_selected(&self) {
        if let Some(ref id) = self.selected_id.clone() {
            let lock = self.downloads.lock().unwrap();
            if let Some(i) = lock.iter().find(|i| &i.id == id) {
                i.pause_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    fn delete_selected(&mut self) {
        if let Some(id) = self.selected_id.take() {
            use std::sync::atomic::Ordering::Relaxed;
            let lock = self.downloads.lock().unwrap();
            if let Some(i) = lock.iter().find(|i| i.id == id) {
                i.cancel_flag.store(true, Relaxed);
            }
            drop(lock);
            self.downloads.lock().unwrap().retain(|i| i.id != id);
        }
    }
}

impl eframe::App for IDGApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let items: Vec<DownloadItem> = self.downloads.lock().unwrap().clone();

        // ── Toolbar ──────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("toolbar")
            .min_height(58.0)
            .max_height(58.0)
            .frame(Frame::none().fill(TOOLBAR).inner_margin(Margin { left: 6.0, right: 6.0, top: 4.0, bottom: 0.0 }))
            .show(ctx, |ui| { self.toolbar(ui); });

        // ── Status bar ───────────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("statusbar")
            .min_height(22.0)
            .max_height(22.0)
            .frame(Frame::none().fill(Color32::from_rgb(16, 17, 26)).inner_margin(Margin::symmetric(8.0, 3.0)))
            .show(ctx, |ui| { self.status_bar(ui, &items); });

        // ── Sidebar ──────────────────────────────────────────────────────────
        egui::SidePanel::left("sidebar")
            .min_width(150.0)
            .max_width(180.0)
            .frame(Frame::none().fill(SIDEBAR).stroke(Stroke::new(1.0, BORDER)))
            .show(ctx, |ui| {
                ui.style_mut().spacing.item_spacing.y = 1.0;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.sidebar(ui, &items);
                });
            });

        // ── Main area ────────────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG))
            .show(ctx, |ui| {
                self.download_table(ui, &items);
            });

        // ── Dialogs ──────────────────────────────────────────────────────────
        if self.show_add      { self.add_dialog(ctx); }
        if self.show_settings { self.settings_dialog(ctx); }

        // Repaint while active
        let active = items.iter().any(|i| i.status.is_active());
        if active { ctx.request_repaint_after(Duration::from_millis(300)); }
    }
}

// ── Visuals ───────────────────────────────────────────────────────────────────
fn setup_visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill          = PANEL;
    v.window_fill         = Color32::from_rgb(26, 26, 38);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill   = ROW_SEL;
    v.hyperlink_color     = ACCENT;
    v.window_stroke       = Stroke::new(1.0, BORDER);
    v.window_rounding     = Rounding::same(6.0);
    v.widgets.noninteractive.bg_fill  = Color32::from_rgb(28, 28, 42);
    v.widgets.inactive.bg_fill        = Color32::from_rgb(34, 36, 52);
    v.widgets.hovered.bg_fill         = Color32::from_rgb(44, 48, 68);
    v.widgets.active.bg_fill          = ACCENT2;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, BORDER);
    ctx.set_visuals(v);

    let mut s = (*ctx.style()).clone();
    s.text_styles.insert(egui::TextStyle::Body,
        FontId::new(12.5, egui::FontFamily::Proportional));
    s.text_styles.insert(egui::TextStyle::Button,
        FontId::new(12.0, egui::FontFamily::Proportional));
    s.text_styles.insert(egui::TextStyle::Small,
        FontId::new(10.5, egui::FontFamily::Proportional));
    s.spacing.item_spacing  = Vec2::new(6.0, 4.0);
    s.spacing.window_margin = Margin::same(8.0);
    s.spacing.button_padding = Vec2::new(8.0, 4.0);
    ctx.set_style(s);
}

// ── Helpers ───────────────────────────────────────────────────────────────────
fn fmt_size(b: u64) -> String {
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1_024.0;
    let f = b as f64;
    if f >= GB      { format!("{:.2} GB", f/GB) }
    else if f >= MB { format!("{:.1} MB", f/MB) }
    else if f >= KB { format!("{:.0} KB", f/KB) }
    else            { format!("{b} B") }
}

fn fmt_speed(bps: f64) -> String {
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1_024.0;
    if bps >= MB      { format!("{:.2} MB/s", bps/MB) }
    else if bps >= KB { format!("{:.1} KB/s", bps/KB) }
    else              { format!("{:.0} B/s",  bps) }
}

fn fmt_eta(secs: f64) -> String {
    if secs < 60.0        { format!("{:.0}s",  secs) }
    else if secs < 3600.0 { format!("{:.0}m",  secs/60.0) }
    else                  { format!("{:.1}h",  secs/3600.0) }
}

fn filename_from_url(url: &str) -> String {
    url.split('?').next()
        .and_then(|p| p.rsplit('/').next())
        .filter(|s| !s.is_empty() && s.contains('.'))
        .unwrap_or("download")
        .to_string()
}

fn status_info(s: &Status) -> (&'static str, Color32) {
    match s {
        Status::Queued      => ("Queued",        GRAY),
        Status::Probing     => ("Connecting…",   ORANGE),
        Status::Downloading => ("Downloading",   Color32::from_rgb(60, 160, 240)),
        Status::Paused      => ("Paused",        ORANGE),
        Status::Completed   => ("Completed",     GREEN),
        Status::Failed      => ("Failed",        RED),
        Status::Cancelled   => ("Cancelled",     DIMGRAY),
    }
}

fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}


// ── Brand colours ─────────────────────────────────────────────────────────────
const C_BG:       Color32 = Color32::from_rgb(15, 15, 23);
const C_PANEL:    Color32 = Color32::from_rgb(22, 22, 34);
const C_TOOLBAR:  Color32 = Color32::from_rgb(18, 18, 30);
const C_ACCENT:   Color32 = Color32::from_rgb(0, 188, 212);   // teal "GO"
const C_BLUE:     Color32 = Color32::from_rgb(25, 118, 210);  // add-button
const C_GREEN:    Color32 = Color32::from_rgb(76, 175, 80);
const C_RED:      Color32 = Color32::from_rgb(239, 83, 80);
const C_ORANGE:   Color32 = Color32::from_rgb(255, 152, 0);
const C_GRAY:     Color32 = Color32::from_rgb(120, 120, 145);
const C_TEXT:     Color32 = Color32::from_rgb(220, 220, 235);
const C_SEL_ROW:  Color32 = Color32::from_rgba_premultiplied(0, 122, 204, 55);

// ── Category sidebar ───────────────────────────────────────────────────────
