use std::time::Duration;

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput};
use raw_window_handle::HasWindowHandle;

pub fn supported() -> bool {
    cfg!(any(windows, all(unix, not(target_os = "macos"))))
}

pub fn window_flag() -> bool {
    std::env::args().any(|a| a == "--window")
}

pub fn starts_wallpaper() -> bool {
    supported() && !window_flag()
}

pub fn wallpaper_arg() -> Option<std::path::PathBuf> {
    std::env::args()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .map(|a| std::path::PathBuf::from(a.trim().trim_matches('"')))
}

pub fn prepare_env() {
    #[cfg(all(unix, not(target_os = "macos")))]
    if starts_wallpaper() {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    }
}

#[derive(Clone, Copy)]
struct Sample {
    pos: Pos2,
    left: bool,
    right: bool,
    over: bool,
}

fn mods(ctrl: bool, alt: bool, shift: bool) -> Modifiers {
    Modifiers { alt, ctrl, shift, mac_cmd: false, command: ctrl }
}

fn clip_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

fn key_events(out: &mut Vec<Event>, key: Option<Key>, text: Option<String>, down: bool, m: Modifiers) {
    if let Some(key) = key {
        if down && m.command {
            match key {
                Key::C => return out.push(Event::Copy),
                Key::X => return out.push(Event::Cut),
                Key::V => return out.extend(clip_text().map(Event::Paste)),
                _ => {}
            }
        }
        out.push(Event::Key { key, physical_key: Some(key), pressed: down, repeat: false, modifiers: m });
    }
    if down && !m.command && !m.alt {
        if let Some(t) = text.filter(|t| !t.chars().all(char::is_control)) {
            out.push(Event::Text(t));
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{key_events, mods, Sample};
    use eframe::egui::{self, pos2, vec2, Event, Key, Modifiers, MouseWheelUnit};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, Once, OnceLock};
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeEx, VK_CAPITAL, VK_CONTROL, VK_LBUTTON, VK_MENU, VK_RBUTTON,
        VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    static QUEUE: Mutex<Vec<Event>> = Mutex::new(Vec::new());
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static WANTS: AtomicBool = AtomicBool::new(false);
    static CTX: OnceLock<egui::Context> = OnceLock::new();
    static HOOKS: Once = Once::new();

    pub struct Sys {
        hwnd: HWND,
        style: isize,
        ex: isize,
        on: bool,
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn push(e: Event) {
        if let Ok(mut q) = QUEUE.lock() {
            q.push(e);
        }
        if let Some(c) = CTX.get() {
            c.request_repaint();
        }
    }

    unsafe fn desktop_class(h: HWND) -> bool {
        if h == 0 {
            return false;
        }
        let root = GetAncestor(h, GA_ROOT);
        let mut buf = [0u16; 32];
        let n = GetClassNameW(root, buf.as_mut_ptr(), 32).max(0) as usize;
        let name = String::from_utf16_lossy(&buf[..n]);
        name == "Progman" || name == "WorkerW"
    }

    unsafe fn over_desktop(pt: POINT) -> bool {
        desktop_class(WindowFromPoint(pt))
    }

    unsafe fn held(vk: u16) -> bool {
        GetAsyncKeyState(vk as i32) < 0
    }

    unsafe fn held_mods() -> Modifiers {
        mods(held(VK_CONTROL), held(VK_MENU), held(VK_SHIFT))
    }

    fn vk_key(vk: u32) -> Option<Key> {
        let name = match vk {
            0x30..=0x39 | 0x41..=0x5A => (vk as u8 as char).to_string(),
            0x08 => "Backspace".into(),
            0x09 => "Tab".into(),
            0x0D => "Enter".into(),
            0x1B => "Escape".into(),
            0x20 => "Space".into(),
            0x21 => "PageUp".into(),
            0x22 => "PageDown".into(),
            0x23 => "End".into(),
            0x24 => "Home".into(),
            0x25 => "ArrowLeft".into(),
            0x26 => "ArrowUp".into(),
            0x27 => "ArrowRight".into(),
            0x28 => "ArrowDown".into(),
            0x2D => "Insert".into(),
            0x2E => "Delete".into(),
            _ => return None,
        };
        Key::from_name(&name)
    }

    unsafe fn text_of(k: &KBDLLHOOKSTRUCT, m: &Modifiers) -> Option<String> {
        if m.ctrl && !m.alt {
            return None;
        }
        let mut st = [0u8; 256];
        if m.shift {
            st[VK_SHIFT as usize] = 0x80;
        }
        if m.ctrl {
            st[VK_CONTROL as usize] = 0x80;
        }
        if m.alt {
            st[VK_MENU as usize] = 0x80;
        }
        if GetKeyState(VK_CAPITAL as i32) & 1 != 0 {
            st[VK_CAPITAL as usize] = 1;
        }
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
        let mut buf = [0u16; 8];
        let n = ToUnicodeEx(k.vkCode, k.scanCode, st.as_ptr(), buf.as_mut_ptr(), 8, 4, GetKeyboardLayout(thread));
        (n > 0).then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }

    unsafe extern "system" fn kb(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code == 0 && ACTIVE.load(Ordering::Relaxed) && desktop_class(GetForegroundWindow()) {
            let k = &*(l as *const KBDLLHOOKSTRUCT);
            let down = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let m = held_mods();
            let key = vk_key(k.vkCode);
            let text = if down { text_of(k, &m) } else { None };
            let handled = text.is_some() || (key.is_some() && !m.ctrl && !m.alt);
            let mut out = Vec::new();
            key_events(&mut out, key, text, down, m);
            let any = !out.is_empty();
            for e in out {
                push(e);
            }
            if any && handled && WANTS.load(Ordering::Relaxed) {
                return 1;
            }
        }
        CallNextHookEx(0, code, w, l)
    }

    unsafe extern "system" fn ms(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        let wheel = w as u32 == WM_MOUSEWHEEL;
        if code == 0 && (wheel || w as u32 == WM_MOUSEHWHEEL) && ACTIVE.load(Ordering::Relaxed) {
            let s = &*(l as *const MSLLHOOKSTRUCT);
            if over_desktop(s.pt) {
                let d = (s.mouseData >> 16) as i16 as f32 / 120.0;
                let delta = if wheel { vec2(0.0, d) } else { vec2(d, 0.0) };
                push(Event::MouseWheel {
                    unit: MouseWheelUnit::Line,
                    delta,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: held_mods(),
                });
            }
        }
        CallNextHookEx(0, code, w, l)
    }

    unsafe extern "system" fn find(top: HWND, out: LPARAM) -> BOOL {
        let defview = wide("SHELLDLL_DefView");
        if FindWindowExW(top, 0, defview.as_ptr(), null()) != 0 {
            let worker = wide("WorkerW");
            *(out as *mut HWND) = FindWindowExW(0, top, worker.as_ptr(), null());
        }
        1
    }

    unsafe fn worker() -> HWND {
        let progman = FindWindowW(wide("Progman").as_ptr(), null());
        if progman == 0 {
            return 0;
        }
        let mut result = 0usize;
        SendMessageTimeoutW(progman, 0x052C, 0xD, 0x0, SMTO_NORMAL, 1000, &mut result);
        SendMessageTimeoutW(progman, 0x052C, 0xD, 0x1, SMTO_NORMAL, 1000, &mut result);
        let mut found: HWND = 0;
        EnumWindows(Some(find), &mut found as *mut HWND as LPARAM);
        if found == 0 {
            found = FindWindowExW(progman, 0, wide("WorkerW").as_ptr(), null());
        }
        found
    }

    impl Sys {
        pub fn new(cc: &eframe::CreationContext<'_>, _starts: bool) -> Option<Self> {
            let hwnd = match cc.window_handle().ok()?.as_raw() {
                RawWindowHandle::Win32(h) => h.hwnd.get(),
                _ => return None,
            };
            HOOKS.call_once(|| {
                std::thread::spawn(|| unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb), 0, 0);
                    SetWindowsHookExW(WH_MOUSE_LL, Some(ms), 0, 0);
                    let mut msg: MSG = std::mem::zeroed();
                    while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                });
            });
            unsafe {
                Some(Self {
                    hwnd,
                    style: GetWindowLongPtrW(hwnd, GWL_STYLE),
                    ex: GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
                    on: false,
                })
            }
        }

        pub fn set_wallpaper(&mut self, _ctx: &egui::Context, on: bool) {
            if on == self.on {
                return;
            }
            unsafe {
                let hwnd = self.hwnd;
                if on {
                    let parent = worker();
                    if parent == 0 {
                        return;
                    }
                    SetParent(hwnd, parent);
                    let strip = (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_POPUP) as isize;
                    SetWindowLongPtrW(hwnd, GWL_STYLE, (self.style & !strip) | WS_CHILD as isize | WS_VISIBLE as isize);
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (self.ex | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize));
                    self.fit(true);
                    SetWindowPos(hwnd, 0, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW);
                } else {
                    SetWindowLongPtrW(hwnd, GWL_STYLE, self.style | WS_VISIBLE as isize);
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, self.ex);
                    SetParent(hwnd, 0);
                    SetWindowPos(hwnd, 0, 120, 80, 900, 620, SWP_NOZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW);
                }
            }
            self.on = on;
        }

        pub fn gate(&mut self, ctx: &egui::Context, active: bool, wants: bool) {
            let _ = CTX.get_or_init(|| ctx.clone());
            ACTIVE.store(active && self.on, Ordering::Relaxed);
            WANTS.store(wants, Ordering::Relaxed);
        }

        pub fn fit(&mut self, force: bool) {
            unsafe {
                let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
                let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(self.hwnd, &mut r);
                if force || r.right != w || r.bottom != h {
                    let (vx, vy) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
                    MoveWindow(self.hwnd, -vx, -vy, w, h, 1);
                }
            }
        }

        pub fn sample(&mut self) -> Option<Sample> {
            unsafe {
                let mut pt = POINT { x: 0, y: 0 };
                if GetCursorPos(&mut pt) == 0 {
                    return None;
                }
                let over = over_desktop(pt);
                ScreenToClient(self.hwnd, &mut pt);
                Some(Sample {
                    pos: pos2(pt.x as f32, pt.y as f32),
                    left: held(VK_LBUTTON),
                    right: held(VK_RBUTTON),
                    over,
                })
            }
        }

        pub fn poll(&mut self, _wants: bool) -> Vec<Event> {
            QUEUE.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::{key_events, mods, Sample};
    use eframe::egui::{self, pos2, vec2, Event, Key, MouseWheelUnit};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::time::{Duration, Instant};
    use x11rb::connection::Connection;
    use x11rb::protocol::xinput::{self, ConnectionExt as _, XIEventMask};
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, PropMode};
    use x11rb::protocol::Event as X;
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    pub struct Sys {
        conn: RustConnection,
        root: u32,
        win: u32,
        chain: Vec<u32>,
        at: Instant,
        map: Vec<u32>,
        per: usize,
        min: u8,
        mask: u16,
        over: bool,
        kind: u32,
        desktop: u32,
        normal: u32,
        active: u32,
        on: bool,
    }

    fn atom(conn: &RustConnection, name: &[u8]) -> Option<u32> {
        Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom)
    }

    fn sym_key(sym: u32) -> Option<Key> {
        let name = match sym {
            0x30..=0x39 | 0x41..=0x5A => (sym as u8 as char).to_string(),
            0x61..=0x7A => (sym as u8 as char).to_ascii_uppercase().to_string(),
            0x20 => "Space".into(),
            0xff08 => "Backspace".into(),
            0xff09 => "Tab".into(),
            0xff0d | 0xff8d => "Enter".into(),
            0xff1b => "Escape".into(),
            0xffff => "Delete".into(),
            0xff50 => "Home".into(),
            0xff51 => "ArrowLeft".into(),
            0xff52 => "ArrowUp".into(),
            0xff53 => "ArrowRight".into(),
            0xff54 => "ArrowDown".into(),
            0xff55 => "PageUp".into(),
            0xff56 => "PageDown".into(),
            0xff57 => "End".into(),
            0xff63 => "Insert".into(),
            _ => return None,
        };
        Key::from_name(&name)
    }

    fn sym_text(sym: u32) -> Option<String> {
        match sym {
            0x20..=0x7e | 0xa0..=0xff => char::from_u32(sym),
            0x1000100..=0x110ffff => char::from_u32(sym - 0x1000000),
            _ => None,
        }
        .map(String::from)
    }

    impl Sys {
        pub fn new(cc: &eframe::CreationContext<'_>, starts: bool) -> Option<Self> {
            let win = match cc.window_handle().ok()?.as_raw() {
                RawWindowHandle::Xlib(h) => h.window as u32,
                RawWindowHandle::Xcb(h) => h.window.get(),
                _ => return None,
            };
            let (conn, screen) = x11rb::connect(None).ok()?;
            let setup = conn.setup();
            let root = setup.roots[screen].root;
            let (min, max) = (setup.min_keycode, setup.max_keycode);
            conn.xinput_xi_query_version(2, 0).ok()?.reply().ok()?;
            let events = XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE | XIEventMask::RAW_BUTTON_PRESS;
            conn.xinput_xi_select_events(root, &[xinput::EventMask { deviceid: 1, mask: vec![events] }]).ok()?;
            let keys = conn.get_keyboard_mapping(min, max - min + 1).ok()?.reply().ok()?;
            let sys = Self {
                kind: atom(&conn, b"_NET_WM_WINDOW_TYPE")?,
                desktop: atom(&conn, b"_NET_WM_WINDOW_TYPE_DESKTOP")?,
                normal: atom(&conn, b"_NET_WM_WINDOW_TYPE_NORMAL")?,
                active: atom(&conn, b"_NET_ACTIVE_WINDOW")?,
                per: keys.keysyms_per_keycode as usize,
                map: keys.keysyms,
                conn,
                root,
                win,
                chain: Vec::new(),
                at: Instant::now() - Duration::from_secs(10),
                min,
                mask: 0,
                over: false,
                on: starts,
            };
            let _ = sys.conn.flush();
            Some(sys)
        }

        pub fn set_wallpaper(&mut self, ctx: &egui::Context, on: bool) {
            if on == self.on {
                return;
            }
            self.on = on;
            let ty = if on { self.desktop } else { self.normal };
            let _ = self.conn.change_property32(PropMode::REPLACE, self.win, self.kind, AtomEnum::ATOM, &[ty]);
            let _ = self.conn.flush();
            let level = if on { egui::WindowLevel::AlwaysOnBottom } else { egui::WindowLevel::Normal };
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!on));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            if on {
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(0.0, 0.0)));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(800.0, 560.0)));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(900.0, 620.0)));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos2(120.0, 80.0)));
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }

        pub fn gate(&mut self, _ctx: &egui::Context, _active: bool, _wants: bool) {}

        pub fn fit(&mut self, _force: bool) {}

        fn refresh(&mut self) {
            self.chain.clear();
            self.at = Instant::now();
            let mut w = self.win;
            while w != 0 && w != self.root {
                self.chain.push(w);
                match self.conn.query_tree(w).ok().and_then(|c| c.reply().ok()) {
                    Some(t) => w = t.parent,
                    None => break,
                }
            }
        }

        pub fn sample(&mut self) -> Option<Sample> {
            if self.at.elapsed() > Duration::from_secs(2) {
                self.refresh();
            }
            let r = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
            self.mask = u16::from(r.mask);
            self.over = r.child == 0 || self.chain.contains(&r.child);
            Some(Sample {
                pos: pos2(r.root_x as f32, r.root_y as f32),
                left: self.mask & 0x100 != 0,
                right: self.mask & 0x400 != 0,
                over: self.over,
            })
        }

        fn focused(&self) -> bool {
            let reply = self
                .conn
                .get_property(false, self.root, self.active, AtomEnum::WINDOW, 0, 1)
                .ok()
                .and_then(|c| c.reply().ok());
            let id = reply.and_then(|r| r.value32().and_then(|mut v| v.next())).unwrap_or(0);
            id == 0 || self.chain.contains(&id)
        }

        fn press(&self, out: &mut Vec<Event>, code: u32, down: bool) {
            if code < self.min as u32 || !self.focused() {
                return;
            }
            let idx = (code - self.min as u32) as usize * self.per;
            let Some(&base) = self.map.get(idx) else { return };
            let alt = self.map.get(idx + 1).copied().unwrap_or(0);
            let letter = (0x61..=0x7a).contains(&base);
            let shift = self.mask & 1 != 0;
            let upper = shift ^ (self.mask & 2 != 0 && letter);
            let sym = if upper && alt != 0 { alt } else { base };
            let m = mods(self.mask & 4 != 0, self.mask & 8 != 0, shift);
            key_events(out, sym_key(base), sym_text(sym), down, m);
        }

        pub fn poll(&mut self, wants: bool) -> Vec<Event> {
            let mut out = Vec::new();
            let m = mods(self.mask & 4 != 0, self.mask & 8 != 0, self.mask & 1 != 0);
            while let Ok(Some(ev)) = self.conn.poll_for_event() {
                match ev {
                    X::XinputRawButtonPress(e) if self.over => {
                        let delta = match e.detail {
                            4 => vec2(0.0, 1.0),
                            5 => vec2(0.0, -1.0),
                            6 => vec2(1.0, 0.0),
                            7 => vec2(-1.0, 0.0),
                            _ => continue,
                        };
                        out.push(Event::MouseWheel {
                            unit: MouseWheelUnit::Line,
                            delta,
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Wheel,
                            modifiers: m,
                        });
                    }
                    X::XinputRawKeyPress(e) if wants => self.press(&mut out, e.detail, true),
                    X::XinputRawKeyRelease(e) if wants => self.press(&mut out, e.detail, false),
                    _ => {}
                }
            }
            out
        }
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
mod imp {
    use super::Sample;
    use eframe::egui::{self, Event};

    pub struct Sys;

    impl Sys {
        pub fn new(_cc: &eframe::CreationContext<'_>, _starts: bool) -> Option<Self> {
            None
        }
        pub fn set_wallpaper(&mut self, _ctx: &egui::Context, _on: bool) {}
        pub fn gate(&mut self, _ctx: &egui::Context, _active: bool, _wants: bool) {}
        pub fn fit(&mut self, _force: bool) {}
        pub fn sample(&mut self) -> Option<Sample> {
            None
        }
        pub fn poll(&mut self, _wants: bool) -> Vec<Event> {
            Vec::new()
        }
    }
}

pub struct Desktop {
    sys: imp::Sys,
    applied: Option<bool>,
    inside: bool,
    last: Option<Pos2>,
    down: [bool; 2],
    held: [bool; 2],
    fitted: f64,
}

fn edge(raw: &mut RawInput, pos: Pos2, button: PointerButton, now: bool, down: &mut bool, held: &mut bool, over: bool) {
    if now && !*down && over {
        raw.events.push(Event::PointerButton { pos, button, pressed: true, modifiers: Modifiers::NONE });
        *held = true;
    } else if !now && *held {
        raw.events.push(Event::PointerButton { pos, button, pressed: false, modifiers: Modifiers::NONE });
        *held = false;
    }
    *down = now;
}

impl Desktop {
    pub fn attach(cc: &eframe::CreationContext<'_>, starts_wallpaper: bool) -> Option<Self> {
        let _ = cc.window_handle().ok()?;
        let sys = imp::Sys::new(cc, starts_wallpaper)?;
        Some(Self {
            sys,
            applied: None,
            inside: false,
            last: None,
            down: [false; 2],
            held: [false; 2],
            fitted: f64::NEG_INFINITY,
        })
    }

    fn fit(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|i| i.time);
        if time - self.fitted < 1.0 {
            return;
        }
        self.fitted = time;
        self.sys.fit(false);
        #[cfg(not(windows))]
        {
            let (monitor, inner) = ctx.input(|i| (i.viewport().monitor_size, i.viewport().inner_rect));
            if let Some(m) = monitor {
                if inner.is_none_or(|r| (r.size() - m).abs().max_elem() > 1.0 || r.min.to_vec2().length() > 1.0) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(m));
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(Pos2::ZERO));
                }
            }
        }
    }

    fn pointer(&mut self, ctx: &egui::Context, raw: &mut RawInput, s: Sample) {
        let ppp = ctx.pixels_per_point();
        let pos = s.pos / ppp;
        let busy = s.left || s.right || self.last != Some(pos);
        ctx.request_repaint_after(Duration::from_millis(if busy { 16 } else { 50 }));
        if raw.events.iter().any(|e| matches!(e, Event::PointerMoved(_) | Event::PointerButton { .. })) {
            return;
        }
        if s.left && !self.down[0] && !s.over {
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        if s.over || self.held[0] || self.held[1] {
            if self.last != Some(pos) {
                raw.events.push(Event::PointerMoved(pos));
                self.last = Some(pos);
            }
            let (mut down, mut held) = (self.down, self.held);
            edge(raw, pos, PointerButton::Primary, s.left, &mut down[0], &mut held[0], s.over);
            edge(raw, pos, PointerButton::Secondary, s.right, &mut down[1], &mut held[1], s.over);
            self.down = down;
            self.held = held;
            self.inside = true;
        } else {
            self.down = [s.left, s.right];
            if self.inside {
                raw.events.push(Event::PointerGone);
                self.inside = false;
                self.last = None;
            }
        }
    }

    pub fn feed(&mut self, ctx: &egui::Context, raw: &mut RawInput, wallpaper: bool, interactive: bool) {
        if self.applied != Some(wallpaper) {
            self.sys.set_wallpaper(ctx, wallpaper);
            self.applied = Some(wallpaper);
            self.fitted = f64::NEG_INFINITY;
        }
        if !wallpaper {
            self.sys.gate(ctx, false, false);
            return;
        }
        self.fit(ctx);
        let wants = ctx.egui_wants_keyboard_input();
        self.sys.gate(ctx, interactive, wants);
        if !interactive {
            return;
        }
        if let Some(s) = self.sys.sample() {
            self.pointer(ctx, raw, s);
        }
        #[allow(unused_mut)]
        let mut events = self.sys.poll(wants);
        #[cfg(not(windows))]
        if ctx.input(|i| i.viewport().focused.unwrap_or(false)) {
            events.retain(|e| !matches!(e, Event::Key { .. } | Event::Text(_) | Event::Copy | Event::Cut | Event::Paste(_)));
        }
        raw.events.extend(events);
    }
}
