//! Development-only interaction checks against the real rendered egui app.
//! Uses egui input, never desktop automation, user data, or the browser API.
use egui::{Context, Event, Id, Key, Modifiers, PointerButton, RawInput, Rect};

pub(super) struct Snapshot {
    pub category: String,
    pub show_settings: bool,
    pub show_add: bool,
    pub grid: bool,
    pub selection_count: usize,
    pub search: String,
    pub statuses: Vec<(String, String)>,
    pub confirm_remove: bool,
    pub newest_first: bool,
}

#[derive(Clone, Copy)]
enum Action {
    Click(&'static str),
    Key(Key, Modifiers),
    Text(&'static str),
}

struct Step {
    name: &'static str,
    action: Action,
}

#[derive(Default)]
pub(super) struct PreviewTest {
    step: usize,
    frame: usize,
    results: Vec<serde_json::Value>,
    finished: bool,
    clicked: Option<egui::Pos2>,
    step_started: Option<std::time::Instant>,
}

fn steps() -> Vec<Step> {
    let mut steps = Vec::new();
    for label in [
        "All downloads",
        "Downloading",
        "Completed",
        "Paused",
        "Failed",
        "Videos & audio",
        "Documents",
        "Archives",
        "Other",
        "All downloads",
    ] {
        steps.push(Step {
            name: label,
            action: Action::Click(label),
        });
    }
    for (name, action) in [
        (
            "New download shortcut",
            Action::Key(Key::N, Modifiers::CTRL),
        ),
        (
            "Close add dialog",
            Action::Key(Key::Escape, Modifiers::NONE),
        ),
        (
            "Settings shortcut",
            Action::Key(Key::Comma, Modifiers::CTRL),
        ),
        ("Leave settings", Action::Click("All downloads")),
        ("Grid view", Action::Click("Grid view")),
        ("List view", Action::Click("List view")),
        ("Select all downloads", Action::Click("Select all")),
        ("Pause selected downloads", Action::Click("Pause")),
        ("Resume selected downloads", Action::Click("Resume")),
        ("Confirm remove", Action::Click("Remove")),
        ("Cancel remove", Action::Click("Cancel remove")),
        ("Clear selection", Action::Click("Select all")),
        ("Open sort menu", Action::Click("Sort")),
        ("Oldest first", Action::Click("Oldest first")),
        ("Reopen sort menu", Action::Click("Sort")),
        ("Newest first", Action::Click("Newest first")),
        ("Focus search", Action::Key(Key::F, Modifiers::CTRL)),
        ("Search report", Action::Text("report")),
        ("Select search text", Action::Key(Key::A, Modifiers::CTRL)),
        ("Clear search", Action::Key(Key::Backspace, Modifiers::NONE)),
        ("Maximize", Action::Click("Maximize")),
        ("Restore", Action::Click("Restore")),
        ("Minimize and restore", Action::Click("Minimize")),
    ] {
        steps.push(Step { name, action });
    }
    steps
}

impl PreviewTest {
    fn enabled(&self) -> bool {
        !self.finished
            && crate::settings_preview()
            && std::env::args().any(|arg| arg == "--preview-self-test")
    }

    pub(super) fn inject(&mut self, ctx: &Context, input: &mut RawInput) {
        if !self.enabled() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        let steps = steps();
        let Some(step) = steps.get(self.step) else {
            return;
        };
        if self.frame != 5 && self.frame != 6 {
            return;
        }
        let pressed = self.frame == 5;
        if pressed {
            self.step_started = Some(std::time::Instant::now());
        }
        match step.action {
            Action::Click(label) => {
                if pressed {
                    self.clicked = ctx
                        .data(|data| data.get_temp::<Rect>(Id::new(format!("qa:{label}"))))
                        .map(|rect| rect.center());
                    if self.clicked.is_none() {
                        self.finish(ctx, Some(format!("Missing rendered control: {label}")));
                        return;
                    }
                }
                if let Some(pos) = self.clicked {
                    input.events.push(Event::PointerMoved(pos));
                    input.events.push(Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                if label == "Minimize" && !pressed {
                    self.restore_minimized_window(ctx);
                }
            }
            Action::Key(key, modifiers) => {
                let modifiers = Modifiers {
                    command: modifiers.ctrl || modifiers.command,
                    ..modifiers
                };
                input.modifiers = modifiers;
                input.events.push(Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers,
                });
            }
            Action::Text(text) if pressed => input.events.push(Event::Text(text.into())),
            Action::Text(_) => {}
        }
    }

    pub(super) fn observe(&mut self, ctx: &Context, snapshot: Snapshot) {
        if !self.enabled() {
            return;
        }
        self.frame += 1;
        if self.frame < 24 {
            return;
        }
        let steps = steps();
        let Some(step) = steps.get(self.step) else {
            self.finish(ctx, None);
            return;
        };
        if step.name == "Minimize and restore"
            && self
                .step_started
                .is_some_and(|start| start.elapsed() < std::time::Duration::from_millis(900))
        {
            return;
        }
        let okay = match step.name {
            "New download shortcut" => snapshot.show_add,
            "Close add dialog" => !snapshot.show_add,
            "Settings shortcut" => snapshot.show_settings,
            "Leave settings" => !snapshot.show_settings && snapshot.category == "All downloads",
            "Grid view" => snapshot.grid,
            "List view" => !snapshot.grid,
            "Select all downloads" => {
                snapshot.selection_count == snapshot.statuses.len() && snapshot.selection_count > 0
            }
            "Pause selected downloads" => snapshot
                .statuses
                .iter()
                .any(|(name, status)| name == "ubuntu-desktop.iso" && status == "Paused"),
            "Resume selected downloads" => {
                ["ubuntu-desktop.iso", "Design assets.zip"]
                    .iter()
                    .all(|expected| {
                        snapshot
                            .statuses
                            .iter()
                            .any(|(name, status)| name == expected && status == "Downloading")
                    })
            }
            "Confirm remove" => snapshot.confirm_remove && snapshot.statuses.len() == 5,
            "Cancel remove" => !snapshot.confirm_remove && snapshot.statuses.len() == 5,
            "Clear selection" => snapshot.selection_count == 0,
            "Open sort menu" | "Reopen sort menu" => true,
            "Oldest first" => !snapshot.newest_first,
            "Newest first" => snapshot.newest_first,
            "Focus search" => ctx.memory(|memory| memory.has_focus(Id::new("download-search"))),
            "Search report" | "Select search text" => snapshot.search == "report",
            "Clear search" => snapshot.search.is_empty(),
            "Maximize" => ctx.input(|input| input.viewport().maximized == Some(true)),
            "Restore" => ctx.input(|input| input.viewport().maximized != Some(true)),
            "Minimize and restore" => {
                ctx.data(|data| data.get_temp::<bool>(Id::new("qa:minimized-native"))) == Some(true)
                    && ctx.input(|input| input.viewport().minimized != Some(true))
            }
            category => snapshot.category == category && !snapshot.show_settings,
        };
        self.results.push(serde_json::json!({
            "check": step.name, "passed": okay, "category": snapshot.category,
            "settings": snapshot.show_settings, "add_dialog": snapshot.show_add,
            "grid": snapshot.grid, "selection_count": snapshot.selection_count,
            "search": snapshot.search, "statuses": snapshot.statuses,
            "confirm_remove": snapshot.confirm_remove,
            "newest_first": snapshot.newest_first,
            "search_cursor": format!("{:?}", egui::TextEdit::load_state(ctx, Id::new("download-search")).and_then(|state| state.cursor.char_range())),
        }));
        self.write_report(None, true);
        if !okay {
            self.finish(
                ctx,
                Some(format!("Interaction assertion failed: {}", step.name)),
            );
        } else {
            self.step += 1;
            self.frame = 0;
            self.clicked = None;
            if self.step == steps.len() {
                self.finish(ctx, None);
            }
        }
    }

    fn restore_minimized_window(&self, ctx: &Context) {
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            #[cfg(windows)]
            if let Some(handle) = ctx.data(|data| data.get_temp::<isize>(Id::new("idg-window"))) {
                // SAFETY: this is the preview app's own attached HWND. The
                // thread only checks minimized state and restores that window.
                let minimized = unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::IsIconic(handle as _) != 0
                };
                ctx.data_mut(|data| data.insert_temp(Id::new("qa:minimized-native"), minimized));
            }
            crate::desktop::show_window(&ctx);
        });
    }

    fn finish(&mut self, ctx: &Context, error: Option<String>) {
        self.finished = true;
        self.write_report(error, false);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn write_report(&self, error: Option<String>, in_progress: bool) {
        let result = serde_json::json!({
            "passed": !in_progress && error.is_none(), "error": error, "in_progress": in_progress,
            "checks": self.results,
            "method": "Debug preview: synthetic egui input against actual rendered controls; native viewport state assertions",
        });
        let directory = std::path::Path::new("target/fluent-qa");
        let write = std::fs::create_dir_all(directory).and_then(|_| {
            std::fs::write(
                directory.join("interactive.json"),
                serde_json::to_vec_pretty(&result)?,
            )
        });
        if let Err(error) = write {
            eprintln!("Could not save interaction checks: {error}");
        }
    }
}
