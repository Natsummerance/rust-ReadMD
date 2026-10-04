//! Windows Raw Input capture. Packet decoding and scan-code mapping are adapted
//! from ayangweb/BongoCat e5922f3 (Apache-2.0); see third_party/bongocat.
//! The hidden HWND, its userdata, timer and Raw Input registration all belong
//! to this worker. Only snapshots cross threads; keyboard text is never read.
use super::pressed::{smoothing_alpha, PressedInput};
use super::{probe_cursor, BongoInputState, HitTestTarget, InputEvent};
use std::cell::Cell;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, MapVirtualKeyW};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
    RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDEV_REMOVE, RID_INPUT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Packet {
    Key { scan: u16, flags: u16, vk: u16 },
    Mouse { flags: u16 },
}

// Accessibility tools and remote desktops use injected OS input, which does
// not generate WM_INPUT. Observe only these additional edges; physical devices
// continue through Raw Input. No characters, text or foreground titles are read.
const INJECTED_KEY: u32 = WM_APP + 31;
const INJECTED_MOUSE: u32 = WM_APP + 32;
thread_local! { static INJECTED_TARGET: Cell<HWND> = const { Cell::new(null_mut()) }; }

fn injected_key(scan: u16, vk: u16, flags: u32, message: u32) -> Option<Packet> {
    if flags & 0x10 == 0 || !matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP)
    {
        return None;
    }
    Some(Packet::Key {
        scan,
        vk,
        flags: u16::from(matches!(message, WM_KEYUP | WM_SYSKEYUP)) | ((flags as u16 & 1) << 1),
    })
}

unsafe extern "system" fn injected_keyboard_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let key = &*(lp as *const KBDLLHOOKSTRUCT);
        let scan = if key.scanCode == 0 {
            MapVirtualKeyW(key.vkCode, 0)
        } else {
            key.scanCode
        };
        if let Some(Packet::Key { scan, flags, vk }) =
            injected_key(scan as u16, key.vkCode as u16, key.flags, wp as u32)
        {
            INJECTED_TARGET.with(|target| {
                PostMessageW(
                    target.get(),
                    INJECTED_KEY,
                    vk as usize,
                    ((flags as usize) << 16 | scan as usize) as isize,
                );
            });
        }
    }
    CallNextHookEx(null_mut(), code, wp, lp)
}

unsafe extern "system" fn injected_mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 && (*(lp as *const MSLLHOOKSTRUCT)).flags & 1 != 0 {
        let flags = match wp as u32 {
            WM_LBUTTONDOWN => 1,
            WM_LBUTTONUP => 2,
            WM_RBUTTONDOWN => 4,
            WM_RBUTTONUP => 8,
            WM_MBUTTONDOWN => 16,
            WM_MBUTTONUP => 32,
            _ => 0,
        };
        if flags != 0 {
            INJECTED_TARGET.with(|target| {
                PostMessageW(target.get(), INJECTED_MOUSE, flags, 0);
            });
        }
    }
    CallNextHookEx(null_mut(), code, wp, lp)
}

// The upstream decoder validates the declared byte count before interpreting
// either union arm. Aligned storage is used only at the FFI boundary.
fn decode(bytes: &[u8], header: usize) -> Option<Packet> {
    if header < 8 || bytes.len() < header {
        return None;
    }
    let kind = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let count = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
    if count > bytes.len() || count < header {
        return None;
    }
    let short = |at| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    match kind {
        0 if count >= header + 20 => Some(Packet::Mouse {
            flags: short(header + 4)?,
        }),
        1 if count >= header + 8 => Some(Packet::Key {
            scan: short(header)?,
            flags: short(header + 2)?,
            vk: short(header + 6)?,
        }),
        _ => None,
    }
}

unsafe fn read_packet(lparam: LPARAM) -> Option<Packet> {
    let mut count = 0;
    let header = size_of::<RAWINPUTHEADER>() as u32;
    let handle = lparam as HRAWINPUT;
    if GetRawInputData(handle, RID_INPUT, null_mut(), &mut count, header) == u32::MAX
        || count < header
        || count > 65536
    {
        return None;
    }
    let mut words = vec![0usize; (count as usize).div_ceil(size_of::<usize>())];
    let read = GetRawInputData(
        handle,
        RID_INPUT,
        words.as_mut_ptr().cast(),
        &mut count,
        header,
    );
    if read == u32::MAX || read as usize > words.len() * size_of::<usize>() {
        return None;
    }
    decode(
        std::slice::from_raw_parts(words.as_ptr().cast(), read as usize),
        header as usize,
    )
}

struct Worker<F> {
    emit: F,
    target: Arc<Mutex<HitTestTarget>>,
    running: Arc<AtomicBool>,
    pressed: PressedInput,
    previous: Option<BongoInputState>,
    hovered: bool,
    last_frame: Instant,
    last_reconcile: Instant,
    pointer: Option<(f64, f64)>,
    gesture: Option<Gesture>,
}

fn cursor_exposed(hwnd: HWND, point: POINT) -> bool {
    if hwnd.is_null() {
        return false;
    }
    // Query native z-order, rather than using WindowFromPoint: that API skips
    // the pet while WS_EX_TRANSPARENT is set over its transparent pixels.
    let mut above = unsafe { GetWindow(hwnd, GW_HWNDPREV) };
    while !above.is_null() {
        let mut rect: RECT = unsafe { zeroed() };
        if unsafe { IsWindowVisible(above) } != 0
            && unsafe { IsIconic(above) } == 0
            && unsafe { GetWindowLongPtrW(above, GWL_EXSTYLE) } as u32 & WS_EX_TRANSPARENT == 0
            && unsafe { GetWindowRect(above, &mut rect) } != 0
            && point.x >= rect.left
            && point.x < rect.right
            && point.y >= rect.top
            && point.y < rect.bottom
        {
            return false;
        }
        above = unsafe { GetWindow(above, GW_HWNDPREV) };
    }
    true
}

struct Gesture {
    x: f64,
    y: f64,
    threshold: f64,
    draggable: bool,
    head: bool,
    started: bool,
    release_observed: Option<Instant>,
}

impl<F: Fn(InputEvent)> Worker<F> {
    fn publish(&mut self) {
        let mut snapshot = self.pressed.snapshot();
        if let Some(last) = &self.previous {
            snapshot.mouse_x = last.mouse_x;
            snapshot.mouse_y = last.mouse_y;
            snapshot.pointer_x = last.pointer_x;
            snapshot.pointer_y = last.pointer_y;
        }
        let active_before = self.previous.as_ref().is_some_and(|last| last.active);
        if snapshot.active != active_before {
            (self.emit)(InputEvent::Activity(snapshot.active));
        }
        self.previous = Some(snapshot.clone());
        (self.emit)(InputEvent::Bongo(snapshot));
    }

    fn packet(&mut self, packet: Packet) {
        match packet {
            Packet::Key { scan, flags, vk } => {
                if vk == 0xff {
                    return;
                }
                if let Some(hid) = scan_to_hid(scan, flags) {
                    let query = match (scan, flags & 2 != 0) {
                        (0x2a, _) => 0xa0,
                        (0x36, _) => 0xa1,
                        (0x1d, false) => 0xa2,
                        (0x1d, true) => 0xa3,
                        (0x38, false) => 0xa4,
                        (0x38, true) => 0xa5,
                        _ => vk as i32,
                    };
                    if self.pressed.key(hid, query, flags & 1 == 0) {
                        self.publish();
                    }
                }
            }
            Packet::Mouse { flags } => {
                for button in 0..5 {
                    if button == 0 && flags & 1 != 0 {
                        self.begin_gesture();
                    }
                    if button == 0 && flags & 2 != 0 {
                        self.finish_gesture();
                    }
                    if flags & (1 << (button * 2)) != 0 && self.pressed.mouse(button, true) {
                        self.publish();
                    }
                    if flags & (2 << (button * 2)) != 0 && self.pressed.mouse(button, false) {
                        self.publish();
                    }
                }
            }
        }
    }

    fn begin_gesture(&mut self) {
        let mut point: POINT = unsafe { zeroed() };
        let mut rect: RECT = unsafe { zeroed() };
        let mut target = self.target.lock().map(|g| g.clone()).unwrap_or_default();
        if target.hwnd == 0
            || unsafe { GetCursorPos(&mut point) } == 0
            || unsafe { GetWindowRect(target.hwnd as HWND, &mut rect) } == 0
        {
            return;
        }
        let dpi = unsafe { GetDpiForWindow(target.hwnd as HWND) };
        if dpi != 0 {
            target.scale_factor = dpi as f64 / 96.0;
        }
        let probe = probe_cursor(
            &target,
            (rect.left as f64, rect.top as f64),
            (point.x as f64, point.y as f64),
            true,
        );
        let exposed = cursor_exposed(target.hwnd as HWND, point);
        // Test the current cursor at the down edge, never the previous hover.
        if probe.head_clicked && exposed {
            self.gesture = Some(Gesture {
                x: point.x as f64,
                y: point.y as f64,
                threshold: 4.0 * target.scale_factor,
                draggable: !target.lock_position,
                head: probe.head_clicked,
                started: false,
                release_observed: None,
            });
        }
    }

    fn finish_gesture(&mut self) {
        if let Some(gesture) = self.gesture.take() {
            if !gesture.started && gesture.head {
                (self.emit)(InputEvent::PetClick);
            }
        }
    }

    fn reconcile_gesture_release(&mut self, now: Instant, left_down: bool) {
        if let Some(gesture) = self.gesture.as_mut() {
            if left_down {
                gesture.release_observed = None;
            } else {
                // Async state may be up before the queued release edge arrives.
                // A missing edge still cancels without manufacturing a click.
                let released = *gesture.release_observed.get_or_insert(now);
                if now.duration_since(released) >= Duration::from_millis(80) {
                    self.gesture = None;
                }
            }
        }
    }

    fn advance_gesture(&mut self, point: (f64, f64), left_down: bool) {
        // A queued release may arrive after polling sees the button up. Keep
        // its click gesture, but never start a drag from post-release motion.
        if !left_down {
            return;
        }
        if let Some(gesture) = self.gesture.as_mut() {
            if !gesture.started
                && (point.0 - gesture.x).hypot(point.1 - gesture.y) > gesture.threshold
            {
                gesture.started = true;
                if gesture.draggable {
                    (self.emit)(InputEvent::Hover(true));
                    (self.emit)(InputEvent::DragStart);
                }
            }
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let refresh_hover = now.duration_since(self.last_reconcile) >= Duration::from_millis(150);
        let mut point: POINT = unsafe { zeroed() };
        // Losing the input desktop (lock screen/UAC) invalidates held state.
        if unsafe { GetCursorPos(&mut point) } == 0 {
            self.gesture = None;
            if self
                .previous
                .as_ref()
                .is_some_and(|s| s.keyboard_down || s.mouse_down)
            {
                self.pressed.reset();
                self.publish();
            }
            return;
        }
        let left_down = unsafe { GetAsyncKeyState(1) as u16 & 0x8000 } != 0;
        self.advance_gesture((point.x as f64, point.y as f64), left_down);
        self.reconcile_gesture_release(now, left_down);
        if refresh_hover {
            self.last_reconcile = now;
            if self
                .pressed
                .reconcile(|vk| unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 })
            {
                self.publish();
            }
        }
        let mut target = self.target.lock().map(|g| g.clone()).unwrap_or_default();
        let mut origin = (
            target.window_x * target.scale_factor,
            target.window_y * target.scale_factor,
        );
        let mut rect: RECT = unsafe { zeroed() };
        if target.hwnd != 0 && unsafe { GetWindowRect(target.hwnd as HWND, &mut rect) } != 0 {
            origin = (rect.left as f64, rect.top as f64);
            let dpi = unsafe { GetDpiForWindow(target.hwnd as HWND) };
            if dpi != 0 {
                target.scale_factor = dpi as f64 / 96.0;
            }
        }
        let probe = probe_cursor(&target, origin, (point.x as f64, point.y as f64), false);
        // Native user32 owns the drag. Hover must not turn the window transparent
        // or move it through a second coordinate path while capture is active.
        let hovered = probe.hovering && cursor_exposed(target.hwnd as HWND, point);
        // The native move loop temporarily forces an opaque surface. Reassert
        // the actual pointer state after it ends, even if the cursor moved out
        // while the move loop was active and our cached hover is already false.
        if !crate::platform::native_drag_active() && (hovered != self.hovered || refresh_hover) {
            self.hovered = hovered;
            (self.emit)(InputEvent::Hover(self.hovered));
        }
        let elapsed = now.duration_since(self.last_frame);
        if elapsed < Duration::from_millis(16) {
            return;
        }
        self.last_frame = now;
        let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        let mut info: MONITORINFO = unsafe { zeroed() };
        info.cbSize = size_of::<MONITORINFO>() as u32;
        let mut pointer = self.pointer.unwrap_or((0.0, 0.0));
        if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
            let area = info.rcMonitor;
            let w = (area.right - area.left).max(1) as f64;
            let h = (area.bottom - area.top).max(1) as f64;
            let wanted = (
                1.0 - 2.0 * (point.x - area.left) as f64 / w,
                1.0 - 2.0 * (point.y - area.top) as f64 / h,
            );
            let alpha = if self.pointer.is_none() {
                1.0
            } else {
                smoothing_alpha(elapsed)
            };
            pointer.0 += (wanted.0.clamp(-1.0, 1.0) - pointer.0) * alpha;
            pointer.1 += (wanted.1.clamp(-1.0, 1.0) - pointer.1) * alpha;
            self.pointer = Some(pointer);
        }
        let mut snapshot = self.pressed.snapshot();
        snapshot.mouse_x = probe.rel_x;
        snapshot.mouse_y = probe.rel_y;
        snapshot.pointer_x = pointer.0;
        snapshot.pointer_y = pointer.1;
        if self.previous.as_ref() != Some(&snapshot) {
            let was_active = self.previous.as_ref().is_some_and(|s| s.active);
            if was_active != snapshot.active {
                (self.emit)(InputEvent::Activity(snapshot.active));
            }
            self.previous = Some(snapshot.clone());
            (self.emit)(InputEvent::Bongo(snapshot));
        }
    }
}

unsafe extern "system" fn window_proc<F: Fn(InputEvent)>(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(lp as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Worker<F>;
    if !state.is_null() {
        let state = &mut *state;
        match msg {
            INJECTED_KEY => state.packet(Packet::Key {
                scan: lp as u16,
                flags: ((lp as usize) >> 16) as u16,
                vk: wp as u16,
            }),
            INJECTED_MOUSE => state.packet(Packet::Mouse { flags: wp as u16 }),
            WM_INPUT => {
                if let Some(packet) = read_packet(lp) {
                    state.packet(packet);
                }
            }
            WM_TIMER => {
                if !state.running.load(Ordering::Relaxed) {
                    PostQuitMessage(0);
                } else {
                    state.tick();
                }
            }
            WM_INPUT_DEVICE_CHANGE | WM_POWERBROADCAST | WM_WTSSESSION_CHANGE => {
                state.gesture = None;
                state.pressed.reset();
                state.publish();
            }
            _ => {}
        }
    }
    // user32 requires DefWindowProc for foreground WM_INPUT cleanup.
    DefWindowProcW(hwnd, msg, wp, lp)
}

pub(super) fn run<F: Fn(InputEvent)>(
    target: Arc<Mutex<HitTestTarget>>,
    running: Arc<AtomicBool>,
    emit: F,
) {
    let now = Instant::now();
    let mut state = Box::new(Worker {
        emit,
        target,
        running,
        pressed: PressedInput::new(),
        previous: None,
        hovered: false,
        last_frame: now,
        last_reconcile: now,
        pointer: None,
        gesture: None,
    });
    let class: Vec<u16> = format!("ReadMDPetRawInput-{}", std::process::id())
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: state outlives HWND and all callbacks, and is destroyed only after
    // registration/timer removal and DestroyWindow on this same owner thread.
    unsafe {
        let module = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(window_proc::<F>);
        wc.hInstance = module;
        wc.lpszClassName = class.as_ptr();
        if RegisterClassW(&wc) == 0 {
            (state.emit)(InputEvent::Fault("input_window_registration_failed"));
            return;
        }
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            module,
            (&mut *state as *mut Worker<F>).cast::<c_void>(),
        );
        let mut devices = [
            RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 6,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            },
            RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 2,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            },
        ];
        let registered = !hwnd.is_null()
            && RegisterRawInputDevices(devices.as_ptr(), 2, size_of::<RAWINPUTDEVICE>() as u32)
                != 0;
        if registered && SetTimer(hwnd, 1, 8, None) != 0 {
            INJECTED_TARGET.with(|target| target.set(hwnd));
            let keyboard_hook =
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(injected_keyboard_hook), module, 0);
            let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(injected_mouse_hook), module, 0);
            WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
            state.tick();
            let mut msg: MSG = zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if !keyboard_hook.is_null() {
                UnhookWindowsHookEx(keyboard_hook);
            }
            if !mouse_hook.is_null() {
                UnhookWindowsHookEx(mouse_hook);
            }
            INJECTED_TARGET.with(|target| target.set(null_mut()));
            state.pressed.reset();
            state.publish();
            KillTimer(hwnd, 1);
            WTSUnRegisterSessionNotification(hwnd);
        } else {
            (state.emit)(InputEvent::Fault("raw_input_capture_unavailable"));
        }
        if registered {
            for device in &mut devices {
                device.dwFlags = RIDEV_REMOVE;
                device.hwndTarget = null_mut();
            }
            RegisterRawInputDevices(devices.as_ptr(), 2, size_of::<RAWINPUTDEVICE>() as u32);
        }
        if !hwnd.is_null() {
            DestroyWindow(hwnd);
        }
        UnregisterClassW(class.as_ptr(), module);
    }
}

// Layout-independent HID mapping ported from BongoCat's Windows capture.
fn scan_to_hid(scan: u16, flags: u16) -> Option<u16> {
    if flags & 4 != 0 && scan == 0x1d {
        return Some(0x48);
    }
    if flags & 2 != 0 {
        return Some(match scan {
            0x1c => 0x58,
            0x1d => 0xe4,
            0x35 => 0x54,
            0x37 => 0x46,
            0x38 => 0xe6,
            0x47 => 0x4a,
            0x48 => 0x52,
            0x49 => 0x4b,
            0x4b => 0x50,
            0x4d => 0x4f,
            0x4f => 0x4d,
            0x50 => 0x51,
            0x51 => 0x4e,
            0x52 => 0x49,
            0x53 => 0x4c,
            0x5b => 0xe3,
            0x5c => 0xe7,
            0x5d => 0x65,
            _ => return None,
        });
    }
    Some(match scan {
        0x01 => 0x29,
        0x02..=0x0a => 0x1e + scan - 2,
        0x0b => 0x27,
        0x0c => 0x2d,
        0x0d => 0x2e,
        0x0e => 0x2a,
        0x0f => 0x2b,
        0x10..=0x19 => {
            [0x14, 0x1a, 0x08, 0x15, 0x17, 0x1c, 0x18, 0x0c, 0x12, 0x13][(scan - 0x10) as usize]
        }
        0x1a => 0x2f,
        0x1b => 0x30,
        0x1c => 0x28,
        0x1d => 0xe0,
        0x1e..=0x26 => {
            [0x04, 0x16, 0x07, 0x09, 0x0a, 0x0b, 0x0d, 0x0e, 0x0f][(scan - 0x1e) as usize]
        }
        0x27 => 0x33,
        0x28 => 0x34,
        0x29 => 0x35,
        0x2a => 0xe1,
        0x2b => 0x31,
        0x2c..=0x32 => [0x1d, 0x1b, 0x06, 0x19, 0x05, 0x11, 0x10][(scan - 0x2c) as usize],
        0x33 => 0x36,
        0x34 => 0x37,
        0x35 => 0x38,
        0x36 => 0xe5,
        0x37 => 0x55,
        0x38 => 0xe2,
        0x39 => 0x2c,
        0x3a => 0x39,
        0x3b..=0x44 => 0x3a + scan - 0x3b,
        0x45 => 0x53,
        0x46 => 0x47,
        0x47 => 0x5f,
        0x48 => 0x60,
        0x49 => 0x61,
        0x4a => 0x56,
        0x4b => 0x5c,
        0x4c => 0x5d,
        0x4d => 0x5e,
        0x4e => 0x57,
        0x4f => 0x59,
        0x50 => 0x5a,
        0x51 => 0x5b,
        0x52 => 0x62,
        0x53 => 0x63,
        0x56 => 0x64,
        0x57 => 0x44,
        0x58 => 0x45,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gesture_worker(events: Arc<Mutex<Vec<InputEvent>>>) -> Worker<impl Fn(InputEvent)> {
        let now = Instant::now();
        Worker {
            emit: move |event| events.lock().unwrap().push(event),
            target: Arc::new(Mutex::new(HitTestTarget::default())),
            running: Arc::new(AtomicBool::new(true)),
            pressed: PressedInput::new(),
            previous: None,
            hovered: false,
            last_frame: now,
            last_reconcile: now,
            pointer: None,
            gesture: Some(Gesture {
                x: 10.0,
                y: 10.0,
                threshold: 4.0,
                draggable: true,
                head: true,
                started: false,
                release_observed: None,
            }),
        }
    }
    #[test]
    fn fast_release_survives_poll_before_queued_up_and_emits_once() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut worker = gesture_worker(events.clone());
        let now = Instant::now();
        worker.reconcile_gesture_release(now, false);
        worker.reconcile_gesture_release(now + Duration::from_millis(8), false);
        worker.packet(Packet::Mouse { flags: 2 });
        worker.packet(Packet::Mouse { flags: 2 });
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| matches!(e, InputEvent::PetClick))
                .count(),
            1
        );
    }
    #[test]
    fn release_grace_does_not_start_drag_from_later_pointer_motion() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut worker = gesture_worker(events.clone());
        worker.reconcile_gesture_release(Instant::now(), false);
        worker.advance_gesture((40.0, 40.0), false);
        assert!(!worker.gesture.as_ref().unwrap().started);
        assert!(!events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, InputEvent::DragStart)));
        worker.packet(Packet::Mouse { flags: 2 });
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| matches!(e, InputEvent::PetClick))
                .count(),
            1
        );
        worker.gesture = Some(Gesture {
            x: 10.0,
            y: 10.0,
            threshold: 4.0,
            draggable: true,
            head: true,
            started: false,
            release_observed: None,
        });
        worker.advance_gesture((40.0, 40.0), true);
        assert!(events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, InputEvent::DragStart)));
    }
    #[test]
    fn genuinely_missing_release_cancels_without_phantom_pet() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut worker = gesture_worker(events.clone());
        let now = Instant::now();
        worker.reconcile_gesture_release(now, false);
        worker.reconcile_gesture_release(now + Duration::from_millis(80), false);
        worker.packet(Packet::Mouse { flags: 2 });
        assert!(worker.gesture.is_none());
        assert!(!events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, InputEvent::PetClick)));
    }
    #[test]
    fn accessibility_edges_preserve_scan_codes_and_ignore_physical_hook_events() {
        assert_eq!(injected_key(0x1e, 65, 0, WM_KEYDOWN), None);
        assert_eq!(
            injected_key(0x1e, 65, 0x10, WM_KEYDOWN),
            Some(Packet::Key {
                scan: 0x1e,
                vk: 65,
                flags: 0
            })
        );
        assert_eq!(
            injected_key(0x1d, 0xa3, 0x11, WM_KEYUP),
            Some(Packet::Key {
                scan: 0x1d,
                vk: 0xa3,
                flags: 3
            })
        );
        let mut input = PressedInput::new();
        input.key(4, 65, true);
        assert!(!input.key(4, 65, true));
        assert_eq!(input.snapshot().keyboard_taps, 1);
        input.key(4, 65, false);
        assert!(!input.snapshot().keyboard_down);
    }
    #[test]
    fn scan_codes_distinguish_modifiers_numpad_and_layout() {
        assert_eq!(scan_to_hid(0x1d, 0), Some(0xe0));
        assert_eq!(scan_to_hid(0x1d, 2), Some(0xe4));
        assert_eq!(scan_to_hid(0x1c, 2), Some(0x58));
        assert_eq!(scan_to_hid(0x1c, 0), Some(0x28));
        assert_eq!(scan_to_hid(0x1e, 0), Some(4));
        assert_eq!(scan_to_hid(0x1d, 4), Some(0x48));
    }
    #[test]
    fn raw_packet_decoder_checks_lengths_and_keeps_release_bit() {
        let header = size_of::<RAWINPUTHEADER>();
        let mut bytes = vec![0u8; header + 16];
        bytes[..4].copy_from_slice(&1u32.to_le_bytes());
        let count = bytes.len() as u32;
        bytes[4..8].copy_from_slice(&count.to_le_bytes());
        bytes[header..header + 2].copy_from_slice(&0x1eu16.to_le_bytes());
        bytes[header + 2..header + 4].copy_from_slice(&1u16.to_le_bytes());
        bytes[header + 6..header + 8].copy_from_slice(&65u16.to_le_bytes());
        assert_eq!(
            decode(&bytes, header),
            Some(Packet::Key {
                scan: 0x1e,
                flags: 1,
                vk: 65
            })
        );
        assert!(decode(&bytes[..header + 7], header).is_none());
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&bytes, header).is_none());
    }
}
