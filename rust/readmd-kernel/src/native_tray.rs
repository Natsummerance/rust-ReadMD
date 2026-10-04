//! Windows notification area owned by the desktop event-loop thread.
//! The hidden top-level window also receives Explorer's TaskbarCreated broadcast.
#![cfg(all(windows, feature = "desktop"))]

use std::{ffi::c_void, mem::size_of, ptr::null_mut, sync::atomic::{AtomicBool, Ordering}};
type Handle = *mut c_void;
static WAKE: std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync>>> = std::sync::Mutex::new(None);
pub fn set_wake(callback: impl Fn() + Send + Sync + 'static) { *WAKE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(callback)); }
pub fn wake_reader() { if let Some(callback) = WAKE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() { callback(); } }
type Proc = unsafe extern "system" fn(Handle, u32, usize, isize) -> isize;
const CALLBACK: u32 = 0x8000 + 51;
const RETRY: usize = 1;
#[repr(C)] struct Class { style: u32, proc: Option<Proc>, cls_extra: i32, win_extra: i32, instance: Handle, icon: Handle, cursor: Handle, background: Handle, menu: *const u16, name: *const u16 }
#[repr(C)] struct Point { x: i32, y: i32 }
#[repr(C)] struct Guid { a: u32, b: u16, c: u16, d: [u8; 8] }
#[repr(C)] struct IconData { size: u32, hwnd: Handle, id: u32, flags: u32, callback: u32, icon: Handle, tip: [u16; 128], state: u32, mask: u32, info: [u16; 256], version: u32, title: [u16; 64], info_flags: u32, guid: Guid, balloon: Handle }
#[link(name = "user32")] extern "system" {
    fn RegisterClassW(class: *const Class) -> u16;
    fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: Handle, menu: Handle, instance: Handle, param: Handle) -> Handle;
    fn DefWindowProcW(hwnd: Handle, message: u32, w: usize, l: isize) -> isize;
    fn SetWindowLongPtrW(hwnd: Handle, index: i32, value: isize) -> isize;
    fn GetWindowLongPtrW(hwnd: Handle, index: i32) -> isize;
    fn LoadIconW(instance: Handle, name: *const u16) -> Handle;
    fn RegisterWindowMessageW(name: *const u16) -> u32;
    fn DestroyWindow(hwnd: Handle) -> i32;
    fn SetTimer(hwnd: Handle, id: usize, ms: u32, callback: Handle) -> usize;
    fn KillTimer(hwnd: Handle, id: usize) -> i32;
    fn CreatePopupMenu() -> Handle;
    fn AppendMenuW(menu: Handle, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(menu: Handle, flags: u32, x: i32, y: i32, reserved: i32, hwnd: Handle, rect: Handle) -> u32;
    fn DestroyMenu(menu: Handle) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn SetForegroundWindow(hwnd: Handle) -> i32;
    fn PostMessageW(hwnd: Handle, message: u32, w: usize, l: isize) -> i32;
}
#[link(name = "kernel32")] extern "system" { fn GetModuleHandleW(name: *const u16) -> Handle; }
#[link(name = "shell32")] extern "system" { fn Shell_NotifyIconW(message: u32, data: *const IconData) -> i32; }
fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }

#[derive(Debug, Clone, Copy)] pub enum Action { Show, Open, Quit }
struct Context { data: IconData, restart: u32, active: AtomicBool, callback: Box<dyn Fn(Action)>, labels: [Vec<u16>; 3] }
impl Context {
    unsafe fn add(&self) {
        let ok = Shell_NotifyIconW(0, &self.data) != 0;
        self.active.store(ok, Ordering::SeqCst);
        if ok { KillTimer(self.data.hwnd, RETRY); } else { SetTimer(self.data.hwnd, RETRY, 5000, null_mut()); }
    }
}
unsafe extern "system" fn procedure(hwnd: Handle, message: u32, w: usize, l: isize) -> isize {
    let pointer = GetWindowLongPtrW(hwnd, -21) as *mut Context;
    if let Some(ctx) = pointer.as_ref() {
        if message == ctx.restart && ctx.restart != 0 || message == 0x113 && w == RETRY { ctx.add(); return 0; }
        if message == CALLBACK {
            match l as u32 {
                0x202 | 0x203 => (ctx.callback)(Action::Show), // click / double click
                0x205 | 0x7b => {
                    let menu = CreatePopupMenu();
                    if !menu.is_null() {
                        for (i, label) in ctx.labels.iter().enumerate() { AppendMenuW(menu, 0, i + 1, label.as_ptr()); }
                        let mut point = Point { x: 0, y: 0 }; GetCursorPos(&mut point);
                        SetForegroundWindow(hwnd);
                        let selected = TrackPopupMenu(menu, 0x100 | 2, point.x, point.y, 0, hwnd, null_mut());
                        DestroyMenu(menu); PostMessageW(hwnd, 0, 0, 0);
                        match selected { 1 => (ctx.callback)(Action::Show), 2 => (ctx.callback)(Action::Open), 3 => (ctx.callback)(Action::Quit), _ => {} }
                    }
                }
                _ => {}
            }
            return 0;
        }
    }
    DefWindowProcW(hwnd, message, w, l)
}

pub struct Tray { hwnd: Handle, context: Box<Context> }
impl Tray {
    pub fn new(callback: impl Fn(Action) + 'static) -> Option<Self> {
        unsafe {
            let name = wide("ReadMD.Tray.Owner"); let instance = GetModuleHandleW(null_mut());
            let class = Class { style: 0, proc: Some(procedure), cls_extra: 0, win_extra: 0, instance, icon: null_mut(), cursor: null_mut(), background: null_mut(), menu: null_mut(), name: name.as_ptr() };
            RegisterClassW(&class);
            let hwnd = CreateWindowExW(0x80, name.as_ptr(), name.as_ptr(), 0, 0, 0, 0, 0, null_mut(), null_mut(), instance, null_mut());
            if hwnd.is_null() { return None; }
            let mut icon = LoadIconW(instance, 1usize as *const u16);
            if icon.is_null() { icon = LoadIconW(null_mut(), 32512usize as *const u16); }
            let mut data: IconData = std::mem::zeroed();
            data.size = size_of::<IconData>() as u32; data.hwnd = hwnd; data.id = 1;
            data.flags = 1 | 2 | 4; data.callback = CALLBACK; data.icon = icon;
            for (target, value) in data.tip.iter_mut().zip(wide("ReadMD")) { *target = value; }
            let mut context = Box::new(Context { data, restart: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), active: AtomicBool::new(false), callback: Box::new(callback), labels: [wide("打开 ReadMD"), wide("打开文件…"), wide("退出 ReadMD")] });
            SetWindowLongPtrW(hwnd, -21, &mut *context as *mut Context as isize);
            context.add();
            Some(Self { hwnd, context })
        }
    }
    pub fn available(&self) -> bool { self.context.active.load(Ordering::SeqCst) }
    pub fn set_labels(&mut self, labels: [&str; 3]) { self.context.labels = labels.map(wide); }
}
impl Drop for Tray {
    fn drop(&mut self) { unsafe { KillTimer(self.hwnd, RETRY); Shell_NotifyIconW(2, &self.context.data); SetWindowLongPtrW(self.hwnd, -21, 0); DestroyWindow(self.hwnd); } }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn native_layout_matches_windows_x64() { if size_of::<Handle>() == 8 { assert_eq!(size_of::<IconData>(), 976); assert_eq!(size_of::<Class>(), 72); } }
}
