//! Windows notification-area integration and current-user startup settings.
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};

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

#[cfg(windows)]
pub fn set_startup(enabled: bool) -> std::io::Result<()> {
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
            SW_RESTORE, SetForegroundWindow, ShowWindowAsync,
        };
        // SAFETY: the app owns this HWND. ShowWindowAsync safely wakes its UI
        // thread; a hidden winit window cannot rely on redraw requests alone.
        unsafe {
            ShowWindowAsync(handle as _, SW_RESTORE);
            SetForegroundWindow(handle as _);
        }
    }

    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_quotes_paths_with_spaces_and_starts_in_background() {
        assert_eq!(
            startup_command(Path::new(r"C:\Program Files\IDG\IDG.exe")),
            r#""C:\Program Files\IDG\IDG.exe" --background"#
        );
    }
}
