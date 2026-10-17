//! Text drawn the way the game draws its own HUD text: straight into the
//! ImGui context's embedded draw list with the game's fonts, no window.
//!
//! [`training_panel`] reproduces the built-in training panel from
//! `FUN_140090150`: title in the large font centred at y = 20 with alpha
//! 0x55, hint lines in the 32 px body font centred at y = 80 + 32·i with
//! alpha 0x44 (0x66 for the current step). Only valid inside
//! [`crate::Mod::ui`].
//!
//! Fonts are one typeface loaded at several sizes (`Game+0x480..=0x4b0`).
//! The first one added, and so ImGui's default, is the 54 px title font, so
//! text must always push the font it wants rather than rely on the default.
//! Sizes are the fonts' own (`g.FontBaseSize`; `io.FontGlobalScale` is 1).
//! The HUD is 1279.5 x 720 ImGui units, scaled x2 at 2560 x 1440.

use crate::mem::read;

// ImGuiContext
const CTX_FONT: usize = 0x1920;
/// `g.FontBaseSize`: the current font's size without any window font scale.
const CTX_FONT_BASE_SIZE: usize = 0x192c;
const CTX_DRAW_LIST: usize = 0x1ec8;
// ImDrawList
const DL_SHARED_DATA: usize = 0x38;
const DL_CLIP_RECT: usize = 0x90;
// ImDrawListSharedData
const SD_FONT: usize = 0x08;
// Game
const GAME_TITLE_FONT: usize = 0x4a0;
/// 32 px body font (same font as `+0x480`), used for training hint lines.
const GAME_BODY_FONT: usize = 0x498;
const GAME_SCREEN_W: usize = 0x170;
const GAME_UI_SCALE: usize = 0x190;

pub const TITLE_COLOR: u32 = 0x55ff_ffff;
pub const HINT_COLOR: u32 = 0x44ff_ffff;
pub const HINT_CURRENT_COLOR: u32 = 0x66ff_ffff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    /// The 32 px body font (`Game+0x498`): training hint lines.
    Default,
    /// The 54 px title font (`Game+0x4a0`, at UI scale 1.5).
    Title,
}

type RenderTextFn = unsafe extern "C" fn(
    font: usize,
    draw_list: usize,
    size: f32,
    pos: u64,
    col: u32,
    clip: *const [f32; 4],
    begin: *const u8,
    end: *const u8,
    wrap_width: f32,
    cpu_fine_clip: u8,
);
type CalcTextSizeFn = unsafe extern "C" fn(
    font: usize,
    out: *mut [f32; 2],
    size: f32,
    max_width: f32,
    wrap_width: f32,
    begin: *const u8,
    end: *const u8,
    remaining: *mut *const u8,
);
type PushFontFn = unsafe extern "C" fn(font: usize);
type PopFontFn = unsafe extern "C" fn();

fn ctx() -> usize {
    let pp = crate::addrs().imgui_ctx_ptr;
    if pp == 0 {
        0
    } else {
        unsafe { read(pp) }
    }
}

fn available() -> bool {
    let a = crate::addrs();
    ctx() != 0
        && a.font_render_text != 0
        && a.font_calc_text_size != 0
        && a.imgui_push_font != 0
        && a.imgui_pop_font != 0
        && crate::game::Game::get().is_some()
}

/// Width of the HUD in ImGui units (`Game+0x170 * Game+0x190`).
pub fn display_width() -> f32 {
    let Some(g) = crate::game::Game::get() else { return 0.0 };
    unsafe { read::<i32>(g.as_ptr() + GAME_SCREEN_W) as f32 * read::<f32>(g.as_ptr() + GAME_UI_SCALE) }
}

/// Runs `f` with `font` current, like the game's PushFont/PopFont pairs.
fn with_font<R>(font: Font, f: impl FnOnce() -> R) -> R {
    let a = crate::addrs();
    // PushFont(NULL) (fonts not loaded yet) selects ImGui's default font.
    let field = match font {
        Font::Default => GAME_BODY_FONT,
        Font::Title => GAME_TITLE_FONT,
    };
    let font = crate::game::Game::get().map_or(0, |g| unsafe { read::<usize>(g.as_ptr() + field) });
    unsafe { core::mem::transmute::<usize, PushFontFn>(a.imgui_push_font)(font) };
    let r = f();
    unsafe { core::mem::transmute::<usize, PopFontFn>(a.imgui_pop_font)() };
    r
}

/// Text width in the current font, rounded like the game (`(int)(w + 0.95)`).
unsafe fn measure(text: &str) -> f32 {
    let c = ctx();
    let mut out = [0.0f32; 2];
    let f: CalcTextSizeFn = core::mem::transmute(crate::addrs().font_calc_text_size);
    f(
        read(c + CTX_FONT),
        &mut out,
        read(c + CTX_FONT_BASE_SIZE),
        f32::MAX,
        -1.0,
        text.as_ptr(),
        text.as_ptr().add(text.len()),
        core::ptr::null_mut(),
    );
    (out[0] + 0.95).trunc()
}

/// Draws `text` at `pos` in the current font at its own size.
unsafe fn render(text: &str, pos: [f32; 2], color: u32) {
    render_sized(text, pos, color, read(ctx() + CTX_FONT_BASE_SIZE));
}

/// Draws `text` at `pos` in the current font at `size`.
unsafe fn render_sized(text: &str, pos: [f32; 2], color: u32, size: f32) {
    let dl = ctx() + CTX_DRAW_LIST;
    let shared: usize = read(dl + DL_SHARED_DATA);
    let clip: [f32; 4] = read(dl + DL_CLIP_RECT);
    let pos = (pos[0].to_bits() as u64) | ((pos[1].to_bits() as u64) << 32);
    let f: RenderTextFn = core::mem::transmute(crate::addrs().font_render_text);
    f(
        read(shared + SD_FONT),
        dl,
        size,
        pos,
        color,
        &clip,
        text.as_ptr(),
        text.as_ptr().add(text.len()),
        0.0,
        0,
    );
}

/// Draws `text` horizontally centred on the HUD at height `y`; returns its
/// left edge.
pub fn draw_centered(text: &str, y: f32, color: u32, font: Font) -> f32 {
    if text.is_empty() || !available() {
        return display_width() * 0.5;
    }
    with_font(font, || unsafe {
        let x = display_width() * 0.5 - measure(text) * 0.5;
        render(text, [x, y], color);
        x
    })
}

/// Width of `text` in `font`, rounded like the game.
pub fn text_width(text: &str, font: Font) -> f32 {
    if text.is_empty() || !available() {
        return 0.0;
    }
    with_font(font, || unsafe { measure(text) })
}

/// Draws `text` at an absolute HUD position.
pub fn draw_at(text: &str, pos: [f32; 2], color: u32, font: Font) {
    if text.is_empty() || !available() {
        return;
    }
    with_font(font, || unsafe { render(text, pos, color) });
}

/// Logs the font state needed to match the game's HUD text sizes: display
/// size, scales, the context's current font, and every font the game loads
/// (`Game+0x480..=0x4b0`) with its native size and the width of a known
/// built-in hint line in it. Call once from [`crate::Mod::ui`].
pub fn log_diagnostics() {
    if !available() {
        return;
    }
    const SAMPLE: &str = "-Your score is constantly counting down.";
    let g = crate::game::Game::get().unwrap().as_ptr();
    let c = ctx();
    unsafe {
        let window: usize = read(c + 0x1a88);
        crate::log_info!(
            "hud diag: Game+0x170={} +0x190={} display_width={} io.DisplaySize={}x{} io.FontGlobalScale={} io.FontDefault={:#x} \
             g.Font={:#x} g.FontSize={} g.FontBaseSize={} window={:#x} window.FontWindowScale={}",
            read::<i32>(g + GAME_SCREEN_W),
            read::<f32>(g + GAME_UI_SCALE),
            display_width(),
            read::<f32>(c + 0x10),
            read::<f32>(c + 0x14),
            read::<f32>(c + 0xb0),
            read::<usize>(c + 0xb8),
            read::<usize>(c + CTX_FONT),
            read::<f32>(c + 0x1928),
            read::<f32>(c + CTX_FONT_BASE_SIZE),
            window,
            if window != 0 { read::<f32>(window + 0x2b0) } else { 0.0 },
        );
        let calc: CalcTextSizeFn = core::mem::transmute(crate::addrs().font_calc_text_size);
        for off in (0x480..=0x4b0).step_by(8) {
            let font: usize = read(g + off);
            if font == 0 {
                continue;
            }
            let size: f32 = read(font + 0x14);
            let mut out = [0.0f32; 2];
            calc(font, &mut out, size, f32::MAX, -1.0, SAMPLE.as_ptr(), SAMPLE.as_ptr().add(SAMPLE.len()), core::ptr::null_mut());
            crate::log_info!(
                "hud diag: Game+{off:#x} font={font:#x} FontSize={size} Scale={} sample_width={}",
                read::<f32>(font + 0x60),
                out[0]
            );
        }
    }
}

/// One hint line of the training panel.
pub struct HintRow<'a> {
    pub text: &'a str,
    /// `(progress, target)` for steps with an objective.
    pub progress: Option<(i32, i32)>,
}

/// The built-in training panel (`FUN_140090150`): the title, then rows 1..=5
/// at y = 80 + 32·(row-1). Row `stage` is the current step (brighter). Once
/// the player took over, every reached row with an objective gets a
/// `"%i/%i"` counter left of its text, white on the current row.
pub fn training_panel(title: &str, rows: &[HintRow], stage: i32, took_over: bool) {
    draw_centered(title, 20.0, TITLE_COLOR, Font::Title);
    for (i, row) in rows.iter().enumerate().take(5) {
        let n = i as i32 + 1;
        let y = 80.0 + i as f32 * 32.0;
        let color = if n == stage { HINT_CURRENT_COLOR } else { HINT_COLOR };
        let x = draw_centered(row.text, y, color, Font::Default);
        let Some((progress, target)) = row.progress else { continue };
        if n <= stage && took_over && target > 0 && !row.text.is_empty() {
            let counter = format!("{}/{}", progress.min(target), target);
            let color = if n == stage { 0xffff_ffff } else { 0x33ff_ffff };
            let w = text_width(&counter, Font::Default);
            draw_at(&counter, [x - w, y], color, Font::Default);
        }
    }
}

/// The big score of the score training (`FUN_140090150`): title font at
/// 36·`pulse`, the decimal point centred, y = 4, red once the score is
/// at or below 0. (The game also shakes it below -30.)
pub fn score_display(score: f32, pulse: f32) {
    if !available() {
        return;
    }
    let text = format!("{score:.2}");
    let int_part = &text[..text.find('.').unwrap_or(text.len())];
    let color = if score > 0.0 { 0xffff_ffff } else { 0xff00_19ff };
    with_font(Font::Title, || unsafe {
        let (w_int, w_all) = (measure(int_part), measure(&text));
        let x = display_width() * 0.5 - w_int - w_all * 0.5 * (pulse - 1.0);
        let y = 4.0 - (pulse - 1.0) * 10.0;
        render_sized(&text, [x, y], color, 36.0 * pulse);
    });
}
