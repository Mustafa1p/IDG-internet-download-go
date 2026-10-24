//! Fluent window chrome; commands use the existing native viewport lifecycle.
use super::*;

pub(super) fn surface(painter: &egui::Painter, rect: egui::Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    for (position, color) in [
        (rect.left_top(), top),
        (rect.right_top(), top),
        (rect.left_bottom(), bottom),
        (rect.right_bottom(), bottom),
    ] {
        mesh.colored_vertex(position, color);
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

fn chrome_button(ui: &mut Ui, label: &str, kind: u8, height: f32) -> egui::Response {
    let response = ui.add_sized(
        [if height > 50.0 { 50.0 } else { 40.0 }, height],
        egui::Button::new("")
            .fill(Color32::TRANSPARENT)
            .frame(false),
    );
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(response.rect, 5.0, if kind == 2 { RED } else { ROW_EVEN });
    }
    let center = response.rect.center();
    let stroke = Stroke::new(1.2_f32, TEXT_DIM);
    let rect = egui::Rect::from_center_size(center, Vec2::splat(12.0));
    match kind {
        0 => {
            ui.painter()
                .line_segment([rect.left_center(), rect.right_center()], stroke);
        }
        1 => {
            ui.painter().rect_stroke(rect, 1.0, stroke);
        }
        2 => {
            ui.painter()
                .line_segment([rect.left_top(), rect.right_bottom()], stroke);
            ui.painter()
                .line_segment([rect.right_top(), rect.left_bottom()], stroke);
        }
        _ => {
            ui.painter()
                .rect_stroke(rect.translate(Vec2::new(2.0, -2.0)), 1.0, stroke);
            ui.painter().rect_filled(rect, 1.0, PANEL);
            ui.painter().rect_stroke(rect, 1.0, stroke);
        }
    }
    qa_control(ui.ctx(), label, response.rect);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    response.on_hover_text(label)
}

impl IDGApp {
    pub(super) fn shell_header(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let rect = ui.max_rect();
        surface(
            ui.painter(),
            rect,
            Color32::from_rgb(23, 37, 55),
            Color32::from_rgb(16, 29, 47),
        );
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        let drag = ui.interact(
            rect,
            egui::Id::new("window-drag-region"),
            egui::Sense::click_and_drag(),
        );
        if drag.double_clicked() && crate::settings_preview() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        } else if drag.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let wide = ctx.screen_rect().width() >= 1100.0;
        let height = if wide { 62.0 } else { 48.0 };
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = if wide { 12.0 } else { 6.0 };
            let controls = if wide { 300.0 } else { 280.0 };
            let search_width = (ui.available_width() * 0.49).clamp(170.0, 500.0);
            ui.add_space((ui.available_width() - controls - search_width).max(4.0));
            Frame::none()
                .fill(Color32::from_rgb(28, 46, 70))
                .stroke(Stroke::new(1.0_f32, BORDER))
                .rounding(24.0)
                .inner_margin(Margin::symmetric(14.0, 9.0))
                .show(ui, |ui| {
                    ui.set_width((search_width - 28.0).max(140.0));
                    ui.horizontal(|ui| {
                        let (icon, _) =
                            ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                        let center = icon.center() - Vec2::splat(2.0);
                        ui.painter()
                            .circle_stroke(center, 6.0, Stroke::new(1.4_f32, TEXT_DIM));
                        ui.painter().line_segment(
                            [center + Vec2::splat(4.5), icon.right_bottom()],
                            Stroke::new(1.4_f32, TEXT_DIM),
                        );
                        let search = ui.add_sized(
                            [ui.available_width(), 22.0],
                            egui::TextEdit::singleline(&mut self.search)
                                .id(egui::Id::new("download-search"))
                                .frame(false)
                                .hint_text("Search downloads...")
                                .font(FontId::proportional(if wide { 15.0 } else { 13.0 })),
                        );
                        search.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::TextEdit,
                                true,
                                "Search downloads",
                            )
                        });
                        if ctx.input_mut(|input| {
                            input.consume_key(egui::Modifiers::CTRL, egui::Key::F)
                        }) {
                            search.request_focus();
                        }
                    });
                });
            let filter = ui.menu_button("     ", |ui| {
                ui.label(RichText::new("Show downloads").strong());
                for category in [
                    Category::All,
                    Category::Downloading,
                    Category::Completed,
                    Category::Paused,
                    Category::Failed,
                    Category::Videos,
                    Category::Documents,
                    Category::Archives,
                    Category::Other,
                ] {
                    if ui
                        .selectable_label(
                            self.category == category && !self.show_settings,
                            category.label(),
                        )
                        .clicked()
                    {
                        self.category = category;
                        self.show_settings = false;
                        self.settings_draft = None;
                        ui.close_menu();
                    }
                }
            });
            let filter_rect = filter.response.rect;
            for (offset, knob) in [(-5.0, -3.0), (0.0, 4.0), (5.0, -1.0)] {
                let y = filter_rect.center().y + offset;
                ui.painter().line_segment(
                    [
                        egui::pos2(filter_rect.center().x - 8.0, y),
                        egui::pos2(filter_rect.center().x + 8.0, y),
                    ],
                    Stroke::new(1.2_f32, TEXT_DIM),
                );
                ui.painter().circle_filled(
                    egui::pos2(filter_rect.center().x + knob, y),
                    2.0,
                    TEXT_DIM,
                );
            }
            filter.response.on_hover_text("Filter downloads");
            ui.menu_button(RichText::new("···").size(22.0), |ui| {
                if ui.button("New download     Ctrl+N").clicked() {
                    self.show_new_download();
                    ui.close_menu();
                }
                if ui.button("Pause all downloads").clicked() {
                    self.pause_all();
                    ui.close_menu();
                }
                if ui.button("Open download folder").clicked() {
                    open_path(std::path::Path::new(&self.default_save_dir));
                    ui.close_menu();
                }
                if ui.button("Check for updates").clicked() {
                    self.check_updates(ctx);
                    ui.close_menu();
                }
                if ui.button("Settings     Ctrl+,").clicked() {
                    self.open_settings();
                    ui.close_menu();
                }
                if self.tray.is_some() && ui.button("Hide to system tray").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Quit IDG").clicked() {
                    self.exiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    ui.close_menu();
                }
            })
            .response
            .on_hover_text("More options");
            ui.separator();
            if chrome_button(ui, "Minimize", 0, height).clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            if ui
                .add_enabled_ui(crate::settings_preview(), |ui| {
                    chrome_button(
                        ui,
                        if maximized { "Restore" } else { "Maximize" },
                        if maximized { 3 } else { 1 },
                        height,
                    )
                })
                .inner
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            if chrome_button(ui, "Close", 2, height).clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), Stroke::new(1.0_f32, BORDER));
    }

    fn show_new_download(&mut self) {
        self.dialog = AddDialog::new(&self.default_save_dir);
        self.pending_browser_id = None;
        self.show_add = true;
    }

    pub(super) fn shell_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.show_new_download();
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Comma)) {
            self.open_settings();
        }
    }

    pub(super) fn resize_edges(&self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let rect = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("window-border"),
        ));
        painter.rect_stroke(rect.shrink(1.0), 12.0, Stroke::new(1.2_f32, BORDER));
        if !crate::settings_preview() {
            return;
        }
        let Some(position) = ctx.pointer_hover_pos() else {
            return;
        };
        let left = position.x < rect.left() + 5.0;
        let right = position.x > rect.right() - 5.0;
        let top = position.y < rect.top() + 5.0;
        let bottom = position.y > rect.bottom() - 5.0;
        use egui::ResizeDirection as D;
        let direction = match (left, right, top, bottom) {
            (true, _, true, _) => Some((D::NorthWest, egui::CursorIcon::ResizeNwSe)),
            (_, true, true, _) => Some((D::NorthEast, egui::CursorIcon::ResizeNeSw)),
            (true, _, _, true) => Some((D::SouthWest, egui::CursorIcon::ResizeNeSw)),
            (_, true, _, true) => Some((D::SouthEast, egui::CursorIcon::ResizeNwSe)),
            (true, _, _, _) => Some((D::West, egui::CursorIcon::ResizeHorizontal)),
            (_, true, _, _) => Some((D::East, egui::CursorIcon::ResizeHorizontal)),
            (_, _, true, _) => Some((D::North, egui::CursorIcon::ResizeVertical)),
            (_, _, _, true) => Some((D::South, egui::CursorIcon::ResizeVertical)),
            _ => None,
        };
        if let Some((direction, cursor)) = direction {
            ctx.set_cursor_icon(cursor);
            if ctx.input(|input| input.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        }
    }
}
