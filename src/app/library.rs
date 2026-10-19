//! Compact desktop library: navigation, live summaries, transfer rows, and details.
use super::*;
use egui_extras::{Column, TableBuilder};

#[derive(Clone, Copy)]
enum Icon {
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
}

fn draw_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
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
            painter.circle_stroke(p(0.5, 0.5), rect.width() * 0.37, stroke);
            line((0.5, 0.5), (0.73, 0.28));
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
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
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
fn card() -> Frame {
    Frame::none()
        .fill(PANEL)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(7.0)
        .inner_margin(Margin::same(10.0))
}

impl IDGApp {
    fn open_add(&mut self) {
        self.dialog = AddDialog::new(&self.default_save_dir);
        self.pending_browser_id = None;
        self.show_add = true;
    }
    pub(super) fn sidebar(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 6.0, ACCENT2);
            draw_icon(ui.painter(), rect.shrink(6.0), Icon::Download, ACCENT);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.label(RichText::new("IDG").size(22.0).strong());
                    ui.label(RichText::new("GO").size(22.0).color(ACCENT).strong());
                });
                ui.label(
                    RichText::new("Internet Download GO")
                        .size(10.0)
                        .color(TEXT_DIM),
                );
            });
        });
        ui.add_space(14.0);
        if ui
            .add_sized(
                [ui.available_width(), 34.0],
                egui::Button::new(RichText::new("+  Add download").strong().color(TEXT))
                    .fill(ACCENT2)
                    .rounding(6.0),
            )
            .clicked()
        {
            self.open_add();
        }
        let height = if ui.ctx().screen_rect().height() < 550.0 {
            24.0
        } else {
            30.0
        };
        for (title, categories) in [
            (
                "LIBRARY",
                &[
                    Category::All,
                    Category::Downloading,
                    Category::Completed,
                    Category::Paused,
                    Category::Failed,
                ][..],
            ),
            (
                "CATEGORIES",
                &[
                    Category::Videos,
                    Category::Documents,
                    Category::Archives,
                    Category::Other,
                ][..],
            ),
        ] {
            ui.add_space(if height < 30.0 { 4.0 } else { 8.0 });
            ui.label(RichText::new(title).size(10.0).color(TEXT_DIM));
            ui.add_space(4.0);
            for &category in categories {
                let selected = self.category == category;
                let response = ui.add_sized(
                    [ui.available_width(), height],
                    egui::Button::new("")
                        .fill(if selected {
                            ROW_SEL
                        } else {
                            Color32::TRANSPARENT
                        })
                        .rounding(5.0),
                );
                let rect = response.rect;
                if selected {
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.left_top(), Vec2::new(2.0, rect.height())),
                        2.0,
                        ACCENT,
                    );
                }
                draw_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(
                        egui::pos2(rect.left() + 16.0, rect.center().y),
                        Vec2::splat(16.0),
                    ),
                    nav_icon(category),
                    if selected { ACCENT } else { TEXT_DIM },
                );
                ui.painter().text(
                    egui::pos2(rect.left() + 34.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    category.label(),
                    FontId::proportional(11.5),
                    if selected { TEXT } else { TEXT_DIM },
                );
                ui.painter().text(
                    egui::pos2(rect.right() - 10.0, rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    category.count(items).to_string(),
                    FontId::proportional(10.5),
                    if selected { ACCENT } else { TEXT_DIM },
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, category.label())
                });
                if response.clicked() {
                    self.category = category;
                }
            }
        }
        ui.add_space((ui.available_height() - 100.0).max(4.0));
        ui.separator();
        let settings = ui.add_sized(
            [ui.available_width(), 28.0],
            egui::Button::new("Settings").fill(Color32::TRANSPARENT),
        );
        draw_icon(
            ui.painter(),
            egui::Rect::from_center_size(
                egui::pos2(settings.rect.left() + 15.0, settings.rect.center().y),
                Vec2::splat(17.0),
            ),
            Icon::Settings,
            TEXT_DIM,
        );
        if settings.clicked() {
            self.show_settings = true;
            self.default_segments = self.connection_settings.lock().unwrap().requested_count();
        }
        ui.label(RichText::new("Download folder").size(10.0).color(TEXT_DIM));
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(&self.default_save_dir)
                        .size(10.0)
                        .color(TEXT_DIM),
                )
                .truncate(),
            )
            .on_hover_text(&self.default_save_dir);
            if icon_button(ui, Icon::Folder, "Open download folder").clicked() {
                open_path(std::path::Path::new(&self.default_save_dir));
            }
        });
    }
    pub(super) fn library_header(&mut self, ui: &mut Ui, items: &[DownloadItem]) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(self.category.label()).size(23.0).strong());
                ui.label(
                    RichText::new("Your files, organized and ready.")
                        .size(11.5)
                        .color(TEXT_DIM),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("…", |ui| {
                    if ui.button("Pause all downloads").clicked() {
                        self.pause_all();
                        ui.close_menu();
                    }
                    if self.tray.is_some() && ui.button("Hide to system tray").clicked() {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        ui.close_menu();
                    }
                    if ui.button("Settings").clicked() {
                        self.show_settings = true;
                        ui.close_menu();
                    }
                });
                Frame::none()
                    .fill(PANEL)
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .rounding(6.0)
                    .inner_margin(Margin::symmetric(8.0, 5.0))
                    .show(ui, |ui| {
                        ui.add_sized(
                            [
                                if ui.available_width() > 340.0 {
                                    174.0
                                } else {
                                    138.0
                                },
                                18.0,
                            ],
                            egui::TextEdit::singleline(&mut self.search)
                                .frame(false)
                                .hint_text("Search downloads…"),
                        );
                    });
            });
        });
        ui.add_space(12.0);
        let speed: f64 = items
            .iter()
            .filter(|i| i.status.is_active())
            .map(|i| i.speed_bps)
            .sum();
        let values = [
            (Icon::Speed, fmt_speed(speed), "Current speed"),
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
                Icon::List,
                items
                    .iter()
                    .filter(|i| matches!(i.status, Status::Queued))
                    .count()
                    .to_string(),
                "Queued",
            ),
        ];
        card().show(ui, |ui| {
            ui.columns(4, |columns| {
                for (column, (icon, value, label)) in columns.iter_mut().zip(values) {
                    column.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(25.0), egui::Sense::hover());
                        draw_icon(ui.painter(), rect, icon, ACCENT);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(value).size(17.0).strong());
                            ui.label(RichText::new(label).size(10.0).color(TEXT_DIM));
                        });
                    });
                }
            });
        });
        ui.add_space(10.0);
        self.toolbar(ui);
        ui.add_space(8.0);
    }
    pub(super) fn toolbar(&mut self, ui: &mut Ui) {
        let selected = self
            .downloads
            .lock()
            .unwrap()
            .iter()
            .find(|i| Some(&i.id) == self.selected_id.as_ref())
            .cloned();
        ui.horizontal(|ui| {
            ui.spacing_mut().button_padding = Vec2::new(10.0, 6.0);
            if ui
                .add_enabled(
                    selected
                        .as_ref()
                        .is_some_and(|i| i.status == Status::Paused),
                    egui::Button::new("Resume"),
                )
                .clicked()
            {
                self.resume_selected();
            }
            if ui
                .add_enabled(
                    selected
                        .as_ref()
                        .is_some_and(|i| i.status.is_active() && i.supports_pause()),
                    egui::Button::new("Pause"),
                )
                .clicked()
            {
                self.pause_selected();
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Remove"))
                .clicked()
            {
                self.delete_selected();
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Open folder"))
                .clicked()
                && let Some(item) = &selected
                && let Some(path) = item.save_path.parent()
            {
                open_path(path);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("library-sort")
                    .selected_text(if self.newest_first {
                        "Newest first"
                    } else {
                        "Oldest first"
                    })
                    .width(106.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.newest_first, true, "Newest first");
                        ui.selectable_value(&mut self.newest_first, false, "Oldest first");
                    });
            });
        });
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
        let selected = self.selected_id.clone();
        let details = items
            .iter()
            .find(|item| Some(&item.id) == selected.as_ref());
        let details_height = if details.is_some() { 116.0 } else { 0.0 };
        let table_height = (ui.available_height() - details_height - 8.0).max(90.0);
        let mut clicked = None;
        let mut action = None;
        let table = card()
            .inner_margin(Margin::symmetric(6.0, 0.0))
            .show(ui, |ui| {
                ui.set_min_height(table_height);
                let compact = ui.available_width() < 700.0;
                TableBuilder::new(ui)
                    .striped(false)
                    .sense(egui::Sense::click())
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::remainder().at_least(130.0))
                    .column(Column::exact(if compact { 74.0 } else { 110.0 }))
                    .column(Column::exact(if compact { 72.0 } else { 94.0 }))
                    .column(Column::exact(if compact { 64.0 } else { 76.0 }))
                    .column(Column::exact(if compact { 74.0 } else { 88.0 }))
                    .column(Column::exact(if compact { 28.0 } else { 32.0 }))
                    .min_scrolled_height(table_height - 30.0)
                    .max_scroll_height(table_height - 30.0)
                    .header(29.0, |mut header| {
                        for label in ["Name", "Size", "Progress", "Speed", "Status", ""] {
                            header.col(|ui| {
                                ui.label(RichText::new(label).size(10.5).color(TEXT_DIM));
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(49.0, filtered.len(), |mut row| {
                            let item = filtered[row.index()];
                            row.set_selected(Some(&item.id) == selected.as_ref());
                            row.col(|ui| {
                                let mut checked = Some(&item.id) == selected.as_ref();
                                let selection = ui.checkbox(&mut checked, "");
                                selection.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Checkbox,
                                        true,
                                        format!("Select {}", item.filename),
                                    )
                                });
                                if selection.changed() {
                                    clicked =
                                        Some(if checked { Some(item.id.clone()) } else { None });
                                }
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
                                            RichText::new(&item.filename).size(11.5).color(TEXT),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(&item.filename);
                                    ui.label(
                                        RichText::new(host(&item.url)).size(10.0).color(TEXT_DIM),
                                    );
                                });
                            });
                            row.col(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(fmt_size(item.downloaded_bytes)).size(11.0),
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
                                        .size(10.0)
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
                                        .size(11.0),
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
                                        .size(10.5),
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
                                let (label, color) = status_info(&item.status);
                                ui.add(
                                    egui::Label::new(RichText::new(label).size(10.5).color(color))
                                        .truncate(),
                                )
                                .on_hover_text(item.error.as_deref().unwrap_or(label));
                            });
                            row.col(|ui| {
                                if item.status == Status::Completed {
                                    if icon_button(ui, Icon::Folder, "Open download folder")
                                        .clicked()
                                    {
                                        action = Some((item.id.clone(), "OpenFolder"));
                                    }
                                } else if item.status == Status::Paused {
                                    if icon_button(ui, Icon::Play, "Resume download").clicked() {
                                        action = Some((item.id.clone(), "Resume"));
                                    }
                                } else if item.status.is_active()
                                    && item.supports_pause()
                                    && icon_button(ui, Icon::Pause, "Pause download").clicked()
                                {
                                    action = Some((item.id.clone(), "Pause"));
                                }
                            });
                            let response = row.response();
                            if response.clicked() && clicked.is_none() {
                                clicked = Some(Some(item.id.clone()));
                            }
                            if response.double_clicked() && item.status == Status::Completed {
                                action = Some((item.id.clone(), "OpenFile"));
                            }
                            response.context_menu(|ui| {
                                for (label, command) in [
                                    ("Open file", "OpenFile"),
                                    ("Open folder", "OpenFolder"),
                                    ("Remove", "Delete"),
                                ] {
                                    if ui
                                        .add_enabled(
                                            command != "OpenFile"
                                                || item.status == Status::Completed,
                                            egui::Button::new(label),
                                        )
                                        .clicked()
                                    {
                                        action = Some((item.id.clone(), command));
                                        ui.close_menu();
                                    }
                                }
                            });
                        });
                    });
            });
        if filtered.is_empty() {
            let message = if query.is_empty() {
                "Add a link or download from your browser to get started."
            } else {
                "No downloads match your search."
            };
            let rect = table.response.rect;
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                message,
                FontId::proportional(11.5),
                TEXT_DIM,
            );
        }
        if let Some(item) = details {
            ui.add_space(8.0);
            self.download_details(ui, item);
        }
        if let Some(id) = clicked {
            self.selected_id = id;
        }
        if let Some((id, command)) = action {
            self.handle_action(&id, command, items);
        }
    }
    fn download_details(&self, ui: &mut Ui, item: &DownloadItem) {
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
            ui.label(
                RichText::new(format!(
                    "{} active downloads",
                    items.iter().filter(|i| i.status.is_active()).count()
                ))
                .size(10.0)
                .color(TEXT_DIM),
            );
            ui.separator();
            let api_error = ui
                .ctx()
                .data(|data| data.get_temp::<String>(egui::Id::new("idg-api-error")));
            ui.label(
                RichText::new(if api_error.is_some() {
                    "Browser integration unavailable"
                } else {
                    "Browser integration ready"
                })
                .size(10.0)
                .color(if api_error.is_some() { ORANGE } else { ACCENT }),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("IDG v{APP_VERSION}"))
                        .size(10.0)
                        .color(TEXT_DIM),
                );
            });
        });
    }
}
