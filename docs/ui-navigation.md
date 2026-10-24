# IDG GO interface routes and actions

This inventory maps the implemented Rust desktop interface to its control state and handlers. These are desktop views and dialogs, not website URL routes. It describes source behavior; verification results belong in the separate QA report.

## Application layout

`IDGApp::update` in `src/app.rs` composes the sidebar, `shell_header`, status bar, and central content. The central content switches between `settings_page` and the library's `library_header` / `download_table`. `add_dialog` and `update_prompt` are foreground windows. All views share the existing `Downloads` collection and download engine.

| Surface | Source |
| --- | --- |
| Window header, search, menus, keyboard shortcuts, locked sizing | `src/app/shell.rs` |
| Sidebar, statistic cards, toolbar, list/grid, details, footer | `src/app/library.rs` |
| New download, media inspection, update controls and action dispatch | `src/app.rs` |
| Settings draft, sections and Save / Reset | `src/app/settings.rs` |
| Windows tray, startup, geometry recovery and browser repair | `src/desktop.rs` |

## Sidebar and category filters

`IDGApp::sidebar` selects `self.category`, closes Settings, and clears the Settings draft when returning to the library. `Category::matches` determines membership. Counts come from `Category::count`, using the full current queue rather than the search result.

| Navigation entry | State | Actual filter |
| --- | --- | --- |
| All downloads | `Category::All` | Every download |
| Downloading | `Category::Downloading` | `Queued`, `Probing`, or `Downloading` |
| Completed | `Category::Completed` | `Completed` |
| Paused | `Category::Paused` | `Paused` |
| Failed | `Category::Failed` | `Failed` or `Cancelled` |
| Videos & audio | `Category::Videos` | File extension categorized by `file_category` |
| Documents | `Category::Documents` | File extension categorized by `file_category` |
| Archives | `Category::Archives` | File extension categorized by `file_category` |
| Other | `Category::Other` | All remaining file extensions |

`file_category` in `src/app/library.rs` recognizes video/audio (`mp4`, `mkv`, `webm`, `avi`, `mov`, `mp3`, `m4a`, `opus`, `flac`, `wav`, `ogg`, `aac`), documents (`pdf`, `doc`, `docx`, `xls`, `xlsx`, `ppt`, `pptx`, `txt`, `csv`, `epub`, `odt`), and archives (`zip`, `rar`, `7z`, `gz`, `tar`, `bz2`, `xz`). Classification uses the filename, not content inspection.

Other sidebar actions:

- **New download:** `open_add` resets `AddDialog`, clears any pending browser request and sets `show_add`.
- **Settings:** `open_settings` creates a `SettingsDraft` from `current_settings` and sets `show_settings`.
- **Download folder:** calls `open_path` with `default_save_dir` to open the real folder in Windows Explorer.

## Search, filters, statistic cards and display controls

- **Search downloads:** `shell_header` binds the text field to `self.search`. `download_table` applies a trimmed, case-insensitive filename substring search together with `Category::matches`. **Ctrl+F** focuses the search field.
- **Filter downloads:** the header sliders menu exposes the same nine `Category` values as the sidebar. Selecting one returns from Settings to the library.
- **Current speed / Active downloads / Completed / Queued:** `library_header` displays live queue statistics. The cards report state; category navigation is provided by the sidebar and filter menu.
- **Newest first / Oldest first:** `toolbar` updates `self.newest_first`; `download_table` sorts by `DownloadItem::added_at` and reverses for newest first.
- **List view / Grid view:** `toolbar` changes `library_state.grid`; `download_table` renders the same filtered download collection in rows or cards.
- **Empty state Add download:** opens the same working New download dialog through `open_add`. Its split menu also opens New download or Download folder settings. An empty search result displays a search-specific message.

## Selection, toolbar, context menus and details

`library_state.selected_ids` tracks bulk selection; `selected_id` controls the detail panel. Row/card selection and checkboxes are handled by `download_table`. Ctrl-click preserves other selected entries. The list header checkbox selects visible downloads. **Ctrl+A** selects filtered downloads when the UI is not editing text; **Escape** clears selection.

| Action | Handler and behavior |
| --- | --- |
| Resume | `handle_action(id, "Resume", items)` clears `pause_flag`; paused items return to downloading |
| Pause | `handle_action(id, "Pause", items)` sets `pause_flag` and `Status::Paused` for active transfers that support pause |
| Remove | Opens `library_state.confirm_remove`; confirmation calls `handle_action(id, "Delete", items)` for the selected entries |
| Open folder | `handle_action(id, "OpenFolder", items)` opens each selected item's parent directory using `open_path` |
| Copy link | The `download_table` context-menu dispatcher copies `DownloadItem::url` through `Context::copy_text` |
| Open file | `handle_action(id, "OpenFile", items)` uses `open_path`; the row context-menu item is enabled for completed files |

Toolbar Resume/Pause are enabled from applicable selection state. The table context menu provides Resume, Pause, Copy link, Open file, Open folder, and Remove, with state-dependent controls. Double-clicking a completed list row opens its file. Grid cards also provide applicable Resume/Pause and Open folder controls.

**Space** pauses applicable selected transfers or resumes the selection; **Delete** opens removal confirmation when text editing is not active. Confirmation offers **Cancel** and **Remove from list**. Removing sets cancellation flags and removes queue entries; it does not delete downloaded files.

`download_details` shows the selected filename, full destination (with hover text when truncated), downloaded bytes, connection/media format information, status, progress and transfer errors. Its full-height layout provides **Copy download link** and **Open file location**. Short windows use a compact detail summary.

## New download and browser confirmation

All entry points reach `IDGApp::add_dialog`: sidebar New download, empty-state Add download, header More → New download, **Ctrl+N**, and browser requests awaiting confirmation.

| Control | Handler and effect |
| --- | --- |
| Address | Edits `dialog.url`; a change refreshes the suggested filename and media inspection |
| Paste | Reads `arboard::Clipboard`, updates address and suggested filename |
| Filename | Edits `dialog.filename`; sets `filename_edited` to preserve the user's choice |
| Save to | Edits `dialog.save_dir` |
| Browse | `rfd::FileDialog::pick_folder`, initially using the chosen directory |
| Start download | Validates an HTTP/HTTPS URL; uses `safe_filename`, builds the real destination path and calls `enqueue` / `download::engine::start_download` |
| Cancel, window close, Escape | Closes the dialog and aborts pending media inspection; removes an unconfirmed browser request from the queue |

The dialog can initially paste an HTTP/HTTPS clipboard link. It retains the saved default download directory. Parallel connection policy is controlled in Settings rather than per-dialog.

For recognized YouTube URLs, `media::video_url` enables format controls. `inspect_media` calls `media::inspect` asynchronously and displays the title, available video heights, loading state or inspection errors. **Retry format check** repeats inspection. Changing format updates the filename extension.

- Video formats: MP4, MKV and WebM; choose best available or an upper height limit from inspected qualities (fallback choices are available while inspection is pending).
- Audio formats: MP3, M4A, Opus, WAV and FLAC. Applicable lossy formats expose best available or 320/256/192/128/96 kbps. WAV/FLAC display the source-quality conversion explanation.
- `MediaOptions` reaches the existing `media::download` toolchain through `start_download`; video includes audio, and source quality limits output quality.

When a browser sends a request with `awaiting_confirmation`, `IDGApp::update` populates the same dialog from that existing item. Start updates that item, clears `awaiting_confirmation` and invokes `start_download`; it does not create a duplicate item.

## Settings routes and actions

`settings_page` uses a scrollable body and persistent Save / Reset footer. Editing changes the local `SettingsDraft`; **Save changes** runs `apply_settings`, then `eframe::App::save` and storage flush. Changing a draft clears previous feedback. Leaving Settings for a library category discards its unsaved draft.

| Section | Controls | Implementation |
| --- | --- | --- |
| Save directory | Path field; Browse | Edits `draft.directory`; `rfd::FileDialog::pick_folder`; `apply_settings` requires an existing directory |
| Parallel connections | Automatic; Manual; Connections 1–32 | `radio` and `DragValue` edit `draft.segments`; `ConnectionSettings::save` persists policy; zero represents automatic |
| App behavior | Close to tray; Start IDG with Windows | `toggle` edits draft values; `desktop::set_startup` applies Windows startup; tray option is disabled when the tray is unavailable |
| Application updates | Automatic verified download; automatic installation; current status; Check for updates now | Draft update preferences plus live `update_controls` / `check_updates` |
| Browser integration | Connection status and reported error; automatic extension updates | Reads recent `integration::Connection` heartbeat; stores `auto_extension_updates` on Save |
| Browser integration | Detect and update extension now | `integration::synchronize` updates detected IDG unpacked extension folders |
| Browser integration | Choose existing extension folder | `rfd::FileDialog::pick_folder`, `integration::sync_folder`, and `integration::remember_folder` |
| Browser integration | Repair Chrome / Edge integration | Windows `desktop::repair_browser_integration`; reports errors or repair result |

**Reset to defaults** creates a new draft with the system Downloads folder, automatic connections, available tray behavior, Windows startup, automatic update download, manual update installation, and automatic extension updates. Reset still requires **Save changes**. Success/error/unsaved feedback remains visible in the footer.

The custom settings toggles support pointer interaction and focused **Space** activation. Normal egui text fields, checkboxes, radio buttons and action buttons remain keyboard controls.

## Application update popup and banner

`check_updates` invokes `updater::check_for_update` against the repository's latest GitHub release. `poll_updates` consumes background/manual results. `apply_update_check` opens the app window through `desktop::show_window` and sets `show_update_prompt` once a new version is announced in the session.

`update_prompt` displays current/new versions and **What's new**, using `updater::release_highlights` to format release notes. It contains shared `update_controls`, plus **Later** and window-close / Escape dismissal. Dismissal keeps the available update accessible.

Outside Settings, an available update, prepared installer or update error produces a top banner using `update_controls`. The Settings Application updates section uses those same controls:

- **Update now:** `download_update` → `updater::prepare_update`; shows download progress and verifies the SHA-256 digest.
- **Install update and restart IDG:** `install_update` → `updater::install_update`; enabled through `can_install_update` only when the Add dialog is closed and all queue items are completed, failed or cancelled.
- **Retry update check:** `check_updates` after an error.
- **Release notes:** opens the actual GitHub release URL.
- **What's new:** reopens `show_update_prompt` after dismissal.

Automatic installation follows the same readiness check. Pending and paused downloads must finish or be removed before restarting. Checks accept a newer version or a later source build of the same version, using the generated release-notes build marker. Older installed binaries without this logic require a one-time manual update.

## Window controls, More menu and tray

`shell_header` uses real egui viewport commands:

| Control | Handler |
| --- | --- |
| Drag free header area | `ViewportCommand::StartDrag` |
| Maximize / header double-click | Disabled in production to preserve the reference size |
| Minimize | `ViewportCommand::Minimized(true)` |
| Close | `ViewportCommand::Close`; `IDGApp::update` hides to tray when configured and available |
| Window size | Fixed 960 x 640 logical pixels, capped to monitor; resize/maximize disabled in production |

The **More options** menu connects:

- New download → `show_new_download` (**Ctrl+N** also uses this handler).
- Pause all downloads → `pause_all` for downloading items that support pause.
- Open download folder → `open_path(default_save_dir)`.
- Check for updates → `check_updates`.
- Settings → `open_settings` (**Ctrl+,** also opens Settings).
- Hide to system tray → `ViewportCommand::Visible(false)`; shown when a tray exists.
- Quit IDG → sets `exiting` and sends `ViewportCommand::Close`, bypassing close-to-tray behavior.

`SystemTray::new` in `src/desktop.rs` creates **Open IDG** and **Quit IDG**. `SystemTray::actions` feeds `TrayAction::Show` / `TrayAction::Quit` into `IDGApp::update`; show uses `desktop::show_window`, and quit follows the explicit exit path. `WindowGeometry::remember` / `save` preserve usable window geometry; `repair_window_builder` repairs unusable saved geometry on startup.

## Browser integration API paths

These are existing local integration routes, not navigation pages. `server::run_server` / `build_router` listen on `127.0.0.1:17843` and share the GUI's queue, connection policy and HTTP client.

| Local path | Handler | Purpose |
| --- | --- | --- |
| `GET /health` | `health` | App version and media capability |
| `POST /downloads` | `add_download` | Capture/enqueue a browser download, optionally awaiting GUI confirmation |
| `GET /downloads` | `list_downloads` | Current queue for browser integration |
| `POST /show` | `show_window` | Restore/show the real app window |
| `POST /extension/heartbeat` | `extension_heartbeat` | Extension version/connection/error state and synchronization feedback |

The status bar's browser-ready indicator reports bridge/API errors when present. It is distinct from the Settings heartbeat indicator, which confirms a recently connected extension. The footer also shows active downloads and `APP_VERSION`.

## Existing behavior and boundaries

- History persists across restarts without cookies/authorization headers. Interrupted workers are not resumed after restart; entries remain visible with an explanation. Saved preferences and connection settings retain their existing persistence.
- Pause/resume is available for ordinary file transfers; `DownloadItem::supports_pause` excludes YouTube toolchain downloads. Failed items have no dedicated Retry action in this interface.
- Remove cancels/removes an entry and leaves downloaded or partial files on disk. No Delete downloaded file action is introduced.
- Folder browsing is a native OS dialog from `rfd`; Explorer, clipboard integration and external release-note links retain OS behavior and styling.
- YouTube downloads require the bundled yt-dlp / FFmpeg / Deno tools and working upstream access. A selected format cannot improve original source quality.
- Extension synchronization operates on IDG's own recognized unpacked module folders. It does not install a store extension or rewrite browser preferences; a browser reload may still be required as described in Settings.
- At short window heights, details use the compact summary. Navigation and Settings scroll to keep their real actions reachable.
- Settings-preview fixtures are debug-only and isolate saved settings, clipboard auto-paste and updater behavior; preview controls are not product navigation routes.


Build-aware updates are parsed in `src/updater.rs` from the generated release-notes marker; `build.rs` embeds the local source revision/timestamp. Browser handoffs place `idg-reveal-download` in the egui event context, which `IDGApp::reveal_download` consumes to select the visible transfer. Persistent history lives in `src/download/history.rs` and excludes credentials/live worker state.
