//! Leaderboard guard: while any mod is loaded, nothing a modded session
//! produces may reach the online leaderboards.
//!
//! Every write endpoint (`add_score`, `add_replay`, `add_replay_thumb`,
//! `add_clip*`) is detoured to a stub that sends nothing, and the end screen's
//! "Score not submitted (error)" line is rewritten to say why. Reading
//! leaderboards, local replays and everything else keep working.
//!
//! The guard fails closed: if any endpoint cannot be located (e.g. after a game
//! update), [`install`] errors and the loader refuses to load mods.

use core::sync::atomic::{AtomicUsize, Ordering};

use hd_sdk::mem::write;

use crate::hooks;
use crate::preserve::preserving_detour;
use crate::log::{log_error, log_info};
use crate::sigs::{Resolved, UPLOADS};

static GAME_PTR: AtomicUsize = AtomicUsize::new(0);
static SCORE_ERROR_TEXT: AtomicUsize = AtomicUsize::new(0);
static TEXTV_ORIGINAL: AtomicUsize = AtomicUsize::new(0);

const MODDED_TEXT: &core::ffi::CStr = c"Score not submitted (mods loaded)";

// Submit status on the Game object, read by the end screen (FUN_140090150).
const SUBMIT_STATE: usize = 0x14c;
const SUBMIT_MESSAGE_TIMER: usize = 0x150;

unsafe extern "system" fn blocked_add_score(_this: usize, _score: f32) {
    log_info!("blocked score upload (mods loaded)");
    let pp = GAME_PTR.load(Ordering::Relaxed);
    if pp != 0 {
        let game: usize = hd_sdk::mem::read(pp);
        if game != 0 {
            // Same fields add_score itself sets: state -1 = "not submitted",
            // timer = how long the end screen shows the line.
            write::<i32>(game + SUBMIT_STATE, -1);
            write::<f32>(game + SUBMIT_MESSAGE_TIMER, 4.0);
        }
    }
}

unsafe extern "system" fn blocked_upload(_this: usize, _a: usize, _b: usize, _c: usize) -> usize {
    log_info!("blocked replay/clip upload (mods loaded)");
    0
}

// TextV is called directly, so swap its format argument from a
// register-preserving entry (see preserve.rs).
unsafe extern "C" fn textv_handler(fmt: usize) -> usize {
    if fmt == SCORE_ERROR_TEXT.load(Ordering::Relaxed) {
        MODDED_TEXT.as_ptr() as usize
    } else {
        fmt
    }
}
preserving_detour!(textv_entry, textv_handler, TEXTV_ORIGINAL);

pub fn install(r: &Resolved) -> Result<(), String> {
    let missing: Vec<&str> =
        UPLOADS.iter().zip(&r.uploads).filter(|(_, a)| a.is_none()).map(|(s, _)| s.name).collect();
    if !missing.is_empty() {
        return Err(format!("upload endpoints not found: {}", missing.join(", ")));
    }
    GAME_PTR.store(r.addrs.game_ptr, Ordering::Relaxed);

    for (sig, addr) in UPLOADS.iter().zip(&r.uploads) {
        let detour = if sig.name == "add_score" { blocked_add_score as *const () as usize } else { blocked_upload as *const () as usize };
        unsafe { hooks::install(addr.unwrap(), detour, None) }
            .map_err(|e| format!("hooking {} failed (MinHook {e})", sig.name))?;
    }

    // Cosmetic; the upload block above does not depend on it.
    if let (Some(text), true) = (r.score_error_text, r.addrs.imgui_textv != 0) {
        SCORE_ERROR_TEXT.store(text, Ordering::Relaxed);
        if let Err(e) = unsafe { hooks::install(r.addrs.imgui_textv, textv_entry as *const () as usize, Some(&TEXTV_ORIGINAL)) } {
            log_error!("end-screen text hook failed (MinHook {e})");
        }
    }
    log_info!("leaderboard guard active: {} upload endpoints blocked", UPLOADS.len());
    Ok(())
}
