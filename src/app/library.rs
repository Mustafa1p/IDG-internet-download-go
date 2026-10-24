//! Compact desktop library: navigation, live summaries, transfer rows, and details.
use super::*;
use egui_extras::{Column, TableBuilder};

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Download,
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
    Clock,
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
            line((0.5, 0.25), (0.5, 0.5));
            line((0.5, 0.5), (0.7, 0.6));
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
        Category::Completed => Icon::Check,
        Category::Paused => Icon::Pause,
        Category::Failed => Icon::Remove,
        Category::Videos => Icon::Video,
        Category::Documents => Icon::File,
        Category::Archives => Icon::Archive,
        Category::Other => Icon::Folder,
    }
}
fn file_icon(item: &DownloadItem) -> (Icon, Color32) {
    match file_category(&item.filename) {
        Category::Videos => (Icon::Video, Color32::from_rgb(161, 119, 244)),
        Category::Archives => (Icon::Archive, Color32::from_rgb(245, 188, 77)),
        Category::Documents => (Icon::File, Color32::from_rgb(91, 175, 240)),
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
        "zip" | "rar" | "7z" | "gz" | "tar" | "bz2" | "xz" => Category::Archives,
        _ => Category::Other,
    }
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
fn card() -> Frame {
    Frame::none()
        .fill(PANEL)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(12.0)
        .inner_margin(Margin::same(14.0))
}

impl IDGApp {
    fn open_add(&mut self) {
        self.dialog = AddDialog::new(&self.default_save_dir);
        self.pending_browser_id = None;
        self.show_add = true;
    }
    pub(super) fn sidebar(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        let short = ui.ctx().screen_rect().height() < 700.0;
        if short {
            ui.spacing_mut().button_padding.y = 2.0;
            ui.spacing_mut().interact_size.y = 22.0;
        }
        let spacious = ui.ctx().screen_rect().width() >= 1200.0 && !short;
        ui.add_space(if spacious {
            22.0
        } else if short {
            4.0
        } else {
            10.0
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 17.0;
            let (rect, _) = ui.allocate_exact_size(
                Vec2::splat(if spacious {
                    52.0
                } else if short {
                    32.0
                } else {
                    40.0
                }),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(rect, 11.0, ACCENT);
            draw_icon(ui.painter(), rect.shrink(10.0), Icon::Download, TEXT);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.label(
                        RichText::new("IDG")
                            .family(egui::FontFamily::Name("idg-heading".into()))
                            .size(if spacious { 28.0 } else { 22.0 })
                            .strong(),
                    );
                    ui.label(
                        RichText::new("GO")
                            .family(egui::FontFamily::Name("idg-heading".into()))
                            .size(if spacious { 28.0 } else { 22.0 })
                            .color(SUCCESS)
                            .strong(),
                    );
                });
                if !short {
                    ui.label(
                        RichText::new("Internet Download GO")
                            .size(if spacious { 12.0 } else { 10.0 })
                            .color(TEXT_DIM),
                    );
                }
            });
        });
        ui.add_space(if spacious {
            26.0
        } else if short {
            8.0
        } else {
            14.0
        });
        if ui
            .add_sized(
                [
                    ui.available_width(),
                    if spacious {
                        48.0
                    } else if short {
                        32.0
                    } else {
                        38.0
                    },
                ],
                egui::Button::new(
                    RichText::new("+  New download")
                        .size(if spacious { 16.0 } else { 12.0 })
                        .strong()
                        .color(TEXT),
                )
                .fill(ACCENT)
                .rounding(9.0),
            )
            .clicked()
        {
            self.open_add();
        }
        ui.add_space(if spacious {
            22.0
        } else if short {
            6.0
        } else {
            12.0
        });
        let height = if spacious {
            44.0
        } else if ui.ctx().screen_rect().height() < 550.0 {
            22.0
        } else {
            30.0
        };
        for (title, categories) in [
            (
                "",
                &[
                    Category::All,
                    Category::Downloading,
                    Category::Completed,
                    Category::Paused,
                    Category::Failed,
                ][..],
            ),
            (
                "Library",
                &[
                    Category::Videos,
                    Category::Documents,
                    Category::Archives,
                    Category::Other,
                ][..],
            ),
        ] {
            ui.add_space(if height < 30.0 { 4.0 } else { 8.0 });
            if !title.is_empty() {
                ui.separator();
                ui.add_space(5.0);
                ui.label(RichText::new(title).size(14.0).color(TEXT_DIM));
            }
            ui.add_space(4.0);
            for &category in categories {
                let selected = !self.show_settings && self.category == category;
                let response = ui.add_sized(
                    [ui.available_width(), height],
                    egui::Button::new("")
                        .fill(if selected {
                            ROW_SEL
                        } else {
                            Color32::TRANSPARENT
                        })
                        .rounding(8.0),
                );
                let rect = response.rect;
                if response.hovered() && !selected {
                    ui.painter().rect_filled(rect, 8.0, PANEL);
                }
                if selected {
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.left_top(), Vec2::new(5.0, rect.height())),
                        2.0,
                        ACCENT,
                    );
                }
                draw_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(
                        egui::pos2(
                            rect.left() + if spacious { 24.0 } else { 18.0 },
                            rect.center().y,
                        ),
                        Vec2::splat(if spacious { 23.0 } else { 20.0 }),
                    ),
                    nav_icon(category),
                    TEXT_DIM,
                );
                let count = category.count(items).to_string();
                let count_width = ui
                    .painter()
                    .layout_no_wrap(count.clone(), FontId::proportional(13.0), TEXT)
                    .size()
                    .x;
                let badge_width = (count_width + 16.0).max(28.0);
                let badge_height = rect.height().min(25.0);
                let badge = egui::Rect::from_min_max(
                    egui::pos2(
                        rect.right() - 8.0 - badge_width,
                        rect.center().y - badge_height * 0.5,
                    ),
                    egui::pos2(rect.right() - 8.0, rect.center().y + badge_height * 0.5),
                );
                let text_left = rect.left() + if spacious { 57.0 } else { 40.0 };
                let label_rect = egui::Rect::from_min_max(
                    egui::pos2(text_left, rect.top()),
                    egui::pos2((badge.left() - 10.0).max(text_left), rect.bottom()),
                );
                let mut label = egui::text::LayoutJob::simple_singleline(
                    category.label().to_owned(),
                    FontId::proportional(if spacious { 15.0 } else { 11.0 }),
                    if selected { TEXT } else { TEXT_DIM },
                );
                label.wrap.max_width = label_rect.width();
                label.wrap.max_rows = 1;
                label.wrap.break_anywhere = true;
                let galley = ui.painter().layout_job(label);
                ui.painter()
                    .with_clip_rect(label_rect.intersect(ui.clip_rect()))
                    .galley(
                        egui::pos2(text_left, rect.center().y - galley.size().y * 0.5),
                        galley,
                        if selected { TEXT } else { TEXT_DIM },
                    );
                ui.painter().rect_filled(
                    badge,
                    12.0,
                    if selected {
                        ACCENT.gamma_multiply(0.28)
                    } else {
                        PANEL
                    },
                );
                ui.painter().text(
                    badge.center(),
                    egui::Align2::CENTER_CENTER,
                    count,
                    FontId::proportional(13.0),
                    TEXT,
                );
                let response = response.on_hover_text(category.label());
                qa_control(ui.ctx(), category.label(), response.rect);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, category.label())
                });
                if response.clicked() {
                    self.category = category;
                    self.show_settings = false;
                    self.settings_draft = None;
                }
            }
        }
        ui.add_space(
            (ui.available_height() - if short { 112.0 } else { 126.0 }).max(if short {
                4.0
            } else {
                10.0
            }),
        );
        ui.separator();
        let settings = ui.add_sized(
            [
                ui.available_width(),
                if spacious {
                    44.0
                } else if short {
                    26.0
                } else {
                    34.0
                },
            ],
            egui::Button::new("")
                .fill(if self.show_settings {
                    ROW_SEL
                } else {
                    Color32::TRANSPARENT
                })
                .rounding(8.0),
        );
        let r = settings.rect;
        draw_icon(
            ui.painter(),
            egui::Rect::from_center_size(
                egui::pos2(r.left() + 24.0, r.center().y),
                Vec2::splat(23.0),
            ),
            Icon::Settings,
            TEXT_DIM,
        );
        ui.painter().text(
            egui::pos2(r.left() + if spacious { 57.0 } else { 49.0 }, r.center().y),
            egui::Align2::LEFT_CENTER,
            "Settings",
            FontId::proportional(if spacious { 15.0 } else { 11.0 }),
            TEXT,
        );
        settings
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Settings"));
        if settings.clicked() {
            self.open_settings();
        }
        ui.separator();
        ui.add_space(10.0);
        let folder = ui.add_sized(
            [ui.available_width(), if short { 44.0 } else { 54.0 }],
            egui::Button::new("").fill(Color32::TRANSPARENT),
        );
        let r = folder.rect;
        draw_icon(
            ui.painter(),
            egui::Rect::from_center_size(
                egui::pos2(r.left() + 24.0, r.center().y),
                Vec2::splat(23.0),
            ),
            Icon::Folder,
            TEXT_DIM,
        );
        draw_icon(
            ui.painter(),
            egui::Rect::from_center_size(
                egui::pos2(r.right() - 12.0, r.center().y),
                Vec2::splat(16.0),
            ),
            Icon::Link,
            TEXT_DIM,
        );
        ui.painter().text(
            egui::pos2(r.left() + if spacious { 57.0 } else { 49.0 }, r.center().y),
            egui::Align2::LEFT_CENTER,
            "Download folder",
            FontId::proportional(if spacious { 14.0 } else { 11.0 }),
            TEXT,
        );
        folder.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Open download folder")
        });
        if folder.on_hover_text("Open download folder").clicked() {
            open_path(std::path::Path::new(&self.default_save_dir));
        }
    }
    pub(super) fn library_header(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        let roomy = ui.available_width() >= 900.0;
        ui.label(
            RichText::new(self.category.label())
                .family(egui::FontFamily::Name("idg-heading".into()))
                .size(if roomy { 36.0 } else { 27.0 })
                .strong(),
        );
        ui.label(
            RichText::new("Your files, organized and ready.")
                .size(if roomy { 17.0 } else { 13.0 })
                .color(TEXT_DIM),
        );
        ui.add_space(if roomy { 18.0 } else { 12.0 });
        let speed: f64 = items
            .iter()
            .filter(|i| i.status.is_active())
            .map(|i| i.speed_bps)
            .sum::<f64>()
            .max(0.0);
        let values = [
            (
                Icon::Speed,
                format!("{:.2} Mbps", speed * 8.0 / 1_000_000.0),
                "Current speed",
            ),
            (
                Icon::Download,
                items
                    .iter()
                    .filter(|i| i.status.is_active())
                    .count()
                    .to_string(),
                "Active downloads",
            ),
            (
                Icon::Check,
                Category::Completed.count(items).to_string(),
                "Completed",
            ),
            (
                Icon::Clock,
                items
                    .iter()
                    .filter(|i| matches!(i.status, Status::Queued))
                    .count()
                    .to_string(),
                "Queued",
            ),
        ];
        if ui.ctx().screen_rect().height() < 700.0 {
            let gap = 8.0;
            let width = (ui.available_width() - gap * 3.0) / 4.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (index, (icon, value, label)) in values.iter().enumerate() {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(width, 56.0), egui::Sense::hover());
                    ui.painter().add(gradient_panel(
                        rect,
                        10.0,
                        PANEL,
                        Color32::from_rgb(20, 33, 51),
                    ));
                    ui.painter()
                        .rect_stroke(rect, 10.0, Stroke::new(1.0_f32, BORDER));
                    let color = match index {
                        0 => ACCENT,
                        3 => Color32::from_rgb(145, 183, 255),
                        _ => SUCCESS,
                    };
                    let icon_rect = egui::Rect::from_center_size(
                        egui::pos2(rect.left() + 23.0, rect.center().y),
                        Vec2::splat(30.0),
                    );
                    ui.painter()
                        .rect_filled(icon_rect, 8.0, color.gamma_multiply(0.10));
                    draw_icon(ui.painter(), icon_rect.shrink(5.0), *icon, color);
                    if index == 2 {
                        ui.painter().circle_stroke(
                            icon_rect.center(),
                            11.0,
                            Stroke::new(1.8_f32, color),
                        );
                    }
                    let text_x = rect.left() + 45.0;
                    ui.painter().text(
                        egui::pos2(text_x, rect.center().y - 8.0),
                        egui::Align2::LEFT_CENTER,
                        value,
                        FontId::proportional(13.0),
                        TEXT,
                    );
                    ui.painter().text(
                        egui::pos2(text_x, rect.center().y + 12.0),
                        egui::Align2::LEFT_CENTER,
                        label,
                        FontId::proportional(10.0),
                        TEXT_DIM,
                    );
                }
            });
            ui.add_space(10.0);
            self.toolbar(ui);
            ui.add_space(8.0);
            return;
        }
        if self
            .library_state
            .sample_at
            .is_none_or(|at| at.elapsed().as_secs_f32() >= 0.5)
        {
            self.library_state.speed_history.push_back(speed);
            while self.library_state.speed_history.len() > 40 {
                self.library_state.speed_history.pop_front();
            }
            self.library_state.sample_at = Some(std::time::Instant::now());
        }
        let gap = if roomy { 16.0 } else { 9.0 };
        let columns = if ui.available_width() < 650.0 {
            2.0
        } else {
            4.0
        };
        let width = (ui.available_width() - gap * (columns - 1.0)) / columns;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (index, (icon, value, label)) in values.into_iter().enumerate() {
                let (r, _) = ui.allocate_exact_size(
                    Vec2::new(width, if roomy { 104.0 } else { 68.0 }),
                    egui::Sense::hover(),
                );
                ui.painter().add(gradient_panel(
                    r,
                    12.0,
                    PANEL,
                    Color32::from_rgb(20, 33, 51),
                ));
                ui.painter()
                    .rect_stroke(r, 12.0, Stroke::new(1.0_f32, BORDER));
                let color = if index == 0 {
                    ACCENT
                } else if index == 3 {
                    Color32::from_rgb(145, 183, 255)
                } else {
                    SUCCESS
                };
                let size = if roomy { 52.0 } else { 34.0 };
                let icon_rect = egui::Rect::from_center_size(
                    egui::pos2(r.left() + if roomy { 39.0 } else { 26.0 }, r.center().y),
                    Vec2::splat(size),
                );
                ui.painter()
                    .rect_filled(icon_rect, 12.0, color.gamma_multiply(0.10));
                let glyph = icon_rect.shrink(if roomy { 8.0 } else { 5.0 });
                draw_icon(ui.painter(), glyph, icon, color);
                if index == 2 {
                    ui.painter().circle_stroke(
                        glyph.center(),
                        glyph.width() * 0.46,
                        Stroke::new(2.6_f32, color),
                    );
                }
                let text_x = r.left() + if roomy { 80.0 } else { 49.0 };
                ui.painter().text(
                    egui::pos2(text_x, r.center().y - 9.0),
                    egui::Align2::LEFT_CENTER,
                    value,
                    FontId::new(
                        if roomy { 20.0 } else { 14.0 },
                        egui::FontFamily::Name("idg-heading".into()),
                    ),
                    TEXT,
                );
                ui.painter().text(
                    egui::pos2(text_x, r.center().y + 16.0),
                    egui::Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(if roomy { 13.0 } else { 10.0 }),
                    TEXT_DIM,
                );
                if index == 0 && roomy {
                    let plot = egui::Rect::from_min_max(
                        egui::pos2(r.right() - 62.0, r.bottom() - 14.0),
                        egui::pos2(r.right() - 12.0, r.bottom() - 5.0),
                    );
                    let max = self
                        .library_state
                        .speed_history
                        .iter()
                        .copied()
                        .fold(1.0, f64::max);
                    let n = self.library_state.speed_history.len().max(2);
                    let points: Vec<_> = self
                        .library_state
                        .speed_history
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            egui::pos2(
                                plot.left() + plot.width() * i as f32 / (n - 1) as f32,
                                plot.bottom() - plot.height() * (v / max) as f32,
                            )
                        })
                        .collect();
                    if points.len() > 1 {
                        ui.painter()
                            .add(egui::Shape::line(points, Stroke::new(2.0_f32, ACCENT)));
                    }
                }
            }
        });
        ui.add_space(if roomy { 18.0 } else { 12.0 });
        self.toolbar(ui);
        ui.add_space(if roomy { 14.0 } else { 8.0 });
    }
    pub(super) fn toolbar(&mut self, ui: &mut Ui) {
        let items = self.downloads.lock().unwrap().clone();
        self.library_state
            .selected_ids
            .retain(|id| items.iter().any(|i| &i.id == id));
        if !ui.ctx().wants_keyboard_input() {
            if ui.input(|i| i.key_pressed(egui::Key::Delete))
                && !self.library_state.selected_ids.is_empty()
            {
                self.library_state.confirm_remove = true;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Space)) {
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
        let selected: Vec<_> = items.iter().filter(|i| ids.contains(&i.id)).collect();
        let roomy = ui.available_width() >= 1000.0;
        let mut command = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = if roomy { 12.0 } else { 6.0 };
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
                        .min_size(Vec2::new(
                            if label == "Open folder" {
                                if roomy { 50.0 } else { 34.0 }
                            } else if roomy {
                                116.0
                            } else {
                                86.0
                            },
                            if roomy { 50.0 } else { 34.0 },
                        ))
                        .rounding(8.0),
                );
                let color = if enabled {
                    TEXT
                } else {
                    TEXT_DIM.gamma_multiply(0.5)
                };
                draw_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(
                        egui::pos2(
                            if label == "Open folder" {
                                response.rect.center().x
                            } else {
                                response.rect.left() + 18.0
                            },
                            response.rect.center().y,
                        ),
                        Vec2::splat(18.0),
                    ),
                    icon,
                    color,
                );
                if label != "Open folder" {
                    ui.painter().text(
                        egui::pos2(response.rect.left() + 33.0, response.rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        label,
                        FontId::proportional(if roomy { 14.0 } else { 11.0 }),
                        color,
                    );
                }
                let response = response.on_hover_text(label);
                qa_control(ui.ctx(), label, response.rect);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label)
                });
                if response.clicked() {
                    match label {
                        "Remove" => self.library_state.confirm_remove = true,
                        "Open folder" => {
                            if selected.is_empty() {
                                open_path(std::path::Path::new(&self.default_save_dir));
                            } else {
                                command = Some("OpenFolder");
                            }
                        }
                        _ => command = Some(label),
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let sort = egui::ComboBox::from_id_salt("library-sort")
                    .selected_text(if self.newest_first {
                        "Newest first"
                    } else {
                        "Oldest first"
                    })
                    .width(if roomy { 140.0 } else { 100.0 })
                    .show_ui(ui, |ui| {
                        let newest =
                            ui.selectable_value(&mut self.newest_first, true, "Newest first");
                        qa_control(ui.ctx(), "Newest first", newest.rect);
                        let oldest =
                            ui.selectable_value(&mut self.newest_first, false, "Oldest first");
                        qa_control(ui.ctx(), "Oldest first", oldest.rect);
                    });
                qa_control(ui.ctx(), "Sort", sort.response.rect);
                for (grid, icon, label) in [
                    (true, Icon::Grid, "Grid view"),
                    (false, Icon::List, "List view"),
                ] {
                    let response = ui.add_sized(
                        [
                            if roomy { 46.0 } else { 30.0 },
                            if roomy { 42.0 } else { 32.0 },
                        ],
                        egui::Button::new("")
                            .fill(if self.library_state.grid == grid {
                                ACCENT2
                            } else {
                                PANEL
                            })
                            .rounding(7.0),
                    );
                    draw_icon(ui.painter(), response.rect.shrink(8.0), icon, TEXT_DIM);
                    qa_control(ui.ctx(), label, response.rect);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label)
                    });
                    if response.on_hover_text(label).clicked() {
                        self.library_state.grid = grid;
                    }
                }
            });
        });
        if let Some(command) = command {
            for id in &ids {
                self.handle_action(id, command, &items);
            }
        }
        if self.library_state.confirm_remove {
            egui::Window::new("Remove selected downloads?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    ui.label(format!(
                        "Remove {} selected download(s) from the list?",
                        ids.len()
                    ));
                    ui.label("Active transfers will stop. Downloaded files will remain on disk.");
                    ui.horizontal(|ui| {
                        let cancel = ui.button("Cancel");
                        qa_control(ui.ctx(), "Cancel remove", cancel.rect);
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
    pub(super) fn download_table(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        let query = self.search.trim().to_lowercase();
        let mut filtered: Vec<_> = items
            .iter()
            .filter(|item| {
                self.category.matches(item) && item.filename.to_lowercase().contains(&query)
            })
            .collect();
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
        let selected = self.selected_id.clone();
        let details = items
            .iter()
            .find(|item| Some(&item.id) == selected.as_ref() && item.status != Status::Completed);
        let details_height = if details.is_some() {
            if ui.ctx().screen_rect().height() < 650.0 {
                140.0
            } else {
                166.0
            }
        } else {
            0.0
        };
        let table_height = (ui.available_height() - details_height - 8.0).max(90.0);
        let mut clicked = None;
        let mut action = None;
        let mut toggled = None;
        let table_background = ui.painter().add(egui::Shape::Noop);
        let table = card()
            .fill(Color32::TRANSPARENT)
            .inner_margin(Margin::symmetric(6.0, 0.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(table_height);
                let compact = ui.available_width() < 900.0;
                if self.library_state.grid && !filtered.is_empty() {
                    egui::ScrollArea::vertical()
                        .max_height(table_height - 8.0)
                        .show(ui, |ui| {
                            let gap = 12.0;
                            let columns = (((ui.available_width() + gap) / (255.0 + gap)).floor()
                                as usize)
                                .max(1);
                            let width = (ui.available_width() - 12.0 * (columns - 1) as f32)
                                / columns as f32;
                            for chunk in filtered.chunks(columns) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = gap;
                                    for item in chunk {
                                        card()
                                            .fill(
                                                if self
                                                    .library_state
                                                    .selected_ids
                                                    .contains(&item.id)
                                                {
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
                                                            if selection_box(ui, &mut checked)
                                                                .changed()
                                                            {
                                                                toggled = Some((
                                                                    item.id.clone(),
                                                                    checked,
                                                                ));
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
                                                                        RichText::new(
                                                                            &item.filename,
                                                                        )
                                                                        .strong(),
                                                                    )
                                                                    .truncate()
                                                                    .sense(egui::Sense::click()),
                                                                )
                                                                .on_hover_text(&item.filename)
                                                                .clicked()
                                                            {
                                                                clicked =
                                                                    Some(Some(item.id.clone()));
                                                            }
                                                        });
                                                        let (label, color) =
                                                            status_info(&item.status);
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
                                                                if item.status == Status::Completed
                                                                {
                                                                    1.0
                                                                } else {
                                                                    item.progress()
                                                                },
                                                            )
                                                            .desired_height(5.0)
                                                            .fill(ACCENT),
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
                                                                && icon_button(
                                                                    ui,
                                                                    Icon::Play,
                                                                    "Resume",
                                                                )
                                                                .clicked()
                                                            {
                                                                action = Some((
                                                                    item.id.clone(),
                                                                    "Resume",
                                                                ));
                                                            }
                                                            if item.status.is_active()
                                                                && item.supports_pause()
                                                                && icon_button(
                                                                    ui,
                                                                    Icon::Pause,
                                                                    "Pause",
                                                                )
                                                                .clicked()
                                                            {
                                                                action = Some((
                                                                    item.id.clone(),
                                                                    "Pause",
                                                                ));
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
                let data_width =
                    (ui.available_width() - 64.0 - 7.0 * ui.spacing().item_spacing.x).max(416.0);
                TableBuilder::new(ui)
                    .striped(false)
                    .sense(egui::Sense::click())
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::exact(30.0))
                    .column(Column::remainder().at_least(130.0))
                    .column(Column::exact(data_width * 0.14))
                    .column(Column::exact(data_width * 0.16))
                    .column(Column::exact(data_width * 0.13))
                    .column(Column::exact(data_width * 0.14))
                    .column(Column::exact(data_width * 0.12))
                    .column(Column::exact(34.0))
                    .min_scrolled_height(table_height - 30.0)
                    .max_scroll_height(table_height - 30.0)
                    .header(if compact { 36.0 } else { 42.0 }, |mut header| {
                        header.col(|ui| {
                            let mut all = !filtered.is_empty()
                                && filtered
                                    .iter()
                                    .all(|i| self.library_state.selected_ids.contains(&i.id));
                            let selection = ui
                                .checkbox(&mut all, "")
                                .on_hover_text("Select all visible downloads");
                            qa_control(ui.ctx(), "Select all", selection.rect);
                            if selection.changed() {
                                for i in &filtered {
                                    if all {
                                        self.library_state.selected_ids.insert(i.id.clone());
                                    } else {
                                        self.library_state.selected_ids.remove(&i.id);
                                    }
                                }
                            }
                        });
                        for label in ["Name", "Size", "Progress", "Speed", "Status", "Added", ""] {
                            header.col(|ui| {
                                ui.label(
                                    RichText::new(label)
                                        .size(if compact { 12.0 } else { 13.0 })
                                        .color(TEXT_DIM),
                                );
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(
                            if compact { 54.0 } else { 64.0 },
                            filtered.len(),
                            |mut row| {
                                let item = filtered[row.index()];
                                row.set_selected(
                                    self.library_state.selected_ids.contains(&item.id),
                                );
                                row.col(|ui| {
                                    let mut checked =
                                        self.library_state.selected_ids.contains(&item.id);
                                    let selection = selection_box(ui, &mut checked);
                                    selection.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Checkbox,
                                            true,
                                            format!("Select {}", item.filename),
                                        )
                                    });
                                    if selection.changed() {
                                        toggled = Some((item.id.clone(), checked));
                                    }
                                });
                                row.col(|ui| {
                                    let (icon, color) = file_icon(item);
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::new(23.0, 29.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter()
                                        .rect_filled(rect, 4.0, color.gamma_multiply(0.16));
                                    draw_icon(ui.painter(), rect.shrink(4.0), icon, color);
                                    ui.vertical(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&item.filename)
                                                    .size(if compact { 11.5 } else { 14.0 })
                                                    .color(TEXT),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&item.filename);
                                        ui.label(
                                            RichText::new(host(&item.url))
                                                .size(if compact { 10.0 } else { 11.0 })
                                                .color(TEXT_DIM),
                                        );
                                    });
                                });
                                row.col(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(fmt_size(item.downloaded_bytes))
                                                .size(if compact { 11.0 } else { 13.0 }),
                                        );
                                        ui.label(
                                            RichText::new(
                                                item.total_bytes
                                                    .or_else(|| {
                                                        (item.status == Status::Completed)
                                                            .then_some(item.downloaded_bytes)
                                                    })
                                                    .map(|total| format!("/ {}", fmt_size(total)))
                                                    .unwrap_or_else(|| "/ Unknown".into()),
                                            )
                                            .size(if compact { 10.0 } else { 11.0 })
                                            .color(TEXT_DIM),
                                        );
                                    });
                                });
                                row.col(|ui| {
                                    ui.vertical(|ui| {
                                        let progress = if item.status == Status::Completed {
                                            1.0
                                        } else {
                                            item.progress()
                                        };
                                        ui.label(
                                            RichText::new(
                                                if item.total_bytes.is_some()
                                                    || item.status == Status::Completed
                                                {
                                                    format!("{:.0}%", progress * 100.0)
                                                } else {
                                                    "—".into()
                                                },
                                            )
                                            .size(if compact { 11.0 } else { 13.0 }),
                                        );
                                        ui.add(
                                            egui::ProgressBar::new(progress)
                                                .desired_width(if compact { 62.0 } else { 82.0 })
                                                .desired_height(4.0)
                                                .fill(ACCENT),
                                        );
                                    });
                                });
                                row.col(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(if item.status.is_active() {
                                                fmt_speed(item.speed_bps)
                                            } else {
                                                "—".into()
                                            })
                                            .size(if compact { 12.0 } else { 13.0 }),
                                        );
                                        if let Some(seconds) = item.eta_secs() {
                                            ui.label(
                                                RichText::new(format!("{} left", fmt_eta(seconds)))
                                                    .size(9.5)
                                                    .color(TEXT_DIM),
                                            );
                                        }
                                    });
                                });
                                row.col(|ui| {
                                    ui.vertical(|ui| {
                                        let (label, color) = status_info(&item.status);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(label)
                                                    .size(if compact { 12.0 } else { 13.0 })
                                                    .color(color),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(item.error.as_deref().unwrap_or(label));
                                    });
                                });
                                row.col(|ui| {
                                    ui.label(
                                        RichText::new(
                                            item.added_at
                                                .with_timezone(&chrono::Local)
                                                .format("%d %b %Y")
                                                .to_string(),
                                        )
                                        .size(if compact { 10.0 } else { 12.0 })
                                        .color(TEXT_DIM),
                                    );
                                });
                                row.col(|ui| {
                                    if icon_button(ui, Icon::Folder, "Open folder").clicked() {
                                        action = Some((item.id.clone(), "OpenFolder"));
                                    }
                                });
                                let response = row.response();
                                if response.clicked() && clicked.is_none() && toggled.is_none() {
                                    clicked = Some(Some(item.id.clone()));
                                }
                                if response.double_clicked() && item.status == Status::Completed {
                                    action = Some((item.id.clone(), "OpenFile"));
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
                                        if ui
                                            .add_enabled(
                                                match command {
                                                    "OpenFile" => item.status == Status::Completed,
                                                    "Resume" => item.status == Status::Paused,
                                                    "Pause" => {
                                                        item.status.is_active()
                                                            && item.supports_pause()
                                                    }
                                                    _ => true,
                                                },
                                                egui::Button::new(label),
                                            )
                                            .clicked()
                                        {
                                            action = Some((item.id.clone(), command));
                                            ui.close_menu();
                                        }
                                    }
                                });
                            },
                        );
                    });
            });
        ui.painter().set(
            table_background,
            gradient_panel(
                table.response.rect,
                12.0,
                PANEL,
                Color32::from_rgb(19, 32, 51),
            ),
        );
        if !self.library_state.grid || filtered.is_empty() {
            let r = table.response.rect;
            let y = r.top() + if r.width() < 900.0 { 36.0 } else { 42.0 };
            ui.painter().line_segment(
                [egui::pos2(r.left(), y), egui::pos2(r.right(), y)],
                Stroke::new(1.0_f32, BORDER),
            );
        }
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
        if let Some(item) = details {
            ui.add_space(8.0);
            self.download_details(ui, item);
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
    fn download_details(&self, ui: &mut Ui, item: &DownloadItem) {
        if ui.ctx().screen_rect().height() < 650.0 {
            card().inner_margin(Margin::same(10.0)).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.add(
                    egui::Label::new(RichText::new(&item.filename).size(13.0).strong()).truncate(),
                )
                .on_hover_text(&item.filename);
                ui.add(
                    egui::Label::new(
                        RichText::new(item.save_path.to_string_lossy())
                            .size(12.0)
                            .color(TEXT_DIM),
                    )
                    .truncate(),
                )
                .on_hover_text(item.save_path.to_string_lossy());
                ui.horizontal(|ui| {
                    if icon_button(ui, Icon::Folder, "Open folder").clicked()
                        && let Some(path) = item.save_path.parent()
                    {
                        open_path(path);
                    }
                    if ui
                        .add_enabled(
                            item.status == Status::Completed,
                            egui::Button::new("Open file"),
                        )
                        .clicked()
                    {
                        open_path(&item.save_path);
                    }
                });
                let (label, color) = status_info(&item.status);
                ui.horizontal(|ui| {
                    ui.colored_label(color, label);
                    ui.label(format!("{} downloaded", fmt_size(item.downloaded_bytes)));
                });
            });
            return;
        }
        card().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Download details").size(10.0).color(TEXT_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, Icon::Link, "Copy download link").clicked() {
                        ui.ctx().copy_text(item.url.clone());
                    }
                    if icon_button(ui, Icon::Folder, "Open file location").clicked()
                        && let Some(path) = item.save_path.parent()
                    {
                        open_path(path);
                    }
                });
            });
            ui.add(egui::Label::new(RichText::new(&item.filename).strong().size(12.0)).truncate());
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(item.save_path.to_string_lossy())
                            .size(10.0)
                            .color(TEXT_DIM),
                    )
                    .truncate(),
                )
                .on_hover_text(item.save_path.to_string_lossy());
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} downloaded", fmt_size(item.downloaded_bytes)))
                        .size(10.0)
                        .color(TEXT_DIM),
                );
                ui.separator();
                let parts = if !item.supports_pause() {
                    item.media_options.format.label().to_owned()
                } else if item.num_segments == 0 {
                    "Automatic connections".into()
                } else {
                    format!("{} connections", item.num_segments)
                };
                ui.label(RichText::new(parts).size(10.0).color(TEXT_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (label, color) = status_info(&item.status);
                    ui.label(RichText::new(label).color(color).size(10.0));
                });
            });
            ui.add(
                egui::ProgressBar::new(if item.status == Status::Completed {
                    1.0
                } else {
                    item.progress()
                })
                .desired_height(4.0)
                .fill(ACCENT),
            );
            if let Some(error) = &item.error {
                ui.colored_label(RED, error);
            }
        });
    }
    pub(super) fn status_bar(&self, ui: &mut Ui, items: &[DownloadItem]) {
        ui.horizontal(|ui| {
            let api_error = ui
                .ctx()
                .data(|data| data.get_temp::<String>(egui::Id::new("idg-api-error")));
            let unavailable = api_error.is_some()
                || self
                    .desktop_error
                    .as_ref()
                    .is_some_and(|error| error.starts_with("Browser integration:"))
                || ui
                    .ctx()
                    .data(|data| data.get_temp::<String>(egui::Id::new("idg-extension-error")))
                    .is_some();
            let color = if unavailable { ORANGE } else { SUCCESS };
            let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 5.0, color);
            ui.label(
                RichText::new(if unavailable {
                    "Browser integration unavailable"
                } else {
                    "Browser integration ready"
                })
                .size(13.0)
                .color(color),
            );
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!(
                    "{} active downloads",
                    items.iter().filter(|i| i.status.is_active()).count()
                ))
                .size(13.0)
                .color(TEXT_DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("IDG v{APP_VERSION}"))
                        .size(13.0)
                        .color(TEXT_DIM),
                );
            });
        });
    }
}

// A rounded mesh with a continuous vertical material gradient.
fn gradient_panel(rect: egui::Rect, radius: f32, top: Color32, bottom: Color32) -> egui::Shape {
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
