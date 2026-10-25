//! Compact preference tabs with a persistent action bar.
use super::library::{Icon, draw_icon};
use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct SettingsDraft {
    directory: String,
    segments: usize,
    pub(super) speed_limit_kbps: u64,
    close_to_tray: bool,
    startup: bool,
    download_updates: bool,
    install_updates: bool,
    extension_updates: bool,
}

pub(super) fn default_download_folder() -> String {
    dirs::download_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".into())
}

fn hint(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(TEXT_DIM)).wrap());
}

pub(super) fn section(ui: &mut Ui, icon: Icon, title: &str, body: impl FnOnce(&mut Ui)) {
    let response = Frame::none()
        .fill(PANEL)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(12.0)
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 10.0, ROW_SEL);
                draw_icon(ui.painter(), rect.shrink(9.0), icon, ACCENT);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(title)
                            .size(15.0)
                            .color(TEXT)
                            .family(egui::FontFamily::Name("idg-heading".into())),
                    );
                    ui.add_space(4.0);
                    body(ui);
                });
            });
        });
    qa_control(ui.ctx(), "Settings section", response.response.rect);
    ui.add_space(6.0);
}

fn action(ui: &mut Ui, icon: Icon, text: &str, primary: bool) -> egui::Response {
    let response = ui.add(
        egui::Button::new(RichText::new(format!("        {text}")).color(TEXT))
            .min_size(Vec2::new(0.0, 30.0))
            .fill(if primary { ACCENT } else { ROW_EVEN })
            .stroke(Stroke::new(1.0_f32, if primary { ACCENT2 } else { BORDER }))
            .rounding(8.0),
    );
    let rect = egui::Rect::from_center_size(
        egui::pos2(response.rect.left() + 19.0, response.rect.center().y),
        Vec2::splat(18.0),
    );
    draw_icon(ui.painter(), rect, icon, TEXT);
    qa_control(ui.ctx(), text, response.rect);
    response
}

fn checkbox(ui: &mut Ui, value: &mut bool, label: &str) -> egui::Response {
    let checked = *value;
    ui.scope(|ui| {
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.bg_fill = if checked { ACCENT2 } else { BG };
        visuals.widgets.inactive.bg_stroke =
            Stroke::new(1.0_f32, if checked { ACCENT } else { TEXT_DIM });
        visuals.widgets.inactive.fg_stroke = Stroke::new(2.0_f32, TEXT);
        visuals.widgets.hovered.bg_fill = if checked { ACCENT2 } else { ROW_SEL };
        visuals.widgets.hovered.fg_stroke = Stroke::new(2.0_f32, TEXT);
        ui.checkbox(value, label)
    })
    .inner
}

fn radio(ui: &mut Ui, value: &mut bool, selected: bool, label: &str) -> egui::Response {
    let checked = *value == selected;
    let response = ui
        .scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.bg_fill = BG;
            widgets.inactive.bg_stroke =
                Stroke::new(1.5_f32, if checked { ACCENT } else { TEXT_DIM });
            widgets.inactive.fg_stroke = Stroke::new(2.0_f32, ACCENT);
            widgets.hovered.bg_stroke = Stroke::new(1.5_f32, ACCENT);
            widgets.hovered.fg_stroke = Stroke::new(2.0_f32, ACCENT);
            ui.radio_value(value, selected, label)
        })
        .inner;
    qa_control(ui.ctx(), label, response.rect);
    response
}

fn toggle(ui: &mut Ui, value: &mut bool, label: &str, description: &str) {
    ui.horizontal_top(|ui| {
        let (rect, mut response) =
            ui.allocate_exact_size(Vec2::new(44.0, 25.0), egui::Sense::click());
        if response.clicked()
            || (ui.is_enabled()
                && response.has_focus()
                && ui.input(|input| input.key_pressed(egui::Key::Space)))
        {
            *value = !*value;
            response.mark_changed();
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
        });
        let animated = ui.ctx().animate_bool(response.id, *value);
        ui.painter()
            .rect_filled(rect, 13.0, if *value { ACCENT2 } else { BORDER });
        if response.has_focus() {
            ui.painter()
                .rect_stroke(rect.expand(3.0), 15.0, Stroke::new(1.5_f32, ACCENT));
        }
        ui.painter().circle_filled(
            egui::pos2(
                egui::lerp(rect.left() + 12.5..=rect.right() - 12.5, animated),
                rect.center().y,
            ),
            9.0,
            if ui.is_enabled() { TEXT } else { TEXT_DIM },
        );
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            let label_response = ui.add(egui::Label::new(label).wrap().sense(egui::Sense::click()));
            if label_response.clicked() && ui.is_enabled() {
                *value = !*value;
            }
            hint(ui, description);
        });
    });
}

impl IDGApp {
    #[cfg(debug_assertions)]
    pub(super) fn capture_settings_preview(&mut self, ctx: &egui::Context) {
        if std::env::args().any(|argument| argument == "--ui-preview-hold") {
            return;
        }
        let key = egui::Id::new("settings-preview-frame");
        let count = ctx.data_mut(|data| {
            let count = data.get_temp::<usize>(key).unwrap_or(0) + 1;
            data.insert_temp(key, count);
            count
        });
        let capture_frame = 40;
        if count == capture_frame {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            let output = std::env::args()
                .find_map(|argument| {
                    argument
                        .strip_prefix("--settings-preview-output=")
                        .map(PathBuf::from)
                })
                .unwrap_or_else(|| PathBuf::from("target/settings-preview.png"));
            let pixels: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            if let Err(error) = image::save_buffer(
                &output,
                &pixels,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            ) {
                eprintln!("Could not capture settings preview: {error}");
            }
            let metadata = serde_json::json!({
                "logical_width": ctx.screen_rect().width(), "logical_height": ctx.screen_rect().height(),
                "pixels_per_point": ctx.pixels_per_point(), "native_pixels_per_point": ctx.native_pixels_per_point(),
                "image_width": image.width(), "image_height": image.height(),
                "version": APP_VERSION,
                "controls": (["Search downloads", "Minimize", "Maximize", "Close", "Settings section", "Save changes", "Reset to defaults"].into_iter().filter_map(|label| {
                    ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(format!("qa:{label}"))))
                        .map(|rect| (label, [rect.left(), rect.top(), rect.right(), rect.bottom()]))
                }).collect::<std::collections::BTreeMap<_, _>>()),
            });
            let _ = std::fs::write(
                output.with_extension("json"),
                serde_json::to_vec_pretty(&metadata).unwrap_or_default(),
            );
            self.exiting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }

    fn current_settings(&self) -> SettingsDraft {
        let connections = self.connection_settings.lock().unwrap().clone();
        SettingsDraft {
            directory: self.default_save_dir.clone(),
            segments: connections.requested_count(),
            speed_limit_kbps: connections.speed_limit_bps.unwrap_or(0) / 1024,
            close_to_tray: self.close_to_tray,
            startup: self.startup_enabled,
            download_updates: self.auto_download_updates,
            install_updates: self.auto_install_updates,
            extension_updates: self.auto_extension_updates,
        }
    }

    pub(super) fn open_settings(&mut self) {
        if !self.show_settings {
            self.settings_draft = Some(self.current_settings());
            self.settings_message = None;
            self.settings_error = None;
        }
        self.show_settings = true;
    }

    fn apply_settings(&mut self, draft: &SettingsDraft, ctx: &egui::Context) -> Result<(), String> {
        if draft.directory.trim().is_empty() || !PathBuf::from(&draft.directory).is_dir() {
            return Err("Choose an existing download folder before saving.".into());
        }
        if crate::settings_preview() {
            return Err("Preview only: settings are not written to your account.".into());
        }
        let previous = self.connection_settings.lock().unwrap().clone();
        let connections = ConnectionSettings {
            parallel_segments: (draft.segments != 0).then_some(draft.segments.clamp(1, 32)),
            speed_limit_bps: (draft.speed_limit_kbps > 0)
                .then_some(draft.speed_limit_kbps.saturating_mul(1024)),
        };
        connections
            .save()
            .map_err(|error| format!("Could not save parallel connections: {error}"))?;
        #[cfg(windows)]
        if draft.startup != self.startup_enabled
            && let Err(error) = desktop::set_startup(draft.startup)
        {
            return match previous.save() {
                Ok(()) => Err(format!(
                    "Could not update Windows startup: {error}. Changes were not applied."
                )),
                Err(rollback) => Err(format!(
                    "Could not update Windows startup: {error}. Could not restore parallel connections: {rollback}."
                )),
            };
        }
        #[cfg(not(windows))]
        let _ = previous;
        *self.connection_settings.lock().unwrap() = connections;
        self.connection_settings_error = None;
        self.default_save_dir = draft.directory.clone();
        self.close_to_tray = draft.close_to_tray && self.tray.is_some();
        self.startup_enabled = draft.startup;
        self.auto_download_updates = draft.download_updates;
        self.auto_install_updates = draft.install_updates;
        self.auto_extension_updates = draft.extension_updates;
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("extension-auto-update"),
                self.auto_extension_updates,
            )
        });
        Ok(())
    }

    pub(super) fn settings_page(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
    ) {
        let opening = self.settings_draft.is_none();
        let mut draft = self
            .settings_draft
            .take()
            .unwrap_or_else(|| self.current_settings());
        if opening
            && crate::settings_preview()
            && std::env::args().any(|argument| argument == "--settings-preview-manual")
        {
            draft.segments = 8;
        }
        if opening
            && crate::settings_preview()
            && std::env::args().any(|arg| arg == "--settings-preview-limited")
        {
            draft.speed_limit_kbps = 1024;
        }
        let initial_draft = draft.clone();
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
        ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
        ui.spacing_mut().interact_size.y = 26.0;
        ui.spacing_mut().icon_width = 19.0;
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        ui.spacing_mut().scroll.bar_width = 8.0;
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.0));
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.0));
        ui.visuals_mut().selection.bg_fill = ACCENT2;
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Settings").size(25.0).color(TEXT).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.hyperlink_to(
                    "GitHub source",
                    "https://github.com/Mustafa1p/IDG-internet-download-go",
                );
            });
        });
        let tab_id = egui::Id::new("settings-tab");
        let mut tab = ctx
            .data(|data| data.get_temp::<usize>(tab_id))
            .unwrap_or_else(|| {
                if crate::settings_preview() {
                    std::env::args()
                        .find_map(|arg| {
                            arg.strip_prefix("--preview-settings-tab=")
                                .and_then(|v| v.parse().ok())
                        })
                        .unwrap_or(0)
                } else {
                    0
                }
            });
        ui.horizontal(|ui| {
            for (index, label) in ["Downloads", "App", "Updates", "Browser", "Music"]
                .iter()
                .enumerate()
            {
                let response = ui.selectable_value(&mut tab, index, *label);
                qa_control(ctx, &format!("Settings {label}"), response.rect);
            }
        });
        ctx.data_mut(|data| data.insert_temp(tab_id, tab));
        ui.add_space(6.0);
        match tab {
            4 => {
                self.draw_music_settings(ui, ctx);
            }
            0 => section(ui, Icon::Folder, "Downloads & bandwidth", |ui| {
                ui.label("Save directory");
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 132.0).max(80.0);
                    ui.add_sized(
                        [width, 30.0],
                        egui::TextEdit::singleline(&mut draft.directory).desired_width(width),
                    );
                    if action(ui, Icon::Folder, "Browse…", false).clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_directory(&draft.directory)
                            .pick_folder()
                    {
                        draft.directory = path.to_string_lossy().into_owned();
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Connections");
                    let mut automatic = draft.segments == 0;
                    if radio(ui, &mut automatic, true, "Automatic").changed() {
                        draft.segments = 0;
                    }
                    if radio(ui, &mut automatic, false, "Manual").changed() {
                        draft.segments = 8;
                    }
                    if !automatic {
                        ui.add(egui::DragValue::new(&mut draft.segments).range(1..=32));
                    }
                });
                hint(
                    ui,
                    "Automatic selects up to 32 connections when the server supports ranges.",
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Speed limit");
                    let mut limited = draft.speed_limit_kbps > 0;
                    if radio(ui, &mut limited, false, "Unlimited").changed() {
                        draft.speed_limit_kbps = 0;
                    }
                    if radio(ui, &mut limited, true, "Limit").changed() {
                        draft.speed_limit_kbps = 1024;
                    }
                    if limited {
                        ui.add(
                            egui::DragValue::new(&mut draft.speed_limit_kbps)
                                .range(1..=1_000_000)
                                .suffix(" KiB/s"),
                        );
                    }
                });
                hint(
                    ui,
                    "Per download, shared by all connections. Applies to new downloads.",
                );
                if let Some(error) = &self.connection_settings_error {
                    ui.colored_label(RED, error);
                }
            }),
            1 => section(ui, Icon::Monitor, "App behavior", |ui| {
                ui.add_enabled_ui(self.tray.is_some(), |ui| {
                    toggle(
                        ui,
                        &mut draft.close_to_tray,
                        "Keep running when the window closes",
                        "Downloads continue in the tray. Choose Quit IDG to exit.",
                    );
                });
                ui.add_space(6.0);
                ui.add_enabled_ui(cfg!(windows), |ui| {
                    toggle(
                        ui,
                        &mut draft.startup,
                        "Start IDG with Windows",
                        "Launch quietly in the system tray.",
                    );
                });
                ui.add_space(6.0);
                hint(ui, "Developed by Mustafa Ali");
            }),
            2 => section(ui, Icon::Refresh, "Application updates", |ui| {
                hint(
                    ui,
                    &format!(
                        "IDG {} · build {}",
                        APP_VERSION,
                        &env!("IDG_BUILD_ID")[..env!("IDG_BUILD_ID").len().min(8)]
                    ),
                );
                checkbox(
                    ui,
                    &mut draft.download_updates,
                    "Download and verify updates automatically",
                );
                checkbox(
                    ui,
                    &mut draft.install_updates,
                    "Install when downloads are finished",
                );
                self.update_controls(ui, ctx);
                ui.add_enabled_ui(
                    self.update_rx.is_none() && self.update_download_rx.is_none(),
                    |ui| {
                        if action(ui, Icon::Refresh, "Check for updates now", false).clicked() {
                            self.check_updates(ctx);
                        }
                    },
                );
            }),
            _ => section(ui, Icon::Puzzle, "Browser integration", |ui| {
                let connection = ctx.data(|data| {
                    data.get_temp::<crate::integration::Connection>(egui::Id::new(
                        "extension-connection",
                    ))
                });
                if let Some(connection) = connection
                    .filter(|connection| connection.seen.elapsed() < Duration::from_secs(150))
                {
                    ui.label(
                        RichText::new(format!(
                            "Connected · {} ({})",
                            connection.version, connection.install_type
                        ))
                        .color(SUCCESS),
                    );
                    if let Some(error) = connection.error {
                        ui.colored_label(RED, error);
                    }
                } else {
                    hint(ui, "Open the extension popup in Chrome or Edge to connect.");
                }
                checkbox(
                    ui,
                    &mut draft.extension_updates,
                    "Update the detected extension with IDG",
                );
                if action(ui, Icon::Refresh, "Detect and update extension", false).clicked() {
                    match crate::integration::synchronize() {
                        Ok(count) => {
                            self.settings_message = Some(format!(
                                "Updated {count} extension folder(s). Reload the extension and website tabs."
                            ))
                        }
                        Err(error) => self.settings_error = Some(error.to_string()),
                    }
                }
                if action(ui, Icon::Folder, "Choose extension folder…", false).clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    match crate::integration::sync_folder(&folder)
                        .and_then(|_| crate::integration::remember_folder(&folder))
                    {
                        Ok(()) => {
                            self.settings_message = Some(
                                "Extension files are current. Reload it and website tabs.".into(),
                            )
                        }
                        Err(error) => self.settings_error = Some(error.to_string()),
                    }
                }
                #[cfg(windows)]
                if action(ui, Icon::Wrench, "Repair Chrome / Edge integration", false).clicked() {
                    match desktop::repair_browser_integration() {
                        Ok(()) => {
                            self.desktop_error = None;
                            self.settings_message = Some("Browser integration repaired.".into());
                        }
                        Err(error) => self.settings_error = Some(error.to_string()),
                    }
                }
                if let Some(error) = &self.desktop_error {
                    ui.colored_label(RED, error);
                }
            }),
        };
        if tab == 4 {
            self.settings_draft = Some(draft);
            return;
        }
        if draft != initial_draft {
            self.settings_error = None;
            self.settings_message = None;
        }
        ui.add_space((ui.available_height() - 65.0).max(0.0));
        ui.separator();
        let dirty = draft != self.current_settings();
        let feedback = self
            .settings_error
            .as_ref()
            .map(|text| (text.as_str(), RED))
            .or_else(|| {
                self.settings_message
                    .as_ref()
                    .map(|text| (text.as_str(), SUCCESS))
            })
            .unwrap_or((
                if dirty {
                    "Unsaved changes"
                } else {
                    "Changes apply after Save. Speed limits apply to new downloads."
                },
                TEXT_DIM,
            ));
        ui.add(egui::Label::new(RichText::new(feedback.0).size(12.0).color(feedback.1)).truncate())
            .on_hover_text(feedback.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if action(ui, Icon::Check, "Save changes", true).clicked() {
                self.settings_error = None;
                match self.apply_settings(&draft, ctx) {
                    Ok(()) => {
                        if let Some(storage) = frame.storage_mut() {
                            eframe::App::save(self, storage);
                            storage.flush();
                        }
                        self.settings_message = Some("Settings saved.".into());
                    }
                    Err(error) => {
                        self.settings_error = Some(error);
                        self.settings_message = None;
                    }
                }
            }
            if action(ui, Icon::Reset, "Reset to defaults", false).clicked() {
                draft = SettingsDraft {
                    directory: default_download_folder(),
                    segments: 0,
                    speed_limit_kbps: 0,
                    close_to_tray: self.tray.is_some(),
                    startup: cfg!(windows),
                    download_updates: true,
                    install_updates: false,
                    extension_updates: true,
                };
                self.settings_message =
                    Some("Defaults restored. Select Save changes to apply.".into());
                self.settings_error = None;
            }
        });
        self.settings_draft = Some(draft);
    }
}
