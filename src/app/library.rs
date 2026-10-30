//! Compact desktop library: navigation, live summaries, transfer rows, and details.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Download,
    Close,
    Clock,
    Check,
    Pause,
    File,
    Video,
    Archive,
    Folder,
    Speed,
    List,
    Settings,
    Play,
    Remove,
    Link,
    Monitor,
    Refresh,
    Puzzle,
    Wrench,
    Reset,
    Grid,
}

pub(super) fn draw_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    let p = |x: f32, y: f32| {
        egui::pos2(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        )
    };
    let stroke = Stroke::new(1.6_f32, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], stroke);
    };
    match icon {
        Icon::Clock => {
            painter.circle_stroke(p(0.5, 0.5), rect.width() * 0.38, stroke);
            line((0.5, 0.23), (0.5, 0.5));
            line((0.5, 0.5), (0.72, 0.62));
        }
        Icon::Close => {
            line((0.2, 0.2), (0.8, 0.8));
            line((0.2, 0.8), (0.8, 0.2));
        }
        Icon::Grid => {
            for x in [0.16, 0.58] {
                for y in [0.16, 0.58] {
                    painter.rect_stroke(
                        egui::Rect::from_min_max(p(x, y), p(x + 0.25, y + 0.25)),
                        1.0,
                        stroke,
                    );
                }
            }
        }
        Icon::Monitor => {
            painter.rect_stroke(
                egui::Rect::from_min_max(p(0.1, 0.12), p(0.9, 0.7)),
                1.5,
                stroke,
            );
            line((0.5, 0.7), (0.5, 0.9));
            line((0.2, 0.9), (0.8, 0.9));
        }
        Icon::Refresh | Icon::Reset => {
            let points = (0..24)
                .map(|i| {
                    let angle = -2.7 + i as f32 / 23.0 * 5.1;
                    p(0.5 + 0.36 * angle.cos(), 0.5 + 0.36 * angle.sin())
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
            line((0.13, 0.08), (0.13, 0.38));
            line((0.13, 0.38), (0.42, 0.38));
            if matches!(icon, Icon::Refresh) {
                line((0.87, 0.62), (0.87, 0.92));
                line((0.58, 0.62), (0.87, 0.62));
            }
        }
        Icon::Puzzle => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(0.15, 0.3),
                    p(0.4, 0.3),
                    p(0.36, 0.15),
                    p(0.5, 0.08),
                    p(0.64, 0.15),
                    p(0.6, 0.3),
                    p(0.85, 0.3),
                    p(0.85, 0.55),
                    p(0.98, 0.51),
                    p(0.98, 0.72),
                    p(0.85, 0.68),
                    p(0.85, 0.9),
                    p(0.6, 0.9),
                    p(0.6, 0.75),
                    p(0.4, 0.75),
                    p(0.4, 0.9),
                    p(0.15, 0.9),
                ],
                stroke,
            ));
        }
        Icon::Wrench => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(0.58, 0.12),
                    p(0.55, 0.35),
                    p(0.7, 0.48),
                    p(0.9, 0.4),
                    p(0.83, 0.58),
                    p(0.64, 0.61),
                    p(0.3, 0.93),
                    p(0.12, 0.78),
                    p(0.48, 0.45),
                    p(0.43, 0.25),
                ],
                stroke,
            ));
        }
        Icon::Download => {
            line((0.5, 0.12), (0.5, 0.67));
            line((0.27, 0.45), (0.5, 0.68));
            line((0.5, 0.68), (0.73, 0.45));
            line((0.15, 0.65), (0.15, 0.87));
            line((0.15, 0.87), (0.85, 0.87));
            line((0.85, 0.87), (0.85, 0.65));
        }
        Icon::Check => {
            line((0.18, 0.52), (0.42, 0.75));
            line((0.42, 0.75), (0.83, 0.24));
        }
        Icon::Pause => {
            line((0.33, 0.2), (0.33, 0.8));
            line((0.67, 0.2), (0.67, 0.8));
        }
        Icon::Play | Icon::Video => {
            painter.add(egui::Shape::closed_line(
                vec![p(0.28, 0.17), p(0.82, 0.5), p(0.28, 0.83)],
                stroke,
            ));
        }
        Icon::Folder => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(0.1, 0.3),
                    p(0.1, 0.15),
                    p(0.4, 0.15),
                    p(0.52, 0.3),
                    p(0.9, 0.3),
                    p(0.9, 0.83),
                    p(0.1, 0.83),
                ],
                stroke,
            ));
        }
        Icon::Speed => {
            let gauge = Stroke::new(2.5_f32, color);
            let points = (0..40)
                .map(|i| {
                    let a =
                        std::f32::consts::PI * 0.82 + i as f32 / 39.0 * std::f32::consts::PI * 1.36;
                    p(0.5 + 0.40 * a.cos(), 0.53 + 0.40 * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(points, gauge));
            for (x, y) in [
                (0.17, 0.33),
                (0.30, 0.17),
                (0.50, 0.11),
                (0.70, 0.17),
                (0.83, 0.33),
            ] {
                painter.line_segment(
                    [p(x, y), p(0.5 + (x - 0.5) * 0.85, 0.53 + (y - 0.53) * 0.85)],
                    stroke,
                );
            }
            painter.line_segment([p(0.5, 0.53), p(0.76, 0.27)], gauge);
            painter.circle_filled(p(0.5, 0.53), rect.width() * 0.07, color);
        }
        Icon::List => {
            for y in [0.24, 0.5, 0.76] {
                painter.circle_filled(p(0.17, y), 1.4, color);
                line((0.35, y), (0.85, y));
            }
        }
        Icon::Remove => {
            line((0.15, 0.27), (0.85, 0.27));
            line((0.38, 0.13), (0.62, 0.13));
            line((0.25, 0.28), (0.3, 0.86));
            line((0.3, 0.86), (0.7, 0.86));
            line((0.7, 0.86), (0.75, 0.28));
        }
        Icon::Settings => {
            painter.circle_stroke(p(0.5, 0.5), rect.width() * 0.3, stroke);
            painter.circle_stroke(p(0.5, 0.5), rect.width() * 0.1, stroke);
            for (x, y) in [(0.5, 0.08), (0.5, 0.92), (0.08, 0.5), (0.92, 0.5)] {
                painter.circle_filled(p(x, y), 1.7, color);
            }
        }
        Icon::Link => {
            line((0.18, 0.75), (0.72, 0.22));
            line((0.43, 0.22), (0.76, 0.22));
            line((0.76, 0.22), (0.76, 0.56));
        }
        Icon::File | Icon::Archive => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(0.2, 0.08),
                    p(0.62, 0.08),
                    p(0.83, 0.3),
                    p(0.83, 0.92),
                    p(0.2, 0.92),
                ],
                stroke,
            ));
            line((0.6, 0.09), (0.6, 0.33));
            line((0.6, 0.33), (0.82, 0.33));
            if matches!(icon, Icon::Archive) {
                for y in [0.45, 0.59, 0.73] {
                    line((0.46, y), (0.6, y));
                }
            }
        }
    }
}
fn icon_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    let response = ui.add_sized([28.0, 28.0], egui::Button::new(""));
    draw_icon(
        ui.painter(),
        response.rect.shrink(6.0),
        icon,
        if response.hovered() { ACCENT } else { TEXT_DIM },
    );
    qa_control(ui.ctx(), label, response.rect);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}
fn selection_box(ui: &mut Ui, checked: &mut bool) -> egui::Response {
    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals.widgets;
        for widget in [
            &mut visuals.inactive,
            &mut visuals.hovered,
            &mut visuals.active,
        ] {
            widget.rounding = egui::Rounding::same(3.0);
        }
        ui.checkbox(checked, "")
    })
    .inner
}
fn nav_icon(category: Category) -> Icon {
    match category {
        Category::All => Icon::List,
        Category::Downloading => Icon::Download,
        Category::Queued => Icon::Clock,
        Category::Completed => Icon::Check,
        Category::Paused => Icon::Pause,
        Category::Failed => Icon::Remove,
        Category::Videos => Icon::Video,
        Category::Documents => Icon::File,
        Category::Archives => Icon::Archive,
        Category::Programs => Icon::Monitor,
        Category::Images => Icon::Grid,
        Category::Other => Icon::Folder,
    }
}
fn file_icon(item: &DownloadItem) -> (Icon, Color32) {
    match if crate::download::music::source(&item.url).is_some() {
        Category::Videos
    } else {
        file_category(&item.filename)
    } {
        Category::Videos => (Icon::Video, Color32::from_rgb(161, 119, 244)),
        Category::Archives => (Icon::Archive, Color32::from_rgb(245, 188, 77)),
        Category::Documents => (Icon::File, Color32::from_rgb(91, 175, 240)),
        Category::Programs => (Icon::Monitor, Color32::from_rgb(96, 175, 226)),
        Category::Images => (Icon::Grid, Color32::from_rgb(105, 198, 155)),
        _ => (Icon::File, TEXT_DIM),
    }
}
pub(super) fn file_category(filename: &str) -> Category {
    let extension = std::path::Path::new(filename)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "mp3" | "m4a" | "opus" | "flac" | "wav"
        | "ogg" | "aac" => Category::Videos,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "csv" | "epub"
        | "odt" => Category::Documents,
        "zip" | "rar" | "7z" | "gz" | "tar" | "bz2" | "xz" | "iso" | "vhd" => Category::Archives,
        "exe" | "msi" | "msix" | "appx" | "dmg" | "deb" | "rpm" => Category::Programs,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" => Category::Images,
        _ => Category::Other,
    }
}
fn matches_search(item: &DownloadItem, query: &str) -> bool {
    query.is_empty()
        || item.filename.to_lowercase().contains(query)
        || host(&item.url).to_lowercase().contains(query)
}
fn host(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default()
}
#[derive(Default)]
pub(super) struct LibraryState {
    pub(super) selected_ids: std::collections::HashSet<String>,
    pub(super) grid: bool,
    speed_history: std::collections::VecDeque<f64>,
    sample_at: Option<std::time::Instant>,
    pub(super) confirm_remove: bool,
}
#[cfg(debug_assertions)]
impl LibraryState {
    pub(super) fn preview_activity(&mut self) {
        self.speed_history = (0..60)
            .map(|i| {
                (i as f64 * 0.18).sin().mul_add(700_000.0, 4_800_000.0)
                    + (i as f64 * 0.47).cos() * 180_000.0
            })
            .collect();
    }
}
fn card() -> Frame {
    Frame::none()
        .fill(PANEL)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(12.0)
        .inner_margin(Margin::same(14.0))
}

#[derive(Default)]
struct TransferInteraction {
    clicked: Option<Option<String>>,
    action: Option<(String, &'static str)>,
    toggled: Option<(String, bool)>,
}

fn paint_label(ui: &Ui, rect: egui::Rect, text: &str, size: f32, color: Color32, strong: bool) {
    if rect.width() <= 0.0 {
        return;
    }
    let font = FontId::new(
        size,
        if strong {
            egui::FontFamily::Name("idg-heading".into())
        } else {
            egui::FontFamily::Proportional
        },
    );
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(
            egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
}

fn selection_at(ui: &mut Ui, rect: egui::Rect, checked: &mut bool) -> egui::Response {
    ui.scope(|ui| {
        let widgets = &mut ui.visuals_mut().widgets;
        widgets.inactive.rounding = Rounding::same(3.0);
        widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(83, 93, 112));
        widgets.hovered.rounding = Rounding::same(3.0);
        widgets.active.rounding = Rounding::same(3.0);
        ui.put(rect, egui::Checkbox::without_text(checked))
    })
    .inner
}

fn transfer_columns(width: f32) -> [f32; 9] {
    let mut widths = [
        26.0,
        0.0,
        if width >= 1000.0 {
            75.0
        } else if width >= 820.0 {
            65.0
        } else {
            0.0
        },
        if width >= 900.0 { 110.0 } else { 90.0 },
        if width >= 1050.0 {
            230.0
        } else if width >= 820.0 {
            170.0
        } else {
            120.0
        },
        if width >= 900.0 { 90.0 } else { 78.0 },
        if width >= 1150.0 { 70.0 } else { 0.0 },
        if width >= 1150.0 {
            120.0
        } else if width >= 820.0 {
            95.0
        } else {
            0.0
        },
        28.0,
    ];
    let gaps = widths.iter().filter(|w| **w > 0.0).count() as f32 * 8.0;
    widths[1] = (width - widths.iter().sum::<f32>() - gaps).max(120.0);
    widths
}

fn column_rects(rect: egui::Rect, widths: &[f32; 9]) -> [egui::Rect; 9] {
    let mut x = rect.left();
    std::array::from_fn(|i| {
        let r = egui::Rect::from_min_size(
            egui::pos2(x, rect.top()),
            egui::vec2(widths[i], rect.height()),
        );
        if widths[i] > 0.0 {
            x += widths[i] + 8.0;
        }
        r
    })
}

impl IDGApp {
    pub(super) fn sidebar(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        let compact = ui.ctx().screen_rect().height() < 720.0;
        if compact {
            ui.spacing_mut().interact_size.y = 22.0;
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.spacing_mut().button_padding.y = 1.0;
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 9.0, ACCENT);
            draw_icon(ui.painter(), r.shrink(8.0), Icon::Download, TEXT);
            ui.add_space(4.0);
            ui.label(RichText::new("IDG").size(22.0).strong());
            ui.label(RichText::new("GO").size(22.0).strong().color(ACCENT));
        });
        ui.add_space(if compact { 2.0 } else { 16.0 });
        for (title, categories) in [
            (
                "DOWNLOADS",
                &[
                    Category::All,
                    Category::Downloading,
                    Category::Queued,
                    Category::Completed,
                    Category::Paused,
                    Category::Failed,
                ][..],
            ),
            (
                "FILE TYPES",
                &[
                    Category::Videos,
                    Category::Documents,
                    Category::Archives,
                    Category::Programs,
                    Category::Images,
                    Category::Other,
                ][..],
            ),
        ] {
            ui.label(RichText::new(title).size(11.0).strong().color(DIMGRAY));
            ui.add_space(if compact { 1.0 } else { 5.0 });
            for &category in categories {
                let selected = !self.show_settings && self.category == category;
                let response = ui.add_sized(
                    [ui.available_width(), if compact { 22.0 } else { 32.0 }],
                    egui::Button::new("")
                        .frame(false)
                        .fill(if selected {
                            ROW_SEL
                        } else {
                            Color32::TRANSPARENT
                        })
                        .rounding(6.0),
                );
                let r = response.rect;
                if response.hovered() && !selected {
                    ui.painter().rect_filled(r, 6.0, PANEL);
                }
                if selected {
                    ui.painter().rect_stroke(
                        r.shrink(0.5),
                        6.0,
                        Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.4)),
                    );
                }
                draw_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(
                        egui::pos2(r.left() + 16.0, r.center().y),
                        Vec2::splat(17.0),
                    ),
                    nav_icon(category),
                    if selected { TEXT } else { TEXT_DIM },
                );
                paint_label(
                    ui,
                    egui::Rect::from_min_max(
                        egui::pos2(r.left() + 33.0, r.top()),
                        egui::pos2(r.right() - 30.0, r.bottom()),
                    ),
                    category.label(),
                    14.0,
                    if selected { TEXT } else { TEXT_DIM },
                    selected,
                );
                ui.painter().text(
                    egui::pos2(r.right() - 10.0, r.center().y),
                    egui::Align2::RIGHT_CENTER,
                    category.count(items),
                    FontId::proportional(12.0),
                    TEXT_DIM,
                );
                qa_control(ui.ctx(), category.label(), r);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, category.label())
                });
                if response.on_hover_text(category.label()).clicked() {
                    self.category = category;
                    self.show_settings = false;
                    self.settings_draft = None;
                }
            }
            ui.add_space(if compact { 2.0 } else { 16.0 });
        }
        let footer = if compact { 88.0 } else { 112.0 };
        if ui.available_height() > footer + 137.0 {
            ui.add_space((ui.available_height() - footer - 137.0).max(0.0));
            self.speed_overview(ui, items);
            ui.add_space(10.0);
        } else {
            ui.add_space((ui.available_height() - footer).max(0.0));
        }
        ui.separator();
        for (icon, label) in [
            (Icon::Settings, "Settings"),
            (Icon::Folder, "Download folder"),
        ] {
            let response = ui.add_sized(
                [ui.available_width(), if compact { 26.0 } else { 32.0 }],
                egui::Button::new("")
                    .frame(false)
                    .fill(if label == "Settings" && self.show_settings {
                        ROW_SEL
                    } else {
                        Color32::TRANSPARENT
                    })
                    .rounding(6.0),
            );
            let r = response.rect;
            draw_icon(
                ui.painter(),
                egui::Rect::from_center_size(
                    egui::pos2(r.left() + 16.0, r.center().y),
                    Vec2::splat(17.0),
                ),
                icon,
                TEXT_DIM,
            );
            paint_label(
                ui,
                egui::Rect::from_min_max(egui::pos2(r.left() + 34.0, r.top()), r.right_bottom()),
                label,
                13.0,
                TEXT,
                false,
            );
            qa_control(ui.ctx(), label, r);
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
            if response.clicked() {
                if label == "Settings" {
                    self.open_settings();
                } else {
                    open_path(std::path::Path::new(&self.default_save_dir));
                }
            }
        }
        ui.horizontal(|ui| {
            let unavailable = self.browser_unavailable(ui.ctx());
            let color = if unavailable { ORANGE } else { SUCCESS };
            let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 3.5, color);
            ui.label(
                RichText::new(if unavailable {
                    "Browser unavailable"
                } else {
                    "Browser integration ready"
                })
                .size(11.5)
                .color(color),
            );
        });
    }
    pub(super) fn library_header(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        let speed: f64 = items
            .iter()
            .filter(|i| i.status.is_active())
            .map(|i| i.speed_bps)
            .sum();
        if self
            .library_state
            .sample_at
            .is_none_or(|at| at.elapsed().as_secs_f32() >= 0.5)
        {
            self.library_state.speed_history.push_back(speed.max(0.0));
            while self.library_state.speed_history.len() > 60 {
                self.library_state.speed_history.pop_front();
            }
            self.library_state.sample_at = Some(std::time::Instant::now());
        }
        self.process_library_commands(ui.ctx());
        let query = self.search.trim().to_lowercase();
        let shown: Vec<_> = items
            .iter()
            .filter(|i| self.category.matches(i) && matches_search(i, &query))
            .collect();
        let inline = ui.available_width() >= 980.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.vertical(|ui| {
                ui.set_width(if inline {
                    240.0
                } else {
                    (ui.available_width() - 240.0).clamp(170.0, 300.0)
                });
                ui.label(
                    RichText::new(self.category.label())
                        .family(egui::FontFamily::Name("idg-heading".into()))
                        .size(25.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!(
                        "{} files  ·  {} active  ·  {} total",
                        shown.len(),
                        shown.iter().filter(|i| i.status.is_active()).count(),
                        fmt_size(shown.iter().filter_map(|i| i.total_bytes).sum())
                    ))
                    .size(12.0)
                    .color(TEXT_DIM),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.library_views(ui);
                if inline {
                    ui.add_space(8.0);
                    self.status_filters(ui, items, true);
                }
            });
        });
        if !inline {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| self.status_filters(ui, items, false));
        }
        if ui.ctx().screen_rect().width() < 1050.0 && !self.library_state.selected_ids.is_empty() {
            ui.horizontal(|ui| {
                self.toolbar(ui);
                ui.label(
                    RichText::new(format!(
                        "{} selected",
                        self.library_state.selected_ids.len()
                    ))
                    .size(12.0)
                    .color(TEXT_DIM),
                );
            });
        }
        ui.add_space(10.0);
    }

    fn status_filters(&mut self, ui: &mut Ui, items: &[DownloadItem], reverse: bool) {
        let mut filters = vec![
            (Category::All, "All"),
            (Category::Downloading, "Active"),
            (Category::Queued, "Queued"),
            (Category::Paused, "Paused"),
            (Category::Completed, "Completed"),
            (Category::Failed, "Failed"),
        ];
        if reverse {
            filters.reverse();
        }
        ui.spacing_mut().item_spacing.x = 3.0;
        for (category, label) in filters {
            let response = ui.add(
                egui::Button::new(
                    RichText::new(format!("{label}  {}", category.count(items))).size(13.0),
                )
                .fill(if self.category == category {
                    ROW_SEL
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(Stroke::new(
                    1.0_f32,
                    if self.category == category {
                        BORDER
                    } else {
                        Color32::TRANSPARENT
                    },
                ))
                .rounding(6.0)
                .min_size(Vec2::new(58.0, 30.0)),
            );
            qa_control(ui.ctx(), &format!("Filter {label}"), response.rect);
            if response.clicked() {
                self.category = category;
            }
        }
    }

    fn library_views(&mut self, ui: &mut Ui) {
        for (grid, icon, label) in [
            (true, Icon::Grid, "Grid view"),
            (false, Icon::List, "List view"),
        ] {
            let r = ui.add_sized(
                [32.0, 32.0],
                egui::Button::new("")
                    .fill(if self.library_state.grid == grid {
                        ROW_SEL
                    } else {
                        PANEL
                    })
                    .rounding(6.0),
            );
            draw_icon(
                ui.painter(),
                r.rect.shrink(8.0),
                icon,
                if self.library_state.grid == grid {
                    ACCENT
                } else {
                    TEXT_DIM
                },
            );
            qa_control(ui.ctx(), label, r.rect);
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
            if r.on_hover_text(label).clicked() {
                self.library_state.grid = grid;
            }
        }
        let sort = egui::ComboBox::from_id_salt("library-sort")
            .selected_text(if self.newest_first {
                "Newest first"
            } else {
                "Oldest first"
            })
            .width(110.0)
            .show_ui(ui, |ui| {
                let a = ui.selectable_value(&mut self.newest_first, true, "Newest first");
                qa_control(ui.ctx(), "Newest first", a.rect);
                let b = ui.selectable_value(&mut self.newest_first, false, "Oldest first");
                qa_control(ui.ctx(), "Oldest first", b.rect);
            });
        qa_control(ui.ctx(), "Sort", sort.response.rect);
    }
    fn speed_overview(&self, ui: &mut Ui, items: &[DownloadItem]) {
        let speed: f64 = items
            .iter()
            .filter(|i| i.status.is_active())
            .map(|i| i.speed_bps)
            .sum();
        Frame::none()
            .fill(BG)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .rounding(9.0)
            .inner_margin(Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width((ui.available_width()).max(100.0));
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
                    draw_icon(ui.painter(), rect, Icon::Speed, ACCENT);
                    ui.label(RichText::new("TOTAL SPEED").size(11.0).color(TEXT_DIM));
                });
                ui.label(RichText::new(fmt_speed(speed)).size(24.0).strong());
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 36.0),
                    egui::Sense::hover(),
                );
                let history = &self.library_state.speed_history;
                let peak = history.iter().copied().fold(1.0_f64, f64::max);
                if history.len() > 1 {
                    let points: Vec<_> = history
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            egui::pos2(
                                rect.left() + rect.width() * i as f32 / (history.len() - 1) as f32,
                                rect.bottom() - 2.0 - (v / peak) as f32 * (rect.height() - 4.0),
                            )
                        })
                        .collect();
                    ui.painter()
                        .add(egui::Shape::line(points, Stroke::new(1.8_f32, ACCENT)));
                }
                ui.label(
                    RichText::new(format!(
                        "{} active  ·  Peak {}",
                        items.iter().filter(|i| i.status.is_active()).count(),
                        fmt_speed(peak.max(speed))
                    ))
                    .size(11.0)
                    .color(TEXT_DIM),
                );
            });
    }

    pub(super) fn toolbar(&mut self, ui: &mut Ui) {
        let items = self.downloads.lock().unwrap().clone();
        let selected: Vec<_> = items
            .iter()
            .filter(|i| self.library_state.selected_ids.contains(&i.id))
            .collect();
        let mut command = None;
        for (icon, label, enabled) in [
            (
                Icon::Play,
                "Resume",
                selected.iter().any(|i| i.status == Status::Paused),
            ),
            (
                Icon::Pause,
                "Pause",
                selected
                    .iter()
                    .any(|i| i.status.is_active() && i.supports_pause()),
            ),
            (Icon::Remove, "Remove", !selected.is_empty()),
            (Icon::Folder, "Open folder", true),
        ] {
            let response = ui.add_enabled(
                enabled,
                egui::Button::new("")
                    .min_size(Vec2::splat(34.0))
                    .frame(false)
                    .rounding(6.0),
            );
            if response.hovered() && enabled {
                ui.painter().rect_filled(response.rect, 6.0, PANEL);
            }
            draw_icon(
                ui.painter(),
                response.rect.shrink(8.0),
                icon,
                if enabled {
                    TEXT_DIM
                } else {
                    GRAY.gamma_multiply(0.55)
                },
            );
            qa_control(ui.ctx(), label, response.rect);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label)
            });
            if response.on_hover_text(label).clicked() {
                if label == "Remove" {
                    self.library_state.confirm_remove = true;
                } else if label == "Open folder" && selected.is_empty() {
                    open_path(std::path::Path::new(&self.default_save_dir));
                } else {
                    command = Some(if label == "Open folder" {
                        "OpenFolder"
                    } else {
                        label
                    });
                }
            }
        }
        if let Some(command) = command {
            let ids: Vec<_> = self.library_state.selected_ids.iter().cloned().collect();
            for id in ids {
                self.handle_action(&id, command, &items);
            }
        }
    }

    fn process_library_commands(&mut self, ctx: &egui::Context) {
        let items = self.downloads.lock().unwrap().clone();
        self.library_state
            .selected_ids
            .retain(|id| items.iter().any(|i| &i.id == id));
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::Delete))
                && !self.library_state.selected_ids.is_empty()
            {
                self.library_state.confirm_remove = true;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                let pause = items.iter().any(|i| {
                    self.library_state.selected_ids.contains(&i.id)
                        && i.status.is_active()
                        && i.supports_pause()
                });
                let ids: Vec<_> = self.library_state.selected_ids.iter().cloned().collect();
                for id in ids {
                    self.handle_action(&id, if pause { "Pause" } else { "Resume" }, &items);
                }
            }
        }
        let ids: Vec<_> = self.library_state.selected_ids.iter().cloned().collect();
        if self.library_state.confirm_remove {
            egui::Window::new("Remove selected downloads?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Remove {} selected download(s) from the list?",
                        ids.len()
                    ));
                    ui.label("Active transfers will stop. Downloaded files will remain on disk.");
                    ui.horizontal(|ui| {
                        let cancel = ui.button("Cancel");
                        qa_control(ctx, "Cancel remove", cancel.rect);
                        if cancel.clicked() {
                            self.library_state.confirm_remove = false;
                        }
                        if ui
                            .button(RichText::new("Remove from list").color(RED))
                            .clicked()
                        {
                            for id in &ids {
                                self.handle_action(id, "Delete", &items);
                            }
                            self.library_state.selected_ids.clear();
                            self.library_state.confirm_remove = false;
                        }
                    });
                });
        }
    }
    fn transfer_rows(
        &mut self,
        ui: &mut Ui,
        items: &[&DownloadItem],
        height: f32,
    ) -> TransferInteraction {
        let mut result = TransferInteraction::default();
        let width = (ui.available_width() - ui.spacing().scroll.allocated_width()).max(100.0);
        let widths = transfer_columns(width);
        let (header, _) = ui.allocate_exact_size(Vec2::new(width, 34.0), egui::Sense::hover());
        let cols = column_rects(header, &widths);
        let mut all = !items.is_empty()
            && items
                .iter()
                .all(|i| self.library_state.selected_ids.contains(&i.id));
        let select = selection_at(
            ui,
            egui::Rect::from_center_size(cols[0].center(), Vec2::splat(22.0)),
            &mut all,
        );
        qa_control(ui.ctx(), "Select all", select.rect);
        if select.changed() {
            for item in items {
                if all {
                    self.library_state.selected_ids.insert(item.id.clone());
                } else {
                    self.library_state.selected_ids.remove(&item.id);
                }
            }
        }
        for (i, label) in [
            (1, "Name"),
            (2, "Size"),
            (3, "Status"),
            (4, "Progress"),
            (5, "Speed"),
            (6, "Time left"),
            (7, "Added"),
        ] {
            paint_label(ui, cols[i], label, 12.0, TEXT_DIM, false);
        }
        ui.painter().hline(
            header.x_range(),
            header.bottom(),
            Stroke::new(1.0_f32, BORDER),
        );
        let row_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height((height - 36.0).max(60.0))
            .show_rows(ui, 52.0, items.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let (block, _) = ui.allocate_exact_size(
                    Vec2::new(width, range.len() as f32 * 52.0),
                    egui::Sense::hover(),
                );
                for (visible, index) in range.enumerate() {
                    let item = items[index];
                    let rect = egui::Rect::from_min_size(
                        block.min + Vec2::new(0.0, visible as f32 * 52.0),
                        Vec2::new(width, 52.0),
                    );
                    let cols = column_rects(rect, &widths);
                    let response = ui.interact(
                        rect,
                        egui::Id::new(("transfer-row", &item.id)),
                        egui::Sense::click(),
                    );
                    qa_control(ui.ctx(), &format!("Row {}", item.filename), rect);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &item.filename)
                    });
                    let selected = self.library_state.selected_ids.contains(&item.id);
                    if selected || response.hovered() {
                        ui.painter().rect_filled(
                            rect.shrink2(Vec2::new(0.0, 1.0)),
                            4.0,
                            if selected { ROW_SEL } else { PANEL },
                        );
                    }
                    ui.painter().hline(
                        rect.x_range(),
                        rect.bottom(),
                        Stroke::new(1.0_f32, BORDER.gamma_multiply(0.45)),
                    );
                    let mut checked = selected;
                    let checkbox = selection_at(
                        ui,
                        egui::Rect::from_center_size(cols[0].center(), Vec2::splat(22.0)),
                        &mut checked,
                    );
                    checkbox.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Checkbox,
                            true,
                            format!("Select {}", item.filename),
                        )
                    });
                    if checkbox.changed() {
                        result.toggled = Some((item.id.clone(), checked));
                    }
                    let (icon, color) = file_icon(item);
                    let glyph = egui::Rect::from_center_size(
                        egui::pos2(cols[1].left() + 15.0, rect.center().y),
                        Vec2::splat(30.0),
                    );
                    ui.painter()
                        .rect_filled(glyph, 6.0, color.gamma_multiply(0.12));
                    ui.painter().rect_stroke(
                        glyph,
                        6.0,
                        Stroke::new(1.0_f32, color.gamma_multiply(0.25)),
                    );
                    draw_icon(ui.painter(), glyph.shrink(7.0), icon, color);
                    let name = egui::Rect::from_min_max(
                        egui::pos2(cols[1].left() + 41.0, rect.top() + 5.0),
                        egui::pos2(cols[1].right(), rect.center().y + 3.0),
                    );
                    paint_label(ui, name, &item.filename, 14.0, TEXT, true);
                    qa_control(
                        ui.ctx(),
                        &format!("File name {}", item.filename),
                        egui::Rect::from_min_size(
                            name.min,
                            Vec2::new(name.width().min(160.0), name.height()),
                        ),
                    );
                    let source = host(&item.url);
                    paint_label(
                        ui,
                        name.translate(Vec2::new(0.0, 20.0)),
                        &source,
                        11.5,
                        TEXT_DIM,
                        false,
                    );
                    ui.interact(
                        cols[1],
                        egui::Id::new(("transfer-name", &item.id)),
                        egui::Sense::hover(),
                    )
                    .on_hover_text(format!("{}\n{}", item.filename, source));
                    qa_control(ui.ctx(), "Table Name", cols[1]);
                    let total = item.total_bytes.or_else(|| {
                        (item.status == Status::Completed).then_some(item.downloaded_bytes)
                    });
                    paint_label(
                        ui,
                        cols[2],
                        &total.map(fmt_size).unwrap_or_else(|| "Unknown".into()),
                        13.0,
                        TEXT_DIM,
                        false,
                    );
                    let (status, color) = status_info(&item.status);
                    ui.painter().circle_filled(
                        egui::pos2(cols[3].left() + 4.0, rect.center().y),
                        3.0,
                        color,
                    );
                    paint_label(
                        ui,
                        cols[3]
                            .shrink2(Vec2::new(12.0, 0.0))
                            .translate(Vec2::new(3.0, 0.0)),
                        status,
                        12.5,
                        color,
                        false,
                    );
                    ui.interact(
                        cols[3],
                        egui::Id::new(("transfer-status", &item.id)),
                        egui::Sense::hover(),
                    )
                    .on_hover_text(item.error.as_deref().unwrap_or(status));
                    let progress = if item.status == Status::Completed {
                        1.0
                    } else {
                        item.progress()
                    };
                    let progress_text = if total.is_some() {
                        format!("{:.0}%", progress * 100.0)
                    } else {
                        "—".into()
                    };
                    let p = cols[4];
                    paint_label(
                        ui,
                        egui::Rect::from_min_max(
                            p.left_top(),
                            egui::pos2(p.left() + 44.0, p.center().y + 2.0),
                        ),
                        &progress_text,
                        13.0,
                        TEXT,
                        true,
                    );
                    let bytes = if p.width() >= 170.0 {
                        format!(
                            "{} / {}",
                            fmt_size(item.downloaded_bytes),
                            total.map(fmt_size).unwrap_or_else(|| "?".into())
                        )
                    } else {
                        fmt_size(item.downloaded_bytes)
                    };
                    let galley =
                        ui.painter()
                            .layout_no_wrap(bytes, FontId::proportional(11.0), TEXT_DIM);
                    ui.painter().with_clip_rect(p).galley(
                        egui::pos2(p.right() - galley.size().x, p.top() + 10.0),
                        galley,
                        TEXT_DIM,
                    );
                    let track = egui::Rect::from_min_size(
                        egui::pos2(p.left(), p.center().y + 8.0),
                        Vec2::new(p.width(), 4.0),
                    );
                    ui.painter()
                        .rect_filled(track, 2.0, BORDER.gamma_multiply(0.65));
                    if progress > 0.0 {
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                track.min,
                                Vec2::new(track.width() * progress.clamp(0.0, 1.0), 4.0),
                            ),
                            2.0,
                            color,
                        );
                    }
                    let speed = if item.status.is_active() {
                        fmt_speed(item.speed_bps)
                    } else {
                        "—".into()
                    };
                    paint_label(ui, cols[5], &speed, 12.5, TEXT_DIM, false);
                    ui.interact(
                        cols[5],
                        egui::Id::new(("transfer-speed", &item.id)),
                        egui::Sense::hover(),
                    )
                    .on_hover_text(&speed);
                    paint_label(
                        ui,
                        cols[6],
                        &item.eta_secs().map(fmt_eta).unwrap_or_else(|| "—".into()),
                        12.0,
                        TEXT_DIM,
                        false,
                    );
                    let added = item.added_at.with_timezone(&chrono::Local);
                    let date = if added.date_naive() == chrono::Local::now().date_naive() {
                        added.format("Today %H:%M").to_string()
                    } else {
                        added.format("%d %b %Y").to_string()
                    };
                    paint_label(ui, cols[7], &date, 11.5, TEXT_DIM, false);
                    let folder = ui.put(
                        egui::Rect::from_center_size(cols[8].center(), Vec2::splat(28.0)),
                        egui::Button::new("").frame(false),
                    );
                    draw_icon(
                        ui.painter(),
                        folder.rect.shrink(6.0),
                        Icon::Folder,
                        TEXT_DIM,
                    );
                    qa_control(ui.ctx(), "Table Folder", folder.rect);
                    if folder.on_hover_text("Open folder").clicked() {
                        result.action = Some((item.id.clone(), "OpenFolder"));
                    }
                    if response.clicked() && result.toggled.is_none() && result.action.is_none() {
                        result.clicked = Some(Some(item.id.clone()));
                    }
                    if response.double_clicked() && item.status == Status::Completed {
                        result.action = Some((item.id.clone(), "OpenFile"));
                    }
                    response.context_menu(|ui| {
                        for (label, command) in [
                            ("Resume", "Resume"),
                            ("Pause", "Pause"),
                            ("Copy link", "CopyLink"),
                            ("Open file", "OpenFile"),
                            ("Open folder", "OpenFolder"),
                            ("Remove", "Delete"),
                        ] {
                            let enabled = match command {
                                "Resume" => item.status == Status::Paused,
                                "Pause" => item.status.is_active() && item.supports_pause(),
                                "OpenFile" => item.status == Status::Completed,
                                _ => true,
                            };
                            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                result.action = Some((item.id.clone(), command));
                                ui.close_menu();
                            }
                        }
                    });
                }
            });
        ui.spacing_mut().item_spacing.y = row_spacing;
        result
    }

    pub(super) fn download_table(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        let query = self.search.trim().to_lowercase();
        let mut filtered: Vec<_> = items
            .iter()
            .filter(|item| self.category.matches(item) && matches_search(item, &query))
            .collect();
        #[cfg(debug_assertions)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("qa:visible-downloads"), filtered.len()));
        filtered.sort_by_key(|item| item.added_at);
        if self.newest_first {
            filtered.reverse();
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A))
            && !ui.ctx().wants_keyboard_input()
        {
            self.library_state
                .selected_ids
                .extend(filtered.iter().map(|i| i.id.clone()));
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.library_state.selected_ids.clear();
            self.selected_id = None;
        }
        let inspector = self
            .selected_id
            .as_ref()
            .and_then(|id| filtered.iter().find(|i| &i.id == id))
            .copied()
            .cloned();
        let detail_height = if inspector.is_some() && ui.available_height() > 280.0 {
            148.0
        } else {
            0.0
        };
        let table_height = (ui.available_height() - 8.0 - detail_height).max(90.0);
        let mut clicked = None;
        let mut action = None;
        let mut toggled = None;
        let table = Frame::none().inner_margin(Margin::ZERO).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(table_height);
            if self.library_state.grid && !filtered.is_empty() {
                egui::ScrollArea::vertical()
                    .max_height(table_height - 8.0)
                    .show(ui, |ui| {
                        let gap = 12.0;
                        let columns = (((ui.available_width() + gap) / (255.0 + gap)).floor()
                            as usize)
                            .max(1);
                        let width =
                            (ui.available_width() - 12.0 * (columns - 1) as f32) / columns as f32;
                        for chunk in filtered.chunks(columns) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                for item in chunk {
                                    card()
                                        .fill(
                                            if self.library_state.selected_ids.contains(&item.id) {
                                                ROW_SEL
                                            } else {
                                                BG
                                            },
                                        )
                                        .show(ui, |ui| {
                                            // Frames inherit the row layout; each card needs its own
                                            // vertical flow so metadata cannot collide with the title.
                                            ui.with_layout(
                                                egui::Layout::top_down(egui::Align::Min),
                                                |ui| {
                                                    ui.set_width((width - 30.0).max(100.0));
                                                    ui.set_min_height(166.0);
                                                    let mut checked = self
                                                        .library_state
                                                        .selected_ids
                                                        .contains(&item.id);
                                                    ui.horizontal(|ui| {
                                                        if selection_box(ui, &mut checked).changed()
                                                        {
                                                            toggled =
                                                                Some((item.id.clone(), checked));
                                                        }
                                                        let (icon, color) = file_icon(item);
                                                        let (r, _) = ui.allocate_exact_size(
                                                            Vec2::splat(25.0),
                                                            egui::Sense::hover(),
                                                        );
                                                        draw_icon(ui.painter(), r, icon, color);
                                                        if ui
                                                            .add(
                                                                egui::Label::new(
                                                                    RichText::new(&item.filename)
                                                                        .strong(),
                                                                )
                                                                .truncate()
                                                                .sense(egui::Sense::click()),
                                                            )
                                                            .on_hover_text(&item.filename)
                                                            .clicked()
                                                        {
                                                            clicked = Some(Some(item.id.clone()));
                                                        }
                                                    });
                                                    let (label, color) = status_info(&item.status);
                                                    ui.colored_label(color, label);
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(host(&item.url))
                                                                .size(11.0)
                                                                .color(TEXT_DIM),
                                                        )
                                                        .truncate(),
                                                    );
                                                    ui.add(
                                                        egui::ProgressBar::new(
                                                            if item.status == Status::Completed {
                                                                1.0
                                                            } else {
                                                                item.progress()
                                                            },
                                                        )
                                                        .desired_height(5.0)
                                                        .fill(status_info(&item.status).1),
                                                    );
                                                    ui.label(format!(
                                                        "{}   {}",
                                                        fmt_size(item.downloaded_bytes),
                                                        if item.status.is_active() {
                                                            fmt_speed(item.speed_bps)
                                                        } else {
                                                            String::new()
                                                        }
                                                    ));
                                                    ui.horizontal(|ui| {
                                                        if item.status == Status::Paused
                                                            && icon_button(ui, Icon::Play, "Resume")
                                                                .clicked()
                                                        {
                                                            action =
                                                                Some((item.id.clone(), "Resume"));
                                                        }
                                                        if item.status.is_active()
                                                            && item.supports_pause()
                                                            && icon_button(ui, Icon::Pause, "Pause")
                                                                .clicked()
                                                        {
                                                            action =
                                                                Some((item.id.clone(), "Pause"));
                                                        }
                                                        if icon_button(
                                                            ui,
                                                            Icon::Folder,
                                                            "Open folder",
                                                        )
                                                        .clicked()
                                                        {
                                                            action = Some((
                                                                item.id.clone(),
                                                                "OpenFolder",
                                                            ));
                                                        }
                                                    });
                                                },
                                            );
                                        });
                                }
                            });
                            ui.add_space(10.0);
                        }
                    });
                return;
            }
            let result = self.transfer_rows(ui, &filtered, table_height);
            clicked = result.clicked;
            action = result.action;
            toggled = result.toggled;
        });
        if filtered.is_empty() {
            let rect = table.response.rect;
            let scale = ((rect.height() - 52.0) / 410.0).clamp(0.45, 1.0);
            let center = egui::pos2(
                rect.center().x,
                rect.top()
                    + 42.0
                    + (rect.height() - 42.0 - 236.0 * scale).max(0.0) * 0.5
                    + 73.0 * scale,
            );
            if query.is_empty() && rect.height() < 220.0 {
                let body = egui::Rect::from_min_max(
                    rect.left_top() + Vec2::new(12.0, 38.0),
                    rect.right_bottom() - Vec2::splat(12.0),
                );
                ui.painter().text(
                    egui::pos2(body.left() + 6.0, body.center().y),
                    egui::Align2::LEFT_CENTER,
                    "No downloads yet",
                    FontId::new(15.0, egui::FontFamily::Name("idg-heading".into())),
                    TEXT,
                );
            } else if query.is_empty() {
                draw_empty_folder(
                    ui.painter(),
                    egui::Rect::from_center_size(center, Vec2::new(210.0, 146.0) * scale),
                );
                ui.painter().text(
                    center + Vec2::new(0.0, 104.0) * scale,
                    egui::Align2::CENTER_CENTER,
                    "No downloads yet",
                    FontId::new(
                        24.0 * scale.max(0.7),
                        egui::FontFamily::Name("idg-heading".into()),
                    ),
                    TEXT,
                );
                ui.painter().text(
                    center + Vec2::new(0.0, 140.0) * scale,
                    egui::Align2::CENTER_CENTER,
                    "Add a link or download from your browser",
                    FontId::proportional(16.0 * scale.max(0.65)),
                    TEXT_DIM,
                );
                ui.painter().text(
                    center + Vec2::new(0.0, 163.0) * scale,
                    egui::Align2::CENTER_CENTER,
                    "to get started.",
                    FontId::proportional(16.0 * scale.max(0.65)),
                    TEXT_DIM,
                );
            } else {
                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "No downloads match your search.",
                    FontId::proportional(16.0 * scale.max(0.65)),
                    TEXT_DIM,
                );
            }
        }
        if let Some(item) = &inspector
            && detail_height > 0.0
        {
            ui.add_space(8.0);
            Frame::none()
                .fill(PANEL)
                .stroke(Stroke::new(1.0_f32, BORDER))
                .rounding(8.0)
                .inner_margin(Margin::same(12.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (icon, color) = file_icon(item);
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::hover());
                        draw_icon(ui.painter(), r.shrink(3.0), icon, color);
                        ui.add_sized(
                            [(ui.available_width() - 240.0).max(80.0), 28.0],
                            egui::Label::new(RichText::new(&item.filename).size(15.0).strong())
                                .halign(egui::Align::Min)
                                .truncate(),
                        )
                        .on_hover_text(&item.filename);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, Icon::Close, "Close details").clicked() {
                                self.selected_id = None;
                            }
                            if icon_button(ui, Icon::Link, "Copy download link").clicked() {
                                ui.ctx().copy_text(item.url.clone());
                            }
                            if icon_button(ui, Icon::Folder, "Details open folder").clicked() {
                                action = Some((item.id.clone(), "OpenFolder"));
                            }
                            if ui
                                .add_enabled(
                                    item.status == Status::Completed,
                                    egui::Button::new("Open file"),
                                )
                                .clicked()
                            {
                                action = Some((item.id.clone(), "OpenFile"));
                            }
                        });
                    });
                    ui.add(
                        egui::Label::new(
                            RichText::new(item.save_path.display().to_string())
                                .size(12.0)
                                .color(TEXT_DIM),
                        )
                        .truncate(),
                    )
                    .on_hover_text(item.save_path.display().to_string());
                    ui.add_space(7.0);
                    let (label, color) = status_info(&item.status);
                    let values = [
                        ("STATUS", label.to_owned()),
                        (
                            "TRANSFERRED",
                            format!(
                                "{} / {}",
                                fmt_size(item.downloaded_bytes),
                                item.total_bytes
                                    .map(fmt_size)
                                    .unwrap_or_else(|| "Unknown".into())
                            ),
                        ),
                        (
                            "SPEED",
                            if item.status.is_active() {
                                fmt_speed(item.speed_bps)
                            } else {
                                "—".into()
                            },
                        ),
                        (
                            "SEGMENTS",
                            if item.num_segments == 0 {
                                "Auto".into()
                            } else {
                                item.num_segments.to_string()
                            },
                        ),
                    ];
                    ui.columns(4, |columns| {
                        for (index, (title, value)) in values.iter().enumerate() {
                            columns[index].label(RichText::new(*title).size(10.5).color(TEXT_DIM));
                            columns[index]
                                .add(
                                    egui::Label::new(
                                        RichText::new(value).size(13.0).color(if index == 0 {
                                            color
                                        } else {
                                            TEXT
                                        }),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(value);
                        }
                    });
                    if let Some(error) = &item.error {
                        ui.add(
                            egui::Label::new(RichText::new(error).size(12.0).color(RED)).truncate(),
                        )
                        .on_hover_text(error);
                    }
                });
        }
        if let Some((id, checked)) = toggled {
            if checked {
                self.library_state.selected_ids.insert(id.clone());
                self.selected_id = Some(id);
            } else {
                self.library_state.selected_ids.remove(&id);
                if self.selected_id.as_ref() == Some(&id) {
                    self.selected_id = None;
                }
            }
        }
        if let Some(id) = clicked {
            if !ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
                self.library_state.selected_ids.clear();
            }
            if let Some(id) = &id {
                self.library_state.selected_ids.insert(id.clone());
            }
            self.selected_id = id;
        }
        if let Some((id, command)) = action {
            if command == "Delete" {
                self.library_state.selected_ids.clear();
                self.library_state.selected_ids.insert(id.clone());
                self.library_state.confirm_remove = true;
            } else if command == "CopyLink" {
                if let Some(item) = items.iter().find(|i| i.id == id) {
                    ui.ctx().copy_text(item.url.clone());
                }
            } else {
                self.handle_action(&id, command, items);
            }
        }
    }
    fn browser_unavailable(&self, ctx: &egui::Context) -> bool {
        ctx.data(|d| d.get_temp::<String>(egui::Id::new("idg-api-error")))
            .is_some()
            || self
                .desktop_error
                .as_ref()
                .is_some_and(|e| e.starts_with("Browser integration:"))
            || ctx
                .data(|d| d.get_temp::<String>(egui::Id::new("idg-extension-error")))
                .is_some()
    }
    pub(super) fn status_bar(&self, ui: &mut Ui, items: &[DownloadItem]) {
        ui.horizontal(|ui| {
            let speed: f64 = items
                .iter()
                .filter(|i| i.status.is_active())
                .map(|i| i.speed_bps)
                .sum();
            ui.label(
                RichText::new(format!("↓ {}", fmt_speed(speed)))
                    .size(12.0)
                    .color(ACCENT),
            );
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "{} active",
                    items.iter().filter(|i| i.status.is_active()).count()
                ))
                .size(12.0)
                .color(TEXT_DIM),
            );
            if ui.available_width() > 480.0 {
                ui.label(
                    RichText::new(format!("·  {} queued", Category::Queued.count(items)))
                        .size(12.0)
                        .color(TEXT_DIM),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("IDG v{APP_VERSION}"))
                        .size(11.0)
                        .color(TEXT_DIM),
                );
                if ui.available_width() > 150.0 {
                    ui.label(
                        RichText::new(if self.browser_unavailable(ui.ctx()) {
                            "Integration unavailable"
                        } else {
                            "Integration ready"
                        })
                        .size(12.0)
                        .color(if self.browser_unavailable(ui.ctx()) {
                            ORANGE
                        } else {
                            SUCCESS
                        }),
                    );
                }
            });
        });
    }
}

pub(super) fn gradient_panel(
    rect: egui::Rect,
    radius: f32,
    top: Color32,
    bottom: Color32,
) -> egui::Shape {
    let color = |y: f32| {
        let t = ((y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let a = top.to_array();
        let b = bottom.to_array();
        Color32::from_rgba_premultiplied(
            (a[0] as f32 * (1.0 - t) + b[0] as f32 * t) as u8,
            (a[1] as f32 * (1.0 - t) + b[1] as f32 * t) as u8,
            (a[2] as f32 * (1.0 - t) + b[2] as f32 * t) as u8,
            (a[3] as f32 * (1.0 - t) + b[3] as f32 * t) as u8,
        )
    };
    let radius = radius.min(rect.height() * 0.5).min(rect.width() * 0.5);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), color(rect.center().y));
    for (center, start) in [
        (rect.left_top() + Vec2::splat(radius), std::f32::consts::PI),
        (
            rect.right_top() + Vec2::new(-radius, radius),
            std::f32::consts::PI * 1.5,
        ),
        (rect.right_bottom() - Vec2::splat(radius), 0.0),
        (
            rect.left_bottom() + Vec2::new(radius, -radius),
            std::f32::consts::FRAC_PI_2,
        ),
    ] {
        for i in 0..=10 {
            let angle = start + i as f32 / 10.0 * std::f32::consts::FRAC_PI_2;
            let p = center + Vec2::new(angle.cos(), angle.sin()) * radius;
            mesh.colored_vertex(p, color(p.y));
        }
    }
    let n = mesh.vertices.len() as u32 - 1;
    for i in 1..=n {
        mesh.indices
            .extend_from_slice(&[0, i, if i == n { 1 } else { i + 1 }]);
    }
    egui::Shape::mesh(mesh)
}

// Soft layered folder artwork, rendered locally at the current display scale.
fn draw_empty_folder(painter: &egui::Painter, rect: egui::Rect) {
    let p = |x: f32, y: f32| {
        egui::pos2(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        )
    };
    let oval = |cx: f32, cy: f32, rx: f32, ry: f32, angle: f32, color: Color32| {
        let points = (0..64)
            .map(|i| {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                let x = rx * a.cos();
                let y = ry * a.sin();
                p(
                    cx + x * angle.cos() - y * angle.sin(),
                    cy + x * angle.sin() + y * angle.cos(),
                )
            })
            .collect();
        painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
    };
    for i in (0..12).rev() {
        let k = i as f32 / 12.0;
        oval(
            0.51,
            0.64,
            0.23 + k * 0.22,
            0.12 + k * 0.09,
            0.0,
            Color32::from_rgba_unmultiplied(8, 18, 33, 3),
        );
    }
    oval(
        0.43,
        0.39,
        0.47,
        0.18,
        -0.68,
        Color32::from_rgba_unmultiplied(38, 64, 99, 63),
    );
    oval(
        0.53,
        0.56,
        0.43,
        0.13,
        -0.38,
        Color32::from_rgba_unmultiplied(40, 68, 105, 48),
    );
    oval(
        0.38,
        0.81,
        0.46,
        0.09,
        -0.52,
        Color32::from_rgba_unmultiplied(27, 52, 88, 45),
    );
    let tab = egui::Rect::from_min_max(p(0.22, 0.17), p(0.49, 0.57));
    painter.add(gradient_panel(
        tab,
        15.0,
        Color32::from_rgb(121, 158, 205),
        Color32::from_rgb(51, 78, 119),
    ));
    painter.rect_stroke(
        tab,
        15.0,
        Stroke::new(1.0_f32, Color32::from_rgb(135, 169, 213)),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            p(0.43, 0.19),
            p(0.53, 0.30),
            p(0.68, 0.30),
            p(0.68, 0.59),
            p(0.43, 0.59),
        ],
        Color32::from_rgb(73, 109, 160),
        Stroke::NONE,
    ));
    let back = egui::Rect::from_min_max(p(0.22, 0.29), p(0.86, 0.88));
    painter.add(gradient_panel(
        back,
        15.0,
        Color32::from_rgb(95, 132, 180),
        Color32::from_rgb(43, 66, 104),
    ));
    painter.rect_stroke(
        back,
        15.0,
        Stroke::new(1.0_f32, Color32::from_rgb(129, 164, 208)),
    );
    let front = egui::Rect::from_min_max(p(0.22, 0.39), p(0.86, 0.88));
    painter.add(gradient_panel(
        front,
        14.0,
        Color32::from_rgb(103, 138, 185),
        Color32::from_rgb(42, 66, 108),
    ));
    painter.rect_stroke(
        front,
        14.0,
        Stroke::new(1.0_f32, Color32::from_rgb(120, 155, 201)),
    );
    let stem = egui::Rect::from_min_max(p(0.49, 0.46), p(0.60, 0.76));
    painter.add(gradient_panel(
        stem,
        5.0,
        Color32::from_rgb(52, 217, 247),
        Color32::from_rgb(16, 157, 242),
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![p(0.39, 0.68), p(0.70, 0.68), p(0.545, 0.91)],
        Color32::from_rgb(21, 170, 249),
        Stroke::new(0.7_f32, Color32::from_rgb(38, 189, 252)),
    ));
}
