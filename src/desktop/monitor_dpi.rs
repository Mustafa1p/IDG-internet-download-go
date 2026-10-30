//! Preserve normal logical client size through native per-monitor DPI transitions.
//! winit still receives every DPI message so rendering and pointer scaling agree.
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GetClientRect, GetWindowRect, IsIconic, IsZoomed, KillTimer, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOZORDER, SendMessageW, SetTimer, SetWindowPos, WM_DPICHANGED,
            WM_NCDESTROY, WM_SIZE, WM_TIMER, WM_WINDOWPOSCHANGED,
        },
    },
};

const SUBCLASS_ID: usize = 0x494447;
const RECHECK_TIMER: usize = SUBCLASS_ID + 1;
struct State {
    dpi: u32,
    logical: [f64; 2],
    changing: bool,
    queued: bool,
}

fn scaled_size(logical: [f64; 2], dpi: u32, available: [i32; 2]) -> [i32; 2] {
    std::array::from_fn(|i| {
        ((logical[i] * dpi as f64 / 96.0).round() as i32).clamp(1, available[i].max(1))
    })
}

pub(super) fn install(hwnd: HWND) {
    // SAFETY: attach_window supplies the live HWND owned by this GUI thread.
    // The allocation belongs to the subclass until WM_NCDESTROY.
    unsafe {
        let mut rect = RECT::default();
        let dpi = GetDpiForWindow(hwnd).max(96);
        if GetClientRect(hwnd, &mut rect) == 0 {
            return;
        }
        let state = Box::into_raw(Box::new(State {
            dpi,
            logical: [
                rect.right as f64 * 96.0 / dpi as f64,
                rect.bottom as f64 * 96.0 / dpi as f64,
            ],
            changing: false,
            queued: false,
        }));
        let installed = SetWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID, state as usize);
        if installed == 0 {
            drop(Box::from_raw(state));
        }
    }
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: comctl32 invokes this only on the owning UI thread. No Rust
    // references into State are held across calls that can reenter the subclass.
    // changing prevents nested resize notifications from replacing normal size.
    unsafe {
        let state = data as *mut State;
        if message == WM_NCDESTROY {
            KillTimer(hwnd, RECHECK_TIMER);
            RemoveWindowSubclass(hwnd, Some(procedure), id);
            drop(Box::from_raw(state));
            return DefSubclassProc(hwnd, message, wparam, lparam);
        }
        if message == WM_DPICHANGED && !(*state).changing {
            let dpi = (wparam & 0xffff) as u32;
            if dpi == 0 {
                return DefSubclassProc(hwnd, message, wparam, lparam);
            }
            (*state).changing = true;
            (*state).dpi = dpi;
            let logical = (*state).logical;
            let normal = IsIconic(hwnd) == 0 && IsZoomed(hwnd) == 0;
            let result = DefSubclassProc(hwnd, message, wparam, lparam);
            if normal {
                let mut outer = RECT::default();
                let mut client = RECT::default();
                let mut monitor = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if GetWindowRect(hwnd, &mut outer) != 0
                    && GetClientRect(hwnd, &mut client) != 0
                    && GetMonitorInfoW(
                        MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                        &mut monitor,
                    ) != 0
                {
                    let margins = [
                        outer.right - outer.left - client.right,
                        outer.bottom - outer.top - client.bottom,
                    ];
                    let size = scaled_size(
                        logical,
                        dpi,
                        [
                            monitor.rcWork.right - monitor.rcWork.left - margins[0],
                            monitor.rcWork.bottom - monitor.rcWork.top - margins[1],
                        ],
                    );
                    if client.right != size[0] || client.bottom != size[1] {
                        SetWindowPos(
                            hwnd,
                            std::ptr::null_mut(),
                            0,
                            0,
                            size[0] + margins[0],
                            size[1] + margins[1],
                            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
            }
            (*state).changing = false;
            return result;
        }
        if message == WM_WINDOWPOSCHANGED && !(*state).changing {
            // Native DPI can change after the move callback and even after a
            // posted message. Debounce until Windows has committed the move.
            (*state).queued = SetTimer(hwnd, RECHECK_TIMER, 50, None) != 0;
        }
        if message == WM_TIMER && wparam == RECHECK_TIMER {
            KillTimer(hwnd, RECHECK_TIMER);
            (*state).queued = false;
            let dpi = GetDpiForWindow(hwnd);
            if dpi > 0 && dpi != (*state).dpi {
                // Windows may update the window DPI only after returning from
                // WM_WINDOWPOSCHANGED. Reconcile from the deferred message, not
                // from the middle of the move notification.
                let mut suggested = RECT::default();
                if GetWindowRect(hwnd, &mut suggested) != 0 {
                    SendMessageW(
                        hwnd,
                        WM_DPICHANGED,
                        ((dpi << 16) | dpi) as usize,
                        (&mut suggested as *mut RECT) as isize,
                    );
                }
            } else if !(*state).changing && IsIconic(hwnd) == 0 && IsZoomed(hwnd) == 0 {
                let mut rect = RECT::default();
                if dpi > 0
                    && GetClientRect(hwnd, &mut rect) != 0
                    && rect.right > 0
                    && rect.bottom > 0
                {
                    (*state).logical = [
                        rect.right as f64 * 96.0 / dpi as f64,
                        rect.bottom as f64 * 96.0 / dpi as f64,
                    ];
                }
            }
            return 0;
        }
        let result = DefSubclassProc(hwnd, message, wparam, lparam);
        if message == WM_SIZE
            && !(*state).changing
            && !(*state).queued
            && IsIconic(hwnd) == 0
            && IsZoomed(hwnd) == 0
        {
            let dpi = GetDpiForWindow(hwnd);
            let mut rect = RECT::default();
            if dpi == (*state).dpi
                && GetClientRect(hwnd, &mut rect) != 0
                && rect.right > 0
                && rect.bottom > 0
            {
                (*state).logical = [
                    rect.right as f64 * 96.0 / dpi as f64,
                    rect.bottom as f64 * 96.0 / dpi as f64,
                ];
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dpi_round_trips_preserve_logical_size_and_fit_destination_work_area() {
        assert_eq!(scaled_size([960.0, 640.0], 120, [2560, 1380]), [1200, 800]);
        assert_eq!(scaled_size([960.0, 640.0], 96, [1920, 1032]), [960, 640]);
        assert_eq!(scaled_size([960.0, 640.0], 144, [2560, 1380]), [1440, 960]);
        assert_eq!(
            scaled_size([2000.0, 1400.0], 120, [1920, 1032]),
            [1920, 1032]
        );
    }
}
