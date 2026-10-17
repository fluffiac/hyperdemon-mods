//! Polling keyboard input. Only reports keys while the game window is focused.

#[link(name = "user32")]
extern "system" {
    fn GetAsyncKeyState(vk: i32) -> i16;
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcessId() -> u32;
}

/// Windows virtual-key codes for convenience.
pub mod vk {
    pub const F1: i32 = 0x70;
    pub const F2: i32 = 0x71;
    pub const F3: i32 = 0x72;
    pub const F4: i32 = 0x73;
    pub const F5: i32 = 0x74;
    pub const F6: i32 = 0x75;
    pub const F7: i32 = 0x76;
    pub const F8: i32 = 0x77;
    pub const F9: i32 = 0x78;
    pub const F10: i32 = 0x79;
    pub const F11: i32 = 0x7A;
    pub const F12: i32 = 0x7B;
}

fn game_focused() -> bool {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        pid == GetCurrentProcessId()
    }
}

/// Whether `vk` is held right now.
pub fn is_down(vk: i32) -> bool {
    game_focused() && unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0
}

/// Edge detector: `pressed()` is true once per key press.
#[derive(Default)]
pub struct KeyEdge {
    vk: i32,
    was_down: bool,
}

impl KeyEdge {
    pub const fn new(vk: i32) -> Self {
        Self { vk, was_down: false }
    }

    pub fn pressed(&mut self) -> bool {
        let down = is_down(self.vk);
        let edge = down && !self.was_down;
        self.was_down = down;
        edge
    }
}
