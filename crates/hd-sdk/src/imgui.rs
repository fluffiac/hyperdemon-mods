//! Minimal bindings to the Dear ImGui build linked into the game (≈1.7x–1.8x;
//! no version string is embedded). Calls go straight to the game's own ImGui
//! functions, so mod windows share its font, style and HUD frame.
//!
//! Only valid inside [`crate::Mod::ui`]. The game does not feed mouse input to
//! its HUD context, so stick to display widgets (text, windows).

use std::ffi::CString;

use crate::mem::{read, write};

/// `ImGuiWindowFlags` values (verified against the game's `ImGui::Begin`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowFlags(pub u32);

impl WindowFlags {
    pub const NONE: Self = Self(0);
    pub const NO_TITLE_BAR: Self = Self(1 << 0);
    pub const NO_RESIZE: Self = Self(1 << 1);
    pub const NO_MOVE: Self = Self(1 << 2);
    pub const NO_SCROLLBAR: Self = Self(1 << 3);
    pub const NO_COLLAPSE: Self = Self(1 << 5);
    pub const ALWAYS_AUTO_RESIZE: Self = Self(1 << 6);
    pub const NO_BACKGROUND: Self = Self(1 << 7);
    pub const NO_SAVED_SETTINGS: Self = Self(1 << 8);
    pub const NO_MOUSE_INPUTS: Self = Self(1 << 9);
    pub const NO_FOCUS_ON_APPEARING: Self = Self(1 << 12);
    /// Also puts a new window behind all others (`CreateNewWindow`,
    /// `FUN_1401d7810`, inserts it at the front of `g.Windows`).
    pub const NO_BRING_TO_FRONT_ON_FOCUS: Self = Self(1 << 13);
    pub const NO_NAV_INPUTS: Self = Self(1 << 18);
    pub const NO_NAV_FOCUS: Self = Self(1 << 19);
    pub const NO_INPUTS: Self = Self(Self::NO_MOUSE_INPUTS.0 | Self::NO_NAV_INPUTS.0 | Self::NO_NAV_FOCUS.0);
    /// Borderless, click-through, auto-sized: a HUD overlay.
    pub const OVERLAY: Self = Self(
        Self::NO_TITLE_BAR.0
            | Self::ALWAYS_AUTO_RESIZE.0
            | Self::NO_SAVED_SETTINGS.0
            | Self::NO_FOCUS_ON_APPEARING.0
            | Self::NO_INPUTS.0,
    );
}

impl core::ops::BitOr for WindowFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

// Offsets into ImGuiContext, taken from code the game inlined
// (`FUN_140090150` writes NextWindowData directly before `Begin`).
const IO_DISPLAY_SIZE: usize = 0x10;
const NEXT_WINDOW_FLAGS: usize = 0x1b30;
const NEXT_WINDOW_POS_COND: usize = 0x1b34;
const NEXT_WINDOW_SIZE_COND: usize = 0x1b38;
const NEXT_WINDOW_POS: usize = 0x1b40;
const NEXT_WINDOW_POS_PIVOT: usize = 0x1b48;
const NEXT_WINDOW_SIZE: usize = 0x1b50;
const NEXT_WINDOW_BG_ALPHA: usize = 0x1b90;
/// `g.CurrentWindow` (`RenderFrame`, `FUN_1401d3070`).
const CURRENT_WINDOW: usize = 0x1a88;
/// `ImGuiWindow::DrawList`.
const WINDOW_DRAW_LIST: usize = 0x2b8;
const HAS_POS: u32 = 1;
const HAS_SIZE: u32 = 2;
const HAS_BG_ALPHA: u32 = 0x40;
/// `ImGuiCond_Always`
const COND_ALWAYS: i32 = 1;

type BeginFn = unsafe extern "C" fn(name: *const i8, p_open: *mut bool, flags: u32) -> bool;
type EndFn = unsafe extern "C" fn();
type TextVFn = unsafe extern "C" fn(fmt: *const i8, args: *const usize);
type SameLineFn = unsafe extern "C" fn(offset_from_start_x: f32, spacing: f32);
type AddRectFilledFn =
    unsafe extern "C" fn(draw_list: usize, min: *const [f32; 2], max: *const [f32; 2], col: u32, rounding: f32, corners: i32);

/// `ImDrawList::AddRectFilled(min, max, col, rounding, rounding_corners)`
/// (`FUN_1401eec40`, called by `RenderFrame`).
#[doc(hidden)]
pub const ADD_RECT_FILLED_SIG: &str =
    "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 40 F3 0F 10 5C 24 70";

fn add_rect_filled_fn() -> Option<usize> {
    static F: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let f = *F.get_or_init(|| unsafe { (crate::host().find_pattern)(crate::abi::StrRef::of(ADD_RECT_FILLED_SIG)) });
    (f != 0).then_some(f)
}

/// Handle passed to [`crate::Mod::ui`].
pub struct Ui {
    _not_send: core::marker::PhantomData<*const ()>,
}

impl Ui {
    #[doc(hidden)]
    pub fn __new() -> Self {
        Self { _not_send: core::marker::PhantomData }
    }

    fn ctx(&self) -> usize {
        let pp = crate::addrs().imgui_ctx_ptr;
        if pp == 0 {
            0
        } else {
            unsafe { read(pp) }
        }
    }

    /// Whether the ImGui bindings resolved in this game build.
    pub fn available(&self) -> bool {
        let a = crate::addrs();
        self.ctx() != 0 && a.imgui_begin != 0 && a.imgui_end != 0 && a.imgui_textv != 0
    }

    /// Size of the current ImGui display in pixels.
    pub fn display_size(&self) -> [f32; 2] {
        let c = self.ctx();
        if c == 0 {
            return [0.0, 0.0];
        }
        unsafe { read(c + IO_DISPLAY_SIZE) }
    }

    /// Fills the screen with `color` (`0xAABBGGRR`) over the 3D view but
    /// behind every ImGui window, so the game's menus and buttons stay
    /// usable. Text from [`crate::hud`] goes to the foreground list and stays
    /// on top. Call every frame.
    pub fn fill_backdrop(&self, color: u32) {
        let Some(add_rect) = add_rect_filled_fn() else { return };
        // Larger than the screen so the window's inner clip rect (inset by
        // its padding) still covers every edge.
        const BLEED: f32 = 64.0;
        let [w, h] = self.display_size();
        let flags = WindowFlags::NO_TITLE_BAR
            | WindowFlags::NO_RESIZE
            | WindowFlags::NO_MOVE
            | WindowFlags::NO_SCROLLBAR
            | WindowFlags::NO_BACKGROUND
            | WindowFlags::NO_SAVED_SETTINGS
            | WindowFlags::NO_FOCUS_ON_APPEARING
            | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS
            | WindowFlags::NO_INPUTS;
        self.window("##hd_backdrop")
            .flags(flags)
            .position([-BLEED, -BLEED], [0.0, 0.0])
            .size([w + 2.0 * BLEED, h + 2.0 * BLEED])
            .show(|ui| unsafe {
                let window: usize = read(ui.ctx() + CURRENT_WINDOW);
                if window == 0 {
                    return;
                }
                let dl: usize = read(window + WINDOW_DRAW_LIST);
                let f: AddRectFilledFn = core::mem::transmute(add_rect);
                f(dl, &[-BLEED, -BLEED], &[w + BLEED, h + BLEED], color, 0.0, 0);
            });
    }

    pub fn window<'a>(&'a self, name: &str) -> Window<'a> {
        Window { ui: self, name: name.to_owned(), flags: WindowFlags::NONE, pos: None, size: None, bg_alpha: None }
    }

    pub fn text(&self, s: &str) {
        let f = crate::addrs().imgui_textv;
        if f == 0 {
            return;
        }
        let Ok(c) = CString::new(s.replace('\0', "")) else { return };
        // Win64 va_list is a pointer to the spilled argument array.
        let args = [c.as_ptr() as usize];
        unsafe { core::mem::transmute::<usize, TextVFn>(f)(c"%s".as_ptr(), args.as_ptr()) }
    }

    pub fn same_line(&self) {
        let f = crate::addrs().imgui_same_line;
        if f != 0 {
            unsafe { core::mem::transmute::<usize, SameLineFn>(f)(0.0, -1.0) }
        }
    }
}

pub struct Window<'a> {
    ui: &'a Ui,
    name: String,
    flags: WindowFlags,
    pos: Option<([f32; 2], [f32; 2])>,
    size: Option<[f32; 2]>,
    bg_alpha: Option<f32>,
}

impl Window<'_> {
    pub fn flags(mut self, flags: WindowFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Place the window at `pos` (pixels), anchored by `pivot` (0..1 per axis).
    pub fn position(mut self, pos: [f32; 2], pivot: [f32; 2]) -> Self {
        self.pos = Some((pos, pivot));
        self
    }

    pub fn size(mut self, size: [f32; 2]) -> Self {
        self.size = Some(size);
        self
    }

    pub fn bg_alpha(mut self, alpha: f32) -> Self {
        self.bg_alpha = Some(alpha);
        self
    }

    /// Begin the window, run `f` if it is visible, end it.
    pub fn show(self, f: impl FnOnce(&Ui)) {
        let a = crate::addrs();
        let ctx = self.ui.ctx();
        if !self.ui.available() {
            return;
        }
        let Ok(name) = CString::new(self.name.replace('\0', "")) else { return };
        unsafe {
            let mut nf: u32 = read(ctx + NEXT_WINDOW_FLAGS);
            if let Some((pos, pivot)) = self.pos {
                write(ctx + NEXT_WINDOW_POS, pos);
                write(ctx + NEXT_WINDOW_POS_PIVOT, pivot);
                write(ctx + NEXT_WINDOW_POS_COND, COND_ALWAYS);
                nf |= HAS_POS;
            }
            if let Some(size) = self.size {
                write(ctx + NEXT_WINDOW_SIZE, size);
                write(ctx + NEXT_WINDOW_SIZE_COND, COND_ALWAYS);
                nf |= HAS_SIZE;
            }
            if let Some(alpha) = self.bg_alpha {
                write(ctx + NEXT_WINDOW_BG_ALPHA, alpha);
                nf |= HAS_BG_ALPHA;
            }
            write(ctx + NEXT_WINDOW_FLAGS, nf);

            let begin: BeginFn = core::mem::transmute(a.imgui_begin);
            let end: EndFn = core::mem::transmute(a.imgui_end);
            if begin(name.as_ptr(), core::ptr::null_mut(), self.flags.0) {
                f(self.ui);
            }
            // ImGui requires End() even when Begin() returned false.
            end();
        }
    }
}
