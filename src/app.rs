use std::{path::PathBuf, sync::Arc, time::Duration};

use egui::{Color32, FontId, Frame, Margin, RichText, Rounding, Stroke, Ui, Vec2};
mod library;
mod settings;
use tokio::runtime::Handle;

use crate::{
    APP_VERSION,
    desktop::{self, SystemTray, TrayAction},
    download::{
        connections::{ConnectionSettings, SharedConnectionSettings},
        engine::start_download,
        media::{self, MediaFormat, MediaInfo, MediaOptions},
        types::{DownloadItem, Downloads, Status, safe_filename},
    },
    server::{ServerState, run_server},
    updater::{PreparedUpdate, UpdateInfo, check_for_update},
};

// ── Palette ───────────────────────────────────────────────────────────────────
const BG: Color32 = Color32::from_rgb(11, 21, 29);
const PANEL: Color32 = Color32::from_rgb(20, 33, 43);
const SIDEBAR: Color32 = Color32::from_rgb(14, 26, 35);
const ROW_EVEN: Color32 = Color32::from_rgb(22, 37, 48);
// Calm row selection; filename text itself is not selectable.
const ROW_SEL: Color32 = Color32::from_rgb(20, 62, 62);
const ACCENT: Color32 = Color32::from_rgb(20, 212, 180);
const ACCENT2: Color32 = Color32::from_rgb(10, 139, 126);
const GREEN: Color32 = Color32::from_rgb(72, 199, 142);
const RED: Color32 = Color32::from_rgb(240, 80, 80);
const ORANGE: Color32 = Color32::from_rgb(255, 165, 0);
const GRAY: Color32 = Color32::from_rgb(110, 115, 135);
const DIMGRAY: Color32 = Color32::from_rgb(140, 157, 167);
const TEXT: Color32 = Color32::from_rgb(236, 243, 245);
const TEXT_DIM: Color32 = Color32::from_rgb(172, 189, 197);
const BORDER: Color32 = Color32::from_rgb(39, 61, 76);

// ── Categories ────────────────────────────────────────────────────────────────
#[derive(PartialEq, Clone, Copy)]
enum Category {
    All,
    Downloading,
    Completed,
    Paused,
    Failed,
    Videos,
    Documents,
    Archives,
    Other,
}
impl Category {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All downloads",
            Self::Downloading => "Downloading",
            Self::Completed => "Completed",
            Self::Paused => "Paused",
            Self::Failed => "Failed",
            Self::Videos => "Videos & audio",
            Self::Documents => "Documents",
            Self::Archives => "Archives",
            Self::Other => "Other",
        }
    }
    fn matches(self, i: &DownloadItem) -> bool {
        match self {
            Self::All => true,
            Self::Downloading => matches!(
                i.status,
                Status::Queued | Status::Probing | Status::Downloading
            ),
            Self::Completed => matches!(i.status, Status::Completed),
            Self::Paused => matches!(i.status, Status::Paused),
            Self::Failed => matches!(i.status, Status::Failed | Status::Cancelled),
            Self::Videos | Self::Documents | Self::Archives | Self::Other => {
                library::file_category(&i.filename) == self
            }
        }
    }
    fn count(self, v: &[DownloadItem]) -> usize {
        v.iter().filter(|i| self.matches(i)).count()
    }
}

// ── Add dialog ────────────────────────────────────────────────────────────────
struct AddDialog {
    url: String,
    save_dir: String,
    filename: String,
    filename_edited: bool,
    media_options: MediaOptions,
    inspected_url: String,
    media_info: Option<MediaInfo>,
    media_error: Option<String>,
    media_rx: Option<tokio::sync::oneshot::Receiver<Result<MediaInfo, String>>>,
    media_task: Option<tokio::task::JoinHandle<()>>,
}
impl AddDialog {
    fn new(dir: &str) -> Self {
        AddDialog {
            url: String::new(),
            save_dir: dir.to_owned(),
            filename: String::new(),
            filename_edited: false,
            media_options: MediaOptions::default(),
            inspected_url: String::new(),
            media_info: None,
            media_error: None,
            media_rx: None,
            media_task: None,
        }
    }
}

impl Drop for AddDialog {
    fn drop(&mut self) {
        if let Some(task) = self.media_task.take() {
            task.abort();
        }
    }
}

// ── App ───────────────────────────────────────────────────────────────────────
pub struct IDGApp {
    window_geometry: desktop::WindowGeometry,
    rt: Handle,
    tray: Option<SystemTray>,
    close_to_tray: bool,
    startup_enabled: bool,
    desktop_error: Option<String>,
    start_hidden: bool,
    exiting: bool,
    client: Arc<reqwest::Client>,
    downloads: Downloads,
    category: Category,
    selected_id: Option<String>,
    show_add: bool,
    pending_browser_id: Option<String>,
    search: String,
    newest_first: bool,
    dialog: AddDialog,
    show_settings: bool,
    settings_draft: Option<settings::SettingsDraft>,
    settings_message: Option<String>,
    settings_error: Option<String>,
    default_save_dir: String,
    connection_settings: SharedConnectionSettings,
    connection_settings_error: Option<String>,
    /// Set by the background update checker when a new version is available.
    update_info: Option<UpdateInfo>,
    /// Receiver for update check results.
    update_rx: Option<tokio::sync::oneshot::Receiver<Result<Option<UpdateInfo>, String>>>,
    background_update_rx: tokio::sync::mpsc::UnboundedReceiver<Result<Option<UpdateInfo>, String>>,
    show_update_prompt: bool,
    announced_update: Option<String>,
    update_download_rx: Option<tokio::sync::oneshot::Receiver<Result<PreparedUpdate, String>>>,
    update_progress: Option<tokio::sync::watch::Receiver<f32>>,
    prepared_update: Option<PreparedUpdate>,
    update_error: Option<String>,
    update_status: String,
    auto_download_updates: bool,
    auto_install_updates: bool,
    auto_extension_updates: bool,
}

impl IDGApp {
    pub fn new(
        cc: &eframe::CreationContext,
        rt: Handle,
        client: Arc<reqwest::Client>,
        background: bool,
    ) -> Self {
        setup_visuals(&cc.egui_ctx);
        if crate::settings_preview() {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    crate::settings_preview_size().into(),
                ));
        }
        // Apply the smaller layout once on upgrade, then respect later resizing.
        if !crate::settings_preview()
            && !cc
                .storage
                .and_then(|storage| eframe::get_value::<bool>(storage, "compact_layout_v1"))
                .unwrap_or(false)
        {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(960.0, 600.0)));
        }
        desktop::attach_window(cc);
        let (tray, mut desktop_error) = match SystemTray::new(&cc.egui_ctx) {
            Ok(tray) => (Some(tray), None),
            Err(error) => (
                None,
                Some(format!(
                    "System tray unavailable: {error}. Closing IDG will exit."
                )),
            ),
        };
        #[cfg(windows)]
        if !crate::settings_preview()
            && let Err(error) = desktop::repair_browser_integration()
        {
            desktop_error = Some(format!("Browser integration: {error}"));
        }
        #[cfg(windows)]
        if !crate::settings_preview()
            && let Err(error) = desktop::ensure_startup_default()
        {
            desktop_error = Some(format!("Windows startup: {error}"));
        }
        let startup_enabled = desktop::startup_enabled().unwrap_or_else(|error| {
            desktop_error = Some(format!("Could not read Windows startup setting: {error}"));
            false
        });
        let close_to_tray = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "close_to_tray"))
            .unwrap_or(true);
        let start_hidden = background && tray.is_some();

        let default_save_dir = cc
            .storage
            .and_then(|storage| eframe::get_value::<String>(storage, "default_save_dir"))
            .filter(|path| !path.trim().is_empty())
            .unwrap_or_else(settings::default_download_folder);
        let default_save_dir = if crate::settings_preview() {
            r"C:\Users\User\Downloads".into()
        } else {
            default_save_dir
        };

        let (connections, connection_settings_error) = match ConnectionSettings::load() {
            Ok(settings) => (settings, None),
            Err(error) => (
                ConnectionSettings::default(),
                Some(format!("Could not load connection settings: {error}")),
            ),
        };
        let connection_settings = Arc::new(std::sync::Mutex::new(connections));

        let downloads: Downloads = Arc::new(std::sync::Mutex::new(Vec::new()));

        // ── Start the local API server (used by the browser extension) ───────
        let srv_state = ServerState {
            downloads: downloads.clone(),
            client: client.clone(),
            rt_handle: rt.clone(),
            default_dir: default_save_dir.clone(),
            egui_ctx: cc.egui_ctx.clone(),
            connection_settings: connection_settings.clone(),
        };
        if !crate::settings_preview() {
            rt.spawn(run_server(srv_state));
        }

        let auto_download_updates = cc
            .storage
            .and_then(|s| eframe::get_value(s, "auto_download_updates"))
            .unwrap_or(true);
        let auto_install_updates = cc
            .storage
            .and_then(|s| eframe::get_value(s, "auto_install_updates"))
            .unwrap_or(false);
        let auto_extension_updates = cc
            .storage
            .and_then(|s| eframe::get_value(s, "auto_extension_updates"))
            .unwrap_or(true);
        cc.egui_ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("extension-auto-update"),
                auto_extension_updates,
            )
        });
        if auto_extension_updates && !crate::settings_preview() {
            let context = cc.egui_ctx.clone();
            rt.spawn_blocking(move || {
                if let Err(error) = crate::integration::synchronize() {
                    context.data_mut(|data| {
                        data.insert_temp(egui::Id::new("idg-extension-error"), error.to_string())
                    });
                }
                context.request_repaint();
            });
        }
        // ── Start update checker ─────────────────────────────────────────────
        let (update_tx, background_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let update_client = client.clone();
        let update_context = cc.egui_ctx.clone();
        if !crate::settings_preview() {
            rt.spawn(async move {
                let mut announced = None;
                loop {
                    let result = check_for_update(&update_client, APP_VERSION)
                        .await
                        .map_err(|e| e.to_string());
                    let notify = if let Ok(Some(info)) = &result {
                        let notify = announced.as_ref() != Some(&info.version);
                        announced = Some(info.version.clone());
                        notify
                    } else {
                        false
                    };
                    if update_tx.send(result).is_err() {
                        break;
                    }
                    // Hidden Windows windows do not redraw: wake the native
                    // window only for a newly discovered update, once per version.
                    if notify {
                        desktop::show_window(&update_context);
                    }
                    update_context.request_repaint();
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(60 * 60)) => {},
                        _ = update_tx.closed() => break,
                    }
                }
            });
        }

        let mut app = IDGApp {
            window_geometry: desktop::WindowGeometry::load(cc.storage),
            tray,
            close_to_tray,
            startup_enabled,
            desktop_error,
            start_hidden,
            exiting: false,
            rt,
            client,
            downloads,
            category: Category::All,
            selected_id: None,
            show_add: false,
            pending_browser_id: None,
            search: String::new(),
            newest_first: true,
            dialog: AddDialog::new(&default_save_dir),
            show_settings: crate::settings_preview(),
            settings_draft: None,
            settings_message: None,
            settings_error: None,
            default_save_dir,
            connection_settings,
            connection_settings_error,
            update_info: None,
            update_rx: None,
            background_update_rx,
            show_update_prompt: false,
            announced_update: None,
            update_download_rx: None,
            update_progress: None,
            prepared_update: None,
            update_error: dirs::config_dir().and_then(|config| {
                if crate::settings_preview() {
                    return None;
                }
                let path = config.join("IDG/update-error.txt");
                let error = std::fs::read_to_string(&path).ok();
                if error.is_some() {
                    let _ = std::fs::remove_file(path);
                }
                error
            }),
            update_status: if crate::settings_preview() {
                "Update checks are paused in preview."
            } else {
                "Checking for updates…"
            }
            .into(),
            auto_download_updates,
            auto_install_updates,
            auto_extension_updates,
        };
        if crate::settings_preview()
            && std::env::args().any(|argument| argument == "--update-preview")
        {
            app.show_settings = false;
            app.show_update_prompt = true;
            app.update_status = "IDG 1.4.4 is available".into();
            app.update_info = Some(UpdateInfo {
                version: "1.4.4".into(),
                release_notes: "- Restore download windows at their last usable size and position.\n- Show a clear update alert with release fixes.\n- Check for updates while IDG runs in the system tray.".into(),
                release_url: "https://github.com/Mustafa1p/IDG-internet-download-go/releases".into(),
                installer_url: String::new(),
                sha256: String::new(),
                size: 0,
            });
        }
        app
    }

    fn enqueue(
        &self,
        url: String,
        save_path: PathBuf,
        preserve_filename: bool,
        options: MediaOptions,
    ) {
        let segments = self.connection_settings.lock().unwrap().requested_count();
        let mut item = DownloadItem::new(url, save_path, segments);
        item.preserve_filename = preserve_filename;
        item.media_options = options;
        let id = item.id.clone();
        self.downloads.lock().unwrap().push(item);
        let dl = self.downloads.clone();
        let cl = self.client.clone();
        self.rt.spawn(async move {
            if let Err(e) = start_download(cl, dl.clone(), id.clone()).await {
                crate::download::types::with_item(&dl, &id, |i| {
                    i.status = Status::Failed;
                    i.error = Some(e.to_string());
                });
            }
        });
    }

    fn add_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_add {
            return;
        }

        // Auto-paste clipboard URL when the dialog first opens.
        if self.dialog.url.is_empty()
            && let Ok(mut cb) = arboard::Clipboard::new()
            && let Ok(text) = cb.get_text()
        {
            let t = text.trim().to_string();
            if t.starts_with("http://") || t.starts_with("https://") {
                self.dialog.filename = filename_from_url(&t);
                self.dialog.url = t;
            }
        }

        let canonical = media::video_url(&self.dialog.url).unwrap_or_default();
        if canonical != self.dialog.inspected_url {
            if let Some(task) = self.dialog.media_task.take() {
                task.abort();
            }
            self.dialog.inspected_url = canonical.clone();
            self.dialog.media_info = None;
            self.dialog.media_error = None;
            self.dialog.media_rx = None;
            if !canonical.is_empty() {
                self.inspect_media(ctx);
            }
        }
        if let Some(rx) = self.dialog.media_rx.as_mut()
            && let Ok(result) = rx.try_recv()
        {
            self.dialog.media_rx = None;
            match result {
                Ok(info) => {
                    if !self.dialog.filename_edited {
                        self.dialog.filename = format!(
                            "{}.{}",
                            safe_filename(&info.title),
                            self.dialog.media_options.format.extension()
                        );
                    }
                    self.dialog.media_info = Some(info);
                }
                Err(error) => self.dialog.media_error = Some(error),
            }
        }

        let mut should_close = false;

        egui::Window::new("New download")
            .resizable(false)
            .collapsible(false)
            .default_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.style_mut().spacing.item_spacing = Vec2::new(8.0, 8.0);

                // Address row
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [62.0, 22.0],
                        egui::Label::new(RichText::new("Address").size(12.0).color(TEXT_DIM)),
                    );
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.dialog.url)
                            .desired_width(300.0)
                            .font(FontId::proportional(12.0))
                            .hint_text("https://example.com/file.zip"),
                    );
                    if r.changed() {
                        self.dialog.filename = filename_from_url(&self.dialog.url);
                        self.dialog.filename_edited = false;
                    }
                    // Paste button
                    if ui
                        .small_button("Paste")
                        .on_hover_text("Paste from clipboard")
                        .clicked()
                        && let Ok(mut cb) = arboard::Clipboard::new()
                        && let Ok(text) = cb.get_text()
                    {
                        self.dialog.url = text.trim().to_string();
                        self.dialog.filename = filename_from_url(&self.dialog.url);
                        self.dialog.filename_edited = false;
                    }
                });

                if media::video_url(&self.dialog.url).is_some() {
                    ui.separator();
                    ui.label(RichText::new("YouTube · choose what to save").strong());
                    let old_format = self.dialog.media_options.format;
                    ui.horizontal(|ui| {
                        ui.label("File type");
                        egui::ComboBox::from_id_salt("youtube-format")
                            .selected_text(self.dialog.media_options.format.label())
                            .show_ui(ui, |ui| { for format in MediaFormat::ALL { ui.selectable_value(&mut self.dialog.media_options.format, format, format.label()); } });
                        if matches!(self.dialog.media_options.format, MediaFormat::Wav | MediaFormat::Flac) {
                            ui.label("Lossless conversion (source quality)");
                        } else if self.dialog.media_options.format.audio() {
                            ui.label("Audio quality");
                            let selected = self.dialog.media_options.audio_bitrate.map(|b| format!("{b} kbps")).unwrap_or_else(|| "Best available".into());
                            egui::ComboBox::from_id_salt("audio-quality").selected_text(selected).show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.dialog.media_options.audio_bitrate, None, "Best available");
                                for rate in [320, 256, 192, 128, 96] { ui.selectable_value(&mut self.dialog.media_options.audio_bitrate, Some(rate), format!("{rate} kbps")); }
                            });
                        } else {
                            ui.label("Video quality");
                            let selected = self.dialog.media_options.height.map(|h|format!("Up to {h}p")).unwrap_or_else(||"Best available".into());
                            egui::ComboBox::from_id_salt("video-quality").selected_text(selected).show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.dialog.media_options.height, None, "Best available");
                                let heights = self.dialog.media_info.as_ref().map(|info|info.heights.clone()).unwrap_or_else(||vec![2160,1440,1080,720,480,360,240,144]);
                                for height in heights { ui.selectable_value(&mut self.dialog.media_options.height, Some(height), format!("Up to {height}p")); }
                            });
                        }
                    });
                    if old_format != self.dialog.media_options.format {
                        let name = PathBuf::from(&self.dialog.filename).with_extension(self.dialog.media_options.format.extension());
                        self.dialog.filename = name.to_string_lossy().into_owned();
                    }
                    if self.dialog.media_rx.is_some() { ui.horizontal(|ui| { ui.spinner(); ui.label("Checking available video qualities…"); }); }
                    if let Some(error) = &self.dialog.media_error { ui.colored_label(ORANGE, error); if ui.button("Retry format check").clicked() { self.inspect_media(ctx); } }
                    ui.label(RichText::new("Video includes audio. MP3 and other audio types are converted; quality is limited by the source.").size(11.0).color(TEXT_DIM));
                    ui.separator();
                }

                ui.horizontal(|ui| {
                    ui.add_sized(
                        [62.0, 22.0],
                        egui::Label::new(RichText::new("Filename").color(TEXT_DIM)),
                    );
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.dialog.filename)
                                .desired_width(330.0),
                        )
                        .changed()
                    {
                        self.dialog.filename_edited = true;
                    }
                });
                // Save dir row
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [62.0, 22.0],
                        egui::Label::new(RichText::new("Save to").size(12.0).color(TEXT_DIM)),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.dialog.save_dir)
                            .desired_width(250.0)
                            .font(FontId::proportional(12.0)),
                    );
                    if ui.small_button("Browse…").clicked()
                        && let Some(p) = rfd::FileDialog::new()
                            .set_directory(&self.dialog.save_dir)
                            .pick_folder()
                    {
                        self.dialog.save_dir = p.to_string_lossy().into_owned();
                    }
                });

                ui.label(
                    RichText::new("Connection selection is managed in Settings.")
                        .size(11.0)
                        .color(TEXT_DIM),
                );

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(" Cancel ").clicked() {
                            should_close = true;
                        }
                        let can = reqwest::Url::parse(self.dialog.url.trim())
                            .is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
                        ui.add_enabled_ui(can, |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("   Start download   ").color(Color32::WHITE),
                                    )
                                    .fill(ACCENT2)
                                    .rounding(3.0),
                                )
                                .clicked()
                            {
                                let url = self.dialog.url.trim().to_string();
                                let dir = PathBuf::from(&self.dialog.save_dir);
                                let name = if self.dialog.filename.is_empty() {
                                    filename_from_url(&url)
                                } else {
                                    self.dialog.filename.trim().to_string()
                                };
                                let save_path = dir.join(safe_filename(&name));
                                if let Some(id) = self.pending_browser_id.take() {
                                    let mut downloads = self.downloads.lock().unwrap();
                                    if let Some(item) = downloads.iter_mut().find(|i| i.id == id) {
                                        item.url = url;
                                        item.filename = safe_filename(&name);
                                        item.save_path = save_path;
                                        item.preserve_filename = self.dialog.filename_edited;
                                        item.media_options = self.dialog.media_options.clone();
                                        item.awaiting_confirmation = false;
                                    }
                                    drop(downloads);
                                    let client = self.client.clone();
                                    let downloads = self.downloads.clone();
                                    self.rt.spawn(async move {
                                        if let Err(e) =
                                            start_download(client, downloads.clone(), id.clone())
                                                .await
                                        {
                                            crate::download::types::with_item(
                                                &downloads,
                                                &id,
                                                |item| {
                                                    item.status = Status::Failed;
                                                    item.error = Some(e.to_string());
                                                },
                                            );
                                        }
                                    });
                                } else {
                                    self.enqueue(url, save_path, self.dialog.filename_edited, self.dialog.media_options.clone());
                                }
                                should_close = true;
                            }
                        });
                    });
                });
            });

        // Close AFTER the window borrow is released.
        if should_close {
            if let Some(id) = self.pending_browser_id.take() {
                self.downloads.lock().unwrap().retain(|item| item.id != id);
            }
            self.show_add = false;
            if let Some(task) = self.dialog.media_task.take() {
                task.abort();
            }
        }
    }

    fn inspect_media(&mut self, ctx: &egui::Context) {
        if let Some(task) = self.dialog.media_task.take() {
            task.abort();
        }
        let url = self.dialog.url.clone();
        let cookies = self.pending_browser_id.as_ref().and_then(|id| {
            self.downloads
                .lock()
                .unwrap()
                .iter()
                .find(|i| &i.id == id)
                .and_then(|i| i.media_cookies.clone())
        });
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.dialog.media_rx = Some(rx);
        self.dialog.media_error = None;
        let ctx = ctx.clone();
        self.dialog.media_task = Some(self.rt.spawn(async move {
            let result = media::inspect(&url, cookies.as_deref())
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
            ctx.request_repaint();
        }));
    }

    fn check_updates(&mut self, ctx: &egui::Context) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.update_rx = Some(rx);
        self.update_error = None;
        self.update_status = "Checking for updates…".into();
        let client = self.client.clone();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            let _ = tx.send(
                check_for_update(&client, APP_VERSION)
                    .await
                    .map_err(|e| e.to_string()),
            );
            ctx.request_repaint();
        });
    }

    fn download_update(&mut self, ctx: &egui::Context) {
        let Some(info) = self.update_info.clone() else {
            return;
        };
        if self.update_download_rx.is_some() || self.prepared_update.is_some() {
            return;
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (progress_tx, progress_rx) = tokio::sync::watch::channel(0.0);
        self.update_download_rx = Some(rx);
        self.update_progress = Some(progress_rx);
        self.update_error = None;
        self.update_status = format!("Downloading and verifying IDG {}…", info.version);
        let client = self.client.clone();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            let result = crate::updater::prepare_update(&client, &info, progress_tx)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn can_install_update(&self) -> bool {
        !self.show_add
            && self.downloads.lock().unwrap().iter().all(|item| {
                matches!(
                    item.status,
                    Status::Completed | Status::Failed | Status::Cancelled
                )
            })
    }

    fn install_update(&mut self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("idg-installing"), true));
        if !self.can_install_update() {
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("idg-installing"), false));
            return;
        }
        if let Some(update) = self.prepared_update.take() {
            match crate::updater::install_update(update, self.startup_enabled) {
                Ok(()) => {
                    self.exiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(error) => {
                    ctx.data_mut(|data| data.insert_temp(egui::Id::new("idg-installing"), false));
                    self.update_error = Some(error.to_string());
                }
            }
        }
    }

    fn poll_updates(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.background_update_rx.try_recv() {
            self.apply_update_check(result, ctx);
        }
        if let Some(rx) = self.update_rx.as_mut()
            && let Ok(result) = rx.try_recv()
        {
            self.update_rx = None;
            self.apply_update_check(result, ctx);
        }
        if let Some(rx) = self.update_download_rx.as_mut()
            && let Ok(result) = rx.try_recv()
        {
            self.update_download_rx = None;
            self.update_progress = None;
            match result {
                Ok(update) => {
                    self.prepared_update = Some(update);
                    self.update_status =
                        "Update verified and ready. IDG will restart after installation.".into();
                }
                Err(error) => {
                    self.update_error = Some(error);
                }
            }
        }
        if self.auto_install_updates
            && self.prepared_update.is_some()
            && self.update_error.is_none()
            && self.can_install_update()
        {
            self.install_update(ctx);
        }
    }

    fn apply_update_check(
        &mut self,
        result: Result<Option<UpdateInfo>, String>,
        ctx: &egui::Context,
    ) {
        match result {
            Ok(info) => {
                if let Some(info) = &info {
                    if self.announced_update.as_ref() != Some(&info.version) {
                        self.announced_update = Some(info.version.clone());
                        self.show_update_prompt = true;
                        desktop::show_window(ctx);
                    }
                    // Preserve download/ready status across repeated checks.
                    if self.update_download_rx.is_none() && self.prepared_update.is_none() {
                        self.update_status = format!("IDG {} is available", info.version);
                    }
                } else if self.update_info.is_none() {
                    self.update_status = "IDG is up to date".into();
                }
                if info.is_some() {
                    self.update_info = info;
                }
                // A failed installer requires an explicit retry.
                if self.auto_download_updates && self.update_error.is_none() {
                    self.download_update(ctx);
                }
            }
            Err(error) => self.update_error = Some(format!("Update check failed: {error}")),
        }
    }

    fn update_prompt(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.show_update_prompt = false;
            return;
        }
        let Some(info) = self.update_info.clone() else {
            return;
        };
        let mut open = self.show_update_prompt;
        let mut later = false;
        egui::Window::new("A new IDG update is available")
            .id(egui::Id::new("idg-update-prompt"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .order(egui::Order::Foreground)
            .default_width(500.0)
            .show(ctx, |ui| {
                ui.heading(format!("Please update to IDG {}", info.version));
                ui.label(format!("You are using IDG {APP_VERSION}."));
                ui.add_space(10.0);
                ui.label(RichText::new("What’s new").strong().color(ACCENT));
                egui::ScrollArea::vertical()
                    .id_salt("update-release-notes")
                    .max_height((ctx.screen_rect().height() * 0.35).min(260.0))
                    .show(ui, |ui| {
                        for line in crate::updater::release_highlights(&info.release_notes) {
                            ui.horizontal_top(|ui| {
                                ui.label("•");
                                ui.add(egui::Label::new(line).wrap());
                            });
                        }
                    });
                ui.add_space(10.0);
                ui.separator();
                self.update_controls(ui, ctx);
                ui.add_space(6.0);
                later = ui.button("Later").clicked();
            });
        self.show_update_prompt = open && !later;
    }

    fn update_controls(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.label(&self.update_status);
        if let Some(progress) = &self.update_progress {
            ui.add(
                egui::ProgressBar::new(*progress.borrow())
                    .show_percentage()
                    .animate(true),
            );
        }
        if let Some(error) = &self.update_error {
            ui.colored_label(RED, error);
        }
        if self.prepared_update.is_some() {
            let ready = self.can_install_update();
            if ui
                .add_enabled(ready, egui::Button::new("Install update and restart IDG"))
                .clicked()
            {
                self.install_update(ctx);
            }
            if !ready {
                ui.label("Finish or remove pending and paused downloads before restarting.");
            }
        } else if self.update_info.is_some() && self.update_download_rx.is_none() {
            if ui.button("Update now").clicked() {
                self.download_update(ctx);
            }
        } else if self.update_error.is_some() && ui.button("Retry update check").clicked() {
            self.check_updates(ctx);
        }
        if let Some(info) = &self.update_info {
            ui.hyperlink_to("Release notes", &info.release_url);
            if !self.show_update_prompt && ui.button("What’s new").clicked() {
                self.show_update_prompt = true;
            }
        }
    }

    // ── Actions ───────────────────────────────────────────────────────────────
    fn handle_action(&mut self, id: &str, action: &str, items: &[DownloadItem]) {
        use std::sync::atomic::Ordering::Relaxed;
        match action {
            "Resume" => {
                let mut lock = self.downloads.lock().unwrap();
                if let Some(i) = lock.iter_mut().find(|i| i.id == id) {
                    i.pause_flag.store(false, Relaxed);
                    if matches!(i.status, Status::Paused) {
                        i.status = Status::Downloading;
                    }
                }
            }
            "Pause" => {
                let mut lock = self.downloads.lock().unwrap();
                if let Some(i) = lock
                    .iter_mut()
                    .find(|i| i.id == id && i.status.is_active() && i.supports_pause())
                {
                    i.pause_flag.store(true, Relaxed);
                    i.status = Status::Paused;
                    i.speed_bps = 0.0;
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
                if self.selected_id.as_deref() == Some(id) {
                    self.selected_id = None;
                }
            }
            "OpenFile" => {
                if let Some(i) = items.iter().find(|i| i.id == id) {
                    open_path(&i.save_path);
                }
            }
            "OpenFolder" => {
                if let Some(i) = items.iter().find(|i| i.id == id)
                    && let Some(d) = i.save_path.parent()
                {
                    open_path(d);
                }
            }
            _ => {}
        }
    }

    fn pause_all(&self) {
        let mut lock = self.downloads.lock().unwrap();
        for i in lock.iter_mut() {
            if matches!(i.status, Status::Downloading) && i.supports_pause() {
                i.pause_flag
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                i.status = Status::Paused;
                i.speed_bps = 0.0;
            }
        }
    }

    fn resume_selected(&self) {
        if let Some(ref id) = self.selected_id.clone() {
            let mut lock = self.downloads.lock().unwrap();
            if let Some(i) = lock.iter_mut().find(|i| &i.id == id) {
                i.pause_flag
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                if matches!(i.status, Status::Paused) {
                    i.status = Status::Downloading;
                }
            }
        }
    }

    fn pause_selected(&self) {
        if let Some(ref id) = self.selected_id.clone() {
            let mut lock = self.downloads.lock().unwrap();
            if let Some(i) = lock
                .iter_mut()
                .find(|i| &i.id == id && i.status.is_active() && i.supports_pause())
            {
                i.pause_flag
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                i.status = Status::Paused;
                i.speed_bps = 0.0;
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
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if crate::settings_preview() {
            return;
        }
        self.window_geometry.save(storage);
        eframe::set_value(storage, "compact_layout_v1", &true);
        eframe::set_value(storage, "close_to_tray", &self.close_to_tray);
        eframe::set_value(storage, "default_save_dir", &self.default_save_dir);
        eframe::set_value(
            storage,
            "auto_download_updates",
            &self.auto_download_updates,
        );
        eframe::set_value(storage, "auto_install_updates", &self.auto_install_updates);
        eframe::set_value(
            storage,
            "auto_extension_updates",
            &self.auto_extension_updates,
        );
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.window_geometry.remember(ctx);
        #[cfg(debug_assertions)]
        if crate::settings_preview() {
            self.capture_settings_preview(ctx);
        }
        if self.start_hidden {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.start_hidden = false;
        }
        if let Some(tray) = &self.tray {
            for action in tray.actions() {
                match action {
                    TrayAction::Show => desktop::show_window(ctx),
                    TrayAction::Quit => {
                        self.exiting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
        if ctx.input(|input| input.viewport().close_requested())
            && self.close_to_tray
            && self.tray.is_some()
            && !self.exiting
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        let items: Vec<DownloadItem> = self.downloads.lock().unwrap().clone();
        if !self.show_add
            && let Some(item) = items.iter().find(|item| item.awaiting_confirmation)
        {
            self.dialog = AddDialog::new(&self.default_save_dir);
            self.dialog.url = item.url.clone();
            self.dialog.filename = item.filename.clone();
            self.dialog.filename_edited = item.preserve_filename;
            self.dialog.media_options = item.media_options.clone();
            self.pending_browser_id = Some(item.id.clone());
            self.selected_id = Some(item.id.clone());
            self.show_add = true;
        }

        self.poll_updates(ctx);
        if !self.show_settings
            && (self.update_info.is_some()
                || self.update_error.is_some()
                || self.prepared_update.is_some())
        {
            egui::TopBottomPanel::top("update_banner")
                .frame(
                    Frame::none()
                        .fill(PANEL)
                        .inner_margin(Margin::symmetric(12.0, 8.0)),
                )
                .show(ctx, |ui| self.update_controls(ui, ctx));
        }

        if let Some(error) =
            ctx.data(|data| data.get_temp::<String>(egui::Id::new("idg-api-error")))
        {
            egui::TopBottomPanel::top("api_error").show(ctx, |ui| {
                ui.colored_label(RED, error);
            });
        }

        // ── Status bar ───────────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("statusbar")
            .min_height(22.0)
            .max_height(22.0)
            .frame(
                Frame::none()
                    .fill(Color32::from_rgb(16, 17, 26))
                    .inner_margin(Margin::symmetric(8.0, 3.0)),
            )
            .show(ctx, |ui| {
                self.status_bar(ui, &items);
            });

        // ── Sidebar ──────────────────────────────────────────────────────────
        egui::SidePanel::left("sidebar")
            .exact_width(
                if self.show_settings && ctx.screen_rect().width() >= 1100.0 {
                    250.0
                } else {
                    174.0
                },
            )
            .resizable(false)
            .frame(
                Frame::none()
                    .fill(SIDEBAR)
                    .inner_margin(Margin::same(10.0))
                    .stroke(Stroke::new(1.0_f32, BORDER)),
            )
            .show(ctx, |ui| {
                ui.style_mut().spacing.item_spacing.y = 1.0;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.sidebar(ui, &items);
                });
            });

        // ── Main area ────────────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG).inner_margin(Margin::same(14.0)))
            .show(ctx, |ui| {
                if self.show_settings {
                    self.settings_page(ui, ctx, frame);
                } else {
                    self.library_header(ui, &items);
                    self.download_table(ui, &items);
                }
            });

        // ── Dialogs ──────────────────────────────────────────────────────────
        if self.show_add {
            self.add_dialog(ctx);
        }
        if self.show_update_prompt {
            self.update_prompt(ctx);
        }

        // Repaint while active
        let active = items.iter().any(|i| i.status.is_active());
        ctx.request_repaint_after(if active {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(1)
        });
    }
}

// ── Visuals ───────────────────────────────────────────────────────────────────
fn setup_visuals(ctx: &egui::Context) {
    // Use Windows' own UI typeface when available; keep egui's portable fallback.
    let mut fonts = egui::FontDefinitions::default();
    let font_root = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .map(|path| path.join("Fonts"));
    if let Some(root) = font_root {
        for (name, file) in [("idg-ui", "segoeui.ttf"), ("idg-semibold", "seguisb.ttf")] {
            if let Ok(bytes) = std::fs::read(root.join(file)) {
                fonts
                    .font_data
                    .insert(name.into(), egui::FontData::from_owned(bytes));
            }
        }
    }
    if fonts.font_data.contains_key("idg-ui") {
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "idg-ui".into());
    }
    let mut headings = fonts.families[&egui::FontFamily::Proportional].clone();
    if fonts.font_data.contains_key("idg-semibold") {
        headings.insert(0, "idg-semibold".into());
    }
    fonts
        .families
        .insert(egui::FontFamily::Name("idg-heading".into()), headings);
    ctx.set_fonts(fonts);
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.extreme_bg_color = BG;
    v.widgets.inactive.weak_bg_fill = ROW_EVEN;
    v.widgets.hovered.weak_bg_fill = ROW_SEL;
    v.window_fill = PANEL;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ROW_SEL;
    v.selection.stroke = Stroke::new(1.0_f32, TEXT);
    v.hyperlink_color = ACCENT;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_rounding = Rounding::same(6.0);
    v.widgets.noninteractive.bg_fill = ROW_EVEN;
    v.widgets.inactive.bg_fill = ROW_EVEN;
    v.widgets.hovered.bg_fill = ROW_SEL;
    v.widgets.active.bg_fill = ACCENT2;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, BORDER);
    ctx.set_visuals(v);

    let mut s = (*ctx.style()).clone();
    s.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(12.5, egui::FontFamily::Proportional),
    );
    s.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(12.0, egui::FontFamily::Proportional),
    );
    s.text_styles.insert(
        egui::TextStyle::Small,
        FontId::new(10.5, egui::FontFamily::Proportional),
    );
    s.spacing.item_spacing = Vec2::new(6.0, 4.0);
    s.spacing.window_margin = Margin::same(8.0);
    s.spacing.button_padding = Vec2::new(10.0, 6.0);
    s.animation_time = 0.15;
    ctx.set_style(s);
}

// ── Helpers ───────────────────────────────────────────────────────────────────
fn fmt_size(b: u64) -> String {
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1_024.0;
    let f = b as f64;
    if f >= GB {
        format!("{:.2} GB", f / GB)
    } else if f >= MB {
        format!("{:.1} MB", f / MB)
    } else if f >= KB {
        format!("{:.0} KB", f / KB)
    } else {
        format!("{b} B")
    }
}

fn fmt_speed(bytes_per_second: f64) -> String {
    format!("{:.2} Mbps", bytes_per_second.max(0.0) * 8.0 / 1_000_000.0)
}

fn fmt_eta(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.0}s", secs)
    } else if secs < 3600.0 {
        format!("{:.0}m", secs / 60.0)
    } else {
        format!("{:.1}h", secs / 3600.0)
    }
}

fn filename_from_url(url: &str) -> String {
    crate::download::probe::filename_from_url(url)
}

fn status_info(s: &Status) -> (&'static str, Color32) {
    match s {
        Status::Queued => ("Queued", GRAY),
        Status::Probing => ("Connecting…", ORANGE),
        Status::Downloading => ("Downloading", ACCENT),
        Status::Paused => ("Paused", ORANGE),
        Status::Completed => ("Completed", GREEN),
        Status::Failed => ("Failed", RED),
        Status::Cancelled => ("Cancelled", DIMGRAY),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn speed_is_decimal_megabits_per_second() {
        assert_eq!(fmt_speed(1_000_000.0), "8.00 Mbps");
        assert_eq!(fmt_speed(125_000.0), "1.00 Mbps");
        assert_eq!(fmt_speed(0.0), "0.00 Mbps");
    }
}
