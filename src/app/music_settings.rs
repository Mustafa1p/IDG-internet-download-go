//! Optional music backend setup and account status.
use super::*;
use crate::download::music;
use std::sync::Mutex;

#[derive(Clone, Default)]
pub(super) struct MusicState {
    pub busy: bool,
    pub message: String,
    pub error: Option<String>,
    pub accounts: Vec<music::Account>,
}

impl IDGApp {
    fn music_installation(&mut self, ui: &mut Ui, ctx: &egui::Context, ready: bool, busy: bool) {
        settings::section(ui, library::Icon::Folder, "Original-service music", |ui| {
            ui.label(if ready {
                "Music backend installed"
            } else {
                "Optional music backend is not installed"
            });
            ui.label("Audio comes from the selected service. Albums and playlists save into a new folder.");
            ui.add_enabled_ui(!busy, |ui| {
                if ui
                    .button(if ready {
                        "Reinstall music backend"
                    } else {
                        "Install music backend"
                    })
                    .clicked()
                {
                    self.music_action(ctx, "install");
                }
            });
            ui.label(RichText::new("Setup requires 64-bit Windows and Python 3.12 (64-bit) and downloads additional dependencies. Your normal file and video downloads keep working without it.").size(12.0).color(TEXT_DIM));
            ui.hyperlink_to(
                "Python for Windows",
                "https://www.python.org/downloads/windows/",
            );
            ui.hyperlink_to(
                "OnTheSpot backend and GPL-2.0 source",
                "https://github.com/ots-downloader/onthespot",
            );
        });
    }

    fn music_action(&mut self, ctx: &egui::Context, action: &'static str) {
        {
            let mut state = self.music_state.lock().unwrap();
            if state.busy {
                return;
            }
            state.busy = true;
            state.error = None;
            state.message = if action == "install" {
                "Installing music dependencies. This may take several minutes…"
            } else {
                "Connecting to the music backend…"
            }
            .into();
        }
        let state = self.music_state.clone();
        let ctx = ctx.clone();
        ctx.request_repaint();
        self.rt.spawn(async move {
            let result = if action == "install" {
                match music::setup().await {
                    Ok(()) => music::action("status", |_| {}).await,
                    Err(error) => Err(error),
                }
            } else {
                let progress_state = state.clone();
                let progress_ctx = ctx.clone();
                music::action(action, move |message| {
                    progress_state.lock().unwrap().message = message.into();
                    progress_ctx.request_repaint();
                }).await
            };
            let mut state = state.lock().unwrap();
            state.busy = false;
            match result {
                Ok(accounts) => { state.accounts = accounts; state.message = if action == "status" { "Account status refreshed. Sign-in is checked when downloading." } else { "Music settings saved for this Windows user. Sign-in is checked when downloading." }.into(); },
                Err(error) => { state.error = Some(error.to_string()); state.message.clear(); }
            }
            ctx.request_repaint();
        });
    }

    pub(super) fn draw_music_settings(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let state = self.music_state.lock().unwrap().clone();
        let ready = music::ready();
        if ready && state.accounts.is_empty() && !state.busy && state.error.is_none() {
            self.music_action(ctx, "status");
        }
        if state.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&state.message);
            });
            ctx.request_repaint_after(Duration::from_millis(200));
        } else if !state.message.is_empty() {
            ui.label(&state.message);
        }
        if let Some(error) = &state.error {
            ui.colored_label(RED, error);
        }
        egui::ScrollArea::vertical().id_salt("music-settings-scroll").max_height(ui.available_height().max(100.0)).show(ui, |ui| {
            if ready {
                ui.collapsing("Music backend installed · setup and licenses", |ui| self.music_installation(ui, ctx, ready, state.busy));
            } else { self.music_installation(ui, ctx, ready, state.busy); }
            settings::section(ui, library::Icon::Wrench, "Music accounts", |ui| {
                ui.add_enabled_ui(ready && !state.busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Refresh accounts").clicked() { self.music_action(ctx, "status"); }
                        if ui.button("Connect Spotify").clicked() { self.music_action(ctx, "pair_spotify"); }
                        if ui.button("Manage music accounts…").clicked() { self.music_action(ctx, "accounts"); }
                    });
                });
                ui.label(RichText::new("For Spotify, select OnTheSpot in Spotify Desktop's Connect to a device menu. Use Manage music accounts for Apple Music, Deezer, Qobuz and Tidal; close that window to save and return here.").size(12.0).color(TEXT_DIM));
                for (key, label) in [("spotify", "Spotify"), ("bandcamp", "Bandcamp"), ("apple_music", "Apple Music"), ("deezer", "Deezer"), ("qobuz", "Qobuz"), ("tidal", "Tidal")] {
                    let configured = state.accounts.iter().any(|a| a.service == key && a.configured);
                    ui.horizontal(|ui| { ui.label(label); ui.colored_label(if configured { SUCCESS } else { TEXT_DIM }, if configured { "Configured" } else if key == "bandcamp" { "Public access · install backend first" } else { "Not connected / not checked" }); });
                }
                ui.label(RichText::new("Account data is encrypted for your Windows user. Subscriptions, source quality and regional availability still apply. Music downloads currently require Unlimited speed and do not support pause.").size(12.0).color(TEXT_DIM));
            });
        });
    }
}

pub(super) fn initial_state() -> Arc<Mutex<MusicState>> {
    Arc::new(Mutex::new(MusicState::default()))
}
