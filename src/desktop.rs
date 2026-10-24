//! Windows notification-area integration and current-user startup settings.
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};

/// Initial client size and recovery size for invalid persisted geometry.
pub const DEFAULT_WINDOW_SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);
/// Smallest supported desktop layout, in egui points.
pub const MIN_WINDOW_SIZE: egui::Vec2 = egui::vec2(800.0, 500.0);

/// Honor the Windows accessibility preference for client-area animation.
pub fn reduced_motion() -> bool {
    #[cfg(windows)]
    {
        let mut enabled: i32 = 1;
        // SAFETY: enabled points to a valid BOOL-sized output for this SPI query.
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(
                windows_sys::Win32::UI::WindowsAndMessaging::SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut enabled as *mut i32).cast(),
                0,
            );
        }
        enabled == 0
    }
    #[cfg(not(windows))]
    false
}

/// Matches eframe 0.29's persisted `window` record. Its built-in saver reads
/// minimized HWNDs as 0x0 at (-32000, -32000), so retain usable geometry instead.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WindowGeometry {
    inner_position_pixels: Option<egui::Pos2>,
    outer_position_pixels: Option<egui::Pos2>,
    fullscreen: bool,
    inner_size_points: Option<egui::Vec2>,
}

impl WindowGeometry {
    /// Load the last usable normal window geometry, if any.
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        storage
            .and_then(|storage| eframe::get_value::<Self>(storage, "window"))
            .filter(Self::is_usable)
            .unwrap_or_default()
    }

    fn is_usable(&self) -> bool {
        self.inner_size_points.is_some_and(|size| {
            size.is_finite() && size.x >= MIN_WINDOW_SIZE.x && size.y >= MIN_WINDOW_SIZE.y
        }) && [self.inner_position_pixels, self.outer_position_pixels]
            .into_iter()
            .flatten()
            .all(|position| position.is_finite() && position.x > -32000.0 && position.y > -32000.0)
    }

    /// Sample normal frames; minimized/maximized frames must not replace them.
    pub fn remember(&mut self, ctx: &egui::Context) {
        let pixels_per_point = ctx.pixels_per_point();
        let candidate = ctx.input(|input| {
            let viewport = input.viewport();
            if viewport.minimized == Some(true)
                || viewport.maximized == Some(true)
                || viewport.fullscreen == Some(true)
            {
                return None;
            }
            let inner = viewport.inner_rect?;
            let outer = viewport.outer_rect?;
            Some(Self {
                inner_position_pixels: Some(inner.min * pixels_per_point),
                outer_position_pixels: Some(outer.min * pixels_per_point),
                fullscreen: false,
                inner_size_points: Some(inner.size()),
            })
        });
        if let Some(candidate) = candidate.filter(Self::is_usable) {
            *self = candidate;
        }
    }

    /// Replace eframe's snapshot with the last usable normal geometry.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        // App::save runs after eframe's saver; overwrite its minimized snapshot.
        eframe::set_value(storage, "window", self);
    }
}

/// Recover old minimized snapshots after eframe applies persisted geometry.
pub fn repair_window_builder(mut builder: egui::ViewportBuilder) -> egui::ViewportBuilder {
    if builder.inner_size.is_some_and(|size| {
        !size.is_finite() || size.x < MIN_WINDOW_SIZE.x || size.y < MIN_WINDOW_SIZE.y
    }) {
        builder.inner_size = Some(DEFAULT_WINDOW_SIZE);
        // The framework has already clamped the invalid minimized position to a
        // screen corner. Let Windows choose a fresh position along with the size.
        builder.position = None;
    }
    builder
}

#[derive(Clone, Copy)]
pub enum TrayAction {
    Show,
    Quit,
}

pub struct SystemTray {
    actions: Receiver<TrayAction>,
    #[cfg(windows)]
    _icon: tray_icon::TrayIcon,
}

impl SystemTray {
    #[cfg(windows)]
    pub fn new(ctx: &egui::Context) -> anyhow::Result<Self> {
        use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
        use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

        let menu = Menu::new();
        let show = MenuItem::with_id("idg-show", "Open IDG", true, None);
        let quit = MenuItem::with_id("idg-quit", "Quit IDG", true, None);
        menu.append_items(&[&show, &PredefinedMenuItem::separator(), &quit])?;
        let image =
            image::load_from_memory(include_bytes!("../extension/images/icon32.png"))?.into_rgba8();
        let (width, height) = image.dimensions();
        let icon = Icon::from_rgba(image.into_raw(), width, height)?;
        let icon = TrayIconBuilder::new()
            .with_id("idg-tray")
            .with_tooltip("IDG — downloads keep running in the background")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;

        let (sender, actions) = mpsc::channel();
        let context = ctx.clone();
        let menu_sender = sender.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = match event.id.as_ref() {
                "idg-show" => TrayAction::Show,
                "idg-quit" => TrayAction::Quit,
                _ => return,
            };
            let _ = menu_sender.send(action);
            match action {
                TrayAction::Show => show_window(&context),
                TrayAction::Quit => request_window_close(&context),
            }
            context.request_repaint();
        }));
        let context = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                let _ = sender.send(TrayAction::Show);
                show_window(&context);
            }
        }));
        Ok(Self {
            actions,
            _icon: icon,
        })
    }

    #[cfg(not(windows))]
    pub fn new(_ctx: &egui::Context) -> anyhow::Result<Self> {
        anyhow::bail!("System tray integration is available on Windows")
    }

    pub fn actions(&self) -> impl Iterator<Item = TrayAction> + '_ {
        self.actions.try_iter()
    }
}

/// A file lock prevents duplicate GUI instances, including simultaneous logins/browser launches.
pub fn acquire_instance() -> std::io::Result<Option<File>> {
    let directory = dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("IDG");
    std::fs::create_dir_all(&directory)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("instance.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
pub fn startup_enabled() -> std::io::Result<bool> {
    match winreg::HKCU.open_subkey(RUN_KEY) {
        Ok(key) => match key.get_value::<String, _>("IDG") {
            Ok(value) => Ok(!value.is_empty()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub fn startup_command(executable: &Path) -> String {
    format!("\"{}\" --background", executable.display())
}

/// Opening a complete portable/installed app repairs native host registration for this user.
#[cfg(windows)]
pub fn repair_browser_integration() -> anyhow::Result<()> {
    let executable = std::env::current_exe()?.with_file_name("IDG-native.exe");
    anyhow::ensure!(
        executable.is_file(),
        "Browser bridge is missing. Install the full IDG package."
    );
    let directory = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Windows AppData directory is unavailable"))?
        .join("IDG");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("com.rust_idm.host.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(include_str!("../extension/com.rust_idm.host.json"))?;
    manifest["path"] = serde_json::Value::String(executable.to_string_lossy().into_owned());
    std::fs::write(&path, serde_json::to_vec_pretty(&manifest)?)?;
    for browser in [r"Software\Google\Chrome", r"Software\Microsoft\Edge"] {
        let (key, _) = winreg::HKCU
            .create_subkey(format!(r"{browser}\NativeMessagingHosts\com.rust_idm.host"))?;
        key.set_value("", &path.to_string_lossy().as_ref())?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn set_startup(enabled: bool) -> std::io::Result<()> {
    let (preferences, _) = winreg::HKCU.create_subkey(r"Software\IDG")?;
    preferences.set_value("StartupConfigured", &1u32)?;
    let (key, _) = winreg::HKCU.create_subkey(RUN_KEY)?;
    if enabled {
        key.set_value("IDG", &startup_command(&std::env::current_exe()?))
    } else {
        match key.delete_value("IDG") {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

#[cfg(not(windows))]
pub fn startup_enabled() -> std::io::Result<bool> {
    Ok(false)
}

#[cfg(not(windows))]
pub fn set_startup(_enabled: bool) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Windows startup is unavailable on this platform",
    ))
}

/// Enable background startup on first portable run; preserve the user's later choice.
#[cfg(windows)]
pub fn ensure_startup_default() -> std::io::Result<()> {
    let (key, _) = winreg::HKCU.create_subkey(r"Software\IDG")?;
    if key.get_value::<u32, _>("StartupConfigured").unwrap_or(0) == 0 {
        set_startup(true)?;
        key.set_value("StartupConfigured", &1u32)?;
    } else if startup_enabled()? {
        // Keep the startup entry valid when a portable installation is moved.
        set_startup(true)?;
    }
    Ok(())
}

/// Capture only our own window handle; it remains valid for the GUI lifetime.
pub fn attach_window(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = cc.window_handle()
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            cc.egui_ctx
                .data_mut(|data| data.insert_temp(egui::Id::new("idg-window"), handle.hwnd.get()));
            let rounded: u32 = 2; // DWMWCP_ROUND on Windows 11; ignored by Windows 10.
            // SAFETY: this HWND belongs to the current app; rounded is a valid
            // DWORD input for the duration of the synchronous DWM call.
            unsafe {
                windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                    handle.hwnd.get() as _,
                    windows_sys::Win32::Graphics::Dwm::DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                    (&rounded as *const u32).cast(),
                    std::mem::size_of::<u32>() as u32,
                );
            }
        }
    }
    #[cfg(not(windows))]
    let _ = cc;
}

#[cfg(windows)]
fn request_window_close(ctx: &egui::Context) {
    if let Some(handle) = ctx.data(|data| data.get_temp::<isize>(egui::Id::new("idg-window"))) {
        // SAFETY: this handle belongs to this app. Posting a message is thread-safe
        // and wakes the native event loop even when no redraws occur while hidden.
        unsafe {
            // Hidden winit windows do not paint. Show without activation so the
            // queued Quit action can run and eframe can save and shut down cleanly.
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindowAsync(
                handle as _,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
            );
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                handle as _,
                windows_sys::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                0,
                0,
            );
        }
    }
}

pub fn show_window(ctx: &egui::Context) {
    #[cfg(windows)]
    if let Some(handle) = ctx.data(|data| data.get_temp::<isize>(egui::Id::new("idg-window"))) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            IsIconic, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindowAsync,
        };
        // SAFETY: the app owns this HWND. ShowWindowAsync safely wakes its UI
        // thread; a hidden winit window cannot rely on redraw requests alone.
        unsafe {
            let command = if IsIconic(handle as _) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            };
            ShowWindowAsync(handle as _, command);
            SetForegroundWindow(handle as _);
        }
    }

    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    // Windows was restored above, including when its hidden event loop sleeps.
    // A second restore through winit can race that operation and undo maximization.
    #[cfg(not(windows))]
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::Storage;

    #[derive(Default)]
    struct MemoryStorage(std::collections::HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn minimized_autosave_does_not_replace_the_last_usable_geometry() {
        let ctx = egui::Context::default();
        let mut geometry = WindowGeometry::default();
        let mut storage = MemoryStorage::default();
        let mut input = egui::RawInput::default();
        let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.inner_rect = Some(egui::Rect::from_min_size(
            egui::pos2(120.0, 140.0),
            egui::vec2(1100.0, 700.0),
        ));
        viewport.outer_rect = viewport.inner_rect;
        viewport.minimized = Some(false);
        let _ = ctx.run(input.clone(), |ctx| geometry.remember(ctx));
        assert_eq!(geometry.inner_size_points, Some(egui::vec2(1100.0, 700.0)));
        let normal_position = geometry.outer_position_pixels;

        let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.minimized = Some(true);
        viewport.inner_rect = Some(egui::Rect::from_min_size(
            egui::pos2(-32000.0, -32000.0),
            egui::Vec2::ZERO,
        ));
        viewport.outer_rect = viewport.inner_rect;
        let _ = ctx.run(input, |ctx| geometry.remember(ctx));
        // Reproduce eframe overwriting `window` before calling App::save.
        storage.set_string("window", "(inner_size_points:Some((x:0.0,y:0.0)))".into());
        geometry.save(&mut storage);
        let restored = WindowGeometry::load(Some(&storage));
        assert_eq!(restored.inner_size_points, Some(egui::vec2(1100.0, 700.0)));
        assert_eq!(restored.outer_position_pixels, normal_position);
    }

    #[test]
    fn previously_saved_minimized_window_recovers_default_size_and_position() {
        let mut storage = MemoryStorage::default();
        storage.set_string("window", "(inner_position_pixels:Some((x:-32000.0,y:-32000.0)),outer_position_pixels:Some((x:-32000.0,y:-32000.0)),fullscreen:false,inner_size_points:Some((x:0.0,y:0.0)))".into());
        assert!(
            WindowGeometry::load(Some(&storage))
                .inner_size_points
                .is_none()
        );
        // eframe clamps the old 0x0 snapshot to 64x64 at a screen edge before
        // invoking our builder hook.
        let repaired = repair_window_builder(
            egui::ViewportBuilder::default()
                .with_inner_size([64.0, 64.0])
                .with_position([0.0, 0.0]),
        );
        assert_eq!(repaired.inner_size, Some(DEFAULT_WINDOW_SIZE));
        assert_eq!(repaired.position, None);
        assert_eq!(
            repair_window_builder(
                egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0])
            )
            .inner_size,
            Some(egui::vec2(1200.0, 800.0))
        );
    }

    #[test]
    fn startup_quotes_paths_with_spaces_and_starts_in_background() {
        assert_eq!(
            startup_command(Path::new(r"C:\Program Files\IDG\IDG.exe")),
            r#""C:\Program Files\IDG\IDG.exe" --background"#
        );
    }
}
