//! TRAINING scenarios: which one is running, and adding entries to the menu.
//!
//! A training scenario is a clip (`.hdclip`, replay format) whose objective
//! logic is keyed by clip id. The TRAINING menu lists clip list A of the scene
//! object (`*(Game+0x68)`), filled from `get_clips.php` while the clip category
//! is TRAINING (`0x1b`). Clicking a row downloads
//! `http://b.hyprd.mn/hd/c/<id>.hdclip`, and once the clip has loaded the game
//! sets `Game+0x370 = id`. See `docs/REVERSE_ENGINEERING.md`.

use crate::game::Game;
use crate::mem::{read, write};
use crate::msvc;

/// Clip category id of the TRAINING menu (`scene+0x1e0`).
pub const CATEGORY_TRAINING: i32 = 0x1b;
/// Base URL the game downloads clips from (`<id>` appended in decimal).
pub const CLIP_URL_PREFIX: &str = "http://b.hyprd.mn/hd/c/";

// Game fields
const SCENE: usize = 0x68;
const WATCHING: usize = 0x365;
const ACTIVE_CLIP: usize = 0x370;
const PLAYER_TOOK_OVER: usize = 0x378;
// Scene fields (clip list A)
const LIST_BEGIN: usize = 0x180;
const LIST_END: usize = 0x188;
const LIST_TOTAL: usize = 0x1c8;
const CATEGORY: usize = 0x1e0;
// 0x50-byte entry
const ENTRY: usize = 0x50;
const E_ID: usize = 0x00;
const E_VALID: usize = 0x04;
const E_DURATION: usize = 0x0c;
const E_TITLE: usize = 0x10;
const E_SUBTITLE: usize = 0x30;
/// Rows per page in the TRAINING menu.
const PAGE: i32 = 3;

/// Id of the clip/training currently loaded, if any.
pub fn active_clip() -> Option<u32> {
    let g = Game::get()?;
    let id: i64 = unsafe { read(g.as_ptr() + ACTIVE_CLIP) };
    (id >= 0).then_some(id as u32)
}

/// True once the player has taken control of the loaded clip (the condition
/// the built-in trainings use before running their objective logic).
pub fn player_in_control() -> bool {
    let Some(g) = Game::get() else { return false };
    unsafe { read::<u8>(g.as_ptr() + WATCHING) == 0 && read::<u8>(g.as_ptr() + PLAYER_TOOK_OVER) != 0 }
}

// Menu scene (*(Game+0x70)) -> +0x18 -> +0xb8 is the World, which doubles
// as the training progress object.
const MENU: usize = 0x70;
const MENU_SUB: usize = 0x18;
const SUB_WORLD: usize = 0xb8;
const WORLD_STAGE: usize = 0x84;
const WORLD_COMPLETED: usize = 0x88;

/// The World as the training UI sees it (`*(*(*(Game+0x70)+0x18)+0xb8)`).
pub fn progress_world() -> Option<usize> {
    let g = Game::get()?;
    unsafe {
        let menu: usize = read(g.as_ptr() + MENU);
        if menu == 0 {
            return None;
        }
        let sub: usize = read(menu + MENU_SUB);
        if sub == 0 {
            return None;
        }
        let w: usize = read(sub + SUB_WORLD);
        (w != 0).then_some(w)
    }
}

/// One key of the clip player's input display (`FUN_140092980`), drawn into
/// the foreground draw list while a training clip's demo plays.
#[doc(hidden)]
pub const INPUT_KEY_SIG: &str = "48 8B C4 4C 89 40 18 48 89 48 08 53";
/// The clip player's scrub bar (`FUN_1400a4e20`), drawn into the foreground
/// draw list; dragging it edits the MAKE CLIP / EXPORT range.
#[doc(hidden)]
pub const SCRUB_BAR_SIG: &str = "48 8B C4 55 41 54 41 55 41 56 41 57 48 8D A8 B8 FE FF FF";

static HIDE_OVERLAY: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static INPUT_KEY_HOOK: crate::hook::Hook = crate::hook::Hook::new();
static SCRUB_BAR_HOOK: crate::hook::Hook = crate::hook::Hook::new();

type InputKeyFn = unsafe extern "C" fn(usize, usize, u64, u64, u64);
type ScrubBarFn = unsafe extern "C" fn(usize);

unsafe extern "C" fn input_key_detour(a: usize, b: usize, c: u64, d: u64, e: u64) {
    if !HIDE_OVERLAY.load(core::sync::atomic::Ordering::Relaxed) {
        INPUT_KEY_HOOK.original::<InputKeyFn>()(a, b, c, d, e);
    }
}

unsafe extern "C" fn scrub_bar_detour(scene: usize) {
    if !HIDE_OVERLAY.load(core::sync::atomic::Ordering::Relaxed) {
        SCRUB_BAR_HOOK.original::<ScrubBarFn>()(scene);
    }
}

/// Hides the clip player's input display and scrub bar (both only called by
/// the clip UI, `FUN_140092da0`) while `hide` is set. The TRY IT / RETRY
/// buttons stay. Installs its hooks the first time `hide` is set.
pub fn hide_clip_overlay(hide: bool) {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    if !hide {
        HIDE_OVERLAY.store(false, core::sync::atomic::Ordering::Relaxed);
        return;
    }
    INSTALLED.call_once(|| {
        let install = |hook: &crate::hook::Hook, sig: &str, detour: *const ()| {
            match crate::mem::find_pattern(sig) {
                Some(target) => {
                    if let Err(e) = unsafe { hook.install(target, detour) } {
                        crate::log_warn!("clip overlay hook: {e}");
                    }
                }
                None => crate::log_warn!("clip overlay function not found; it stays visible"),
            }
        };
        install(&INPUT_KEY_HOOK, INPUT_KEY_SIG, input_key_detour as *const ());
        install(&SCRUB_BAR_HOOK, SCRUB_BAR_SIG, scrub_bar_detour as *const ());
    });
    HIDE_OVERLAY.store(true, core::sync::atomic::Ordering::Relaxed);
}

/// Keeps the game from auto-completing the active training.
///
/// The training UI (`FUN_140090150`) advances a stage counter past every hint
/// line whose progress has reached its target. Built-in trainings set targets
/// per clip id; for any other id all targets are 0, so all six steps pass in
/// the first frame after TRY IT and the game ends the run ("complete"),
/// freeing the cursor. The completion is guarded by the World's `completed`
/// flag (`+0x88`); holding it at 1 keeps the run going. Call every tick while
/// a custom training is active.
pub fn suppress_auto_complete() {
    if let Some(w) = progress_world() {
        unsafe { write::<i32>(w + WORLD_COMPLETED, 1) };
    }
}

/// The training UI's step counter (number of hint steps passed).
pub fn stage() -> Option<i32> {
    progress_world().map(|w| unsafe { read::<i32>(w + WORLD_STAGE) })
}

/// Id the game's hardcoded training logic has no case for. While it is in
/// `Game+0x370`, `FUN_140090150` and the World tick fall through to their
/// generic path: no title, no hints, no spawns, every step passes at once.
pub const NEUTRAL_CLIP: i64 = 0;

/// Replaces the active clip id (`Game+0x370`) and returns the previous one.
/// Use around the game's per-training code to hide which training is running
/// (see [`NEUTRAL_CLIP`]); always restore it right after.
pub fn swap_active_clip(id: i64) -> Option<i64> {
    let g = Game::get()?;
    unsafe {
        let old: i64 = read(g.as_ptr() + ACTIVE_CLIP);
        write(g.as_ptr() + ACTIVE_CLIP, id);
        Some(old)
    }
}

/// `World` vslot `0x118` (`FUN_1400dd4f0`, takes only the World). The score
/// training calls it every UI frame after drawing the score; also called in
/// normal runs behind HUD toggles. Not reversed.
pub fn world_hud() {
    let Some(w) = progress_world() else { return };
    unsafe {
        let f: unsafe extern "C" fn(usize) = core::mem::transmute(read::<usize>(read::<usize>(w) + 0x118));
        f(w);
    }
}

/// Lets the game complete the active training: clears the World's
/// `completed` flag, so the game's own check (stage > 5, which is always true
/// for ids it has no objectives for) ends the run, records the clip id as
/// completed in the menu and saves. Stop calling
/// [`suppress_auto_complete`] first.
pub fn allow_completion() {
    if let Some(w) = progress_world() {
        unsafe { write::<i32>(w + WORLD_COMPLETED, 0) };
    }
}

/// `World+0x30`: pulse scale the score display is drawn with.
pub fn score_pulse() -> f32 {
    progress_world().map_or(1.0, |w| unsafe { read::<f32>(w + 0x30) })
}

/// Progress counters the game increments during play, one per built-in
/// training step (`World+0x90..=0x124`, i32). Names come from the hint line
/// that displays them; what exactly each one counts is inferred from that
/// text. Built-in trainings zero the counter of step `k` while the player has
/// not reached step `k` yet.
pub const COUNTERS: &[(&str, usize)] = &[
    ("bunny_hop", 0x90),
    ("fake", 0x94),
    ("dodge", 0x98),
    ("dagger_jump", 0x9c),
    ("air_dagger_jump", 0xa0),
    ("stomp", 0xa4),
    ("dash", 0xa8),
    ("slide", 0xac),
    ("slide_jump", 0xb0),
    ("rapid_fire", 0xb4),
    ("shot", 0xb8),
    ("gems", 0xbc),
    ("laser_charge", 0xc0),
    ("laser", 0xc4),
    ("laser_lock_on", 0xc8),
    ("grab", 0xcc),
    ("drop", 0xd0),
    ("throw", 0xd4),
    ("bomb1_lasered", 0xd8),
    ("bomb2_lasered", 0xdc),
    ("bluepowerup", 0xe0),
    ("bluepowerup_laser", 0xe4),
    ("bluepowerup_air", 0xe8),
    ("spawner_dagger_kill", 0xec),
    ("spawner_gem_laser", 0xf0),
    ("spawner_gem_dash", 0xf4),
    ("scuttles_stun", 0xf8),
    ("scuttles_stunned", 0xfc),
    ("scuttles_attack", 0x100),
    ("scuttles_boost", 0x104),
    ("snakes_attention", 0x108),
    ("snakes_spin", 0x10c),
    ("snakes_head_hit", 0x110),
    ("snakes_kill", 0x114),
    ("snakes_stomp_stun", 0x118),
    ("spiders_hide", 0x11c),
    ("spiders_drop", 0x120),
    ("spiders_dropped_damage", 0x124),
];

/// World offset of the counter called `name` in [`COUNTERS`].
pub fn counter_offset(name: &str) -> Option<usize> {
    COUNTERS.iter().find(|(n, _)| *n == name).map(|&(_, o)| o)
}

pub fn read_counter(offset: usize) -> i32 {
    progress_world().map_or(0, |w| unsafe { read::<i32>(w + offset) })
}

pub fn write_counter(offset: usize, v: i32) {
    if let Some(w) = progress_world() {
        unsafe { write::<i32>(w + offset, v) };
    }
}

/// True while the clip demo plays (before TRY IT).
pub fn watching() -> bool {
    Game::get().is_some_and(|g| unsafe { read::<u8>(g.as_ptr() + WATCHING) != 0 })
}

/// True once TRY IT handed control to the player (stays set after the run
/// ends; see [`player_in_control`]).
pub fn took_over() -> bool {
    Game::get().is_some_and(|g| unsafe { read::<u8>(g.as_ptr() + PLAYER_TOOK_OVER) != 0 })
}

/// One custom row in the TRAINING menu.
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: u32,
    pub title: String,
    pub subtitle: String,
    /// Shown as the clip length.
    pub duration: u32,
}

/// Keeps a set of custom entries present in the TRAINING list. Call
/// [`Injector::update`] every frame (e.g. from `Mod::ui`); it only touches the
/// list while the TRAINING category is loaded and repairs it whenever the
/// game refreshes the list from the server.
#[derive(Default)]
pub struct Injector {
    server_total: i32,
    last_written: i32,
    waited_frames: u32,
}

impl Injector {
    pub fn update(&mut self, entries: &[Entry]) {
        let a = crate::addrs();
        if entries.is_empty() || a.clip_list_resize == 0 || a.string_assign == 0 {
            return;
        }
        let Some(g) = Game::get() else { return };
        unsafe {
            let scene: usize = read(g.as_ptr() + SCENE);
            if scene == 0 || read::<i32>(scene + CATEGORY) != CATEGORY_TRAINING {
                self.waited_frames = 0;
                return;
            }
            let total: i32 = read(scene + LIST_TOTAL);
            if total != self.last_written {
                self.server_total = total;
            }
            // Wait for the server's page so ours land after it; fall back to
            // injecting anyway (offline) after ~3 s.
            if self.server_total == 0 && self.waited_frames < 180 {
                self.waited_frames += 1;
                return;
            }
            // Start on a fresh page: the game decides whether a page needs
            // fetching by looking at its last row, so never share a page with
            // server rows.
            let base = (self.server_total + PAGE - 1) / PAGE * PAGE;
            let desired = base + entries.len() as i32;

            let vec = scene + LIST_BEGIN;
            let len = (read::<usize>(scene + LIST_END) - read::<usize>(vec)) / ENTRY;
            if len < desired as usize {
                type ResizeFn = unsafe extern "C" fn(vec: usize, n: usize, same: usize);
                core::mem::transmute::<usize, ResizeFn>(a.clip_list_resize)(vec, desired as usize, vec);
            }
            let begin: usize = read(vec);
            for (i, e) in entries.iter().enumerate() {
                let row = begin + (base as usize + i) * ENTRY;
                if read::<u32>(row + E_ID) != e.id || read::<i32>(row + E_VALID) <= 0 {
                    write(row + E_ID, e.id);
                    write(row + E_VALID, 1i32);
                    write(row + 0x08, 0u32);
                }
                write(row + E_DURATION, e.duration);
                if msvc::read_string(row + E_TITLE) != e.title {
                    msvc::assign_string(row + E_TITLE, &e.title);
                }
                if msvc::read_string(row + E_SUBTITLE) != e.subtitle {
                    msvc::assign_string(row + E_SUBTITLE, &e.subtitle);
                }
            }
            write(scene + LIST_TOTAL, desired);
            self.last_written = desired;
        }
    }
}
