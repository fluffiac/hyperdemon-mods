//! Engine hooks that drive mod callbacks.
//!
//! * tick  — detour of the scene-tick delegate thunk (main thread, fixed step)
//! * ui    — detour of the scene-UI delegate thunk (main thread, inside the HUD
//!   ImGui frame)
//! * frame — `SwapBuffers` IAT entry of `hyperdemon.exe` (render thread)
//! * exit  — CRT `exit` IAT entry, to run `on_unload` before teardown
//! * http  — `HttpRequest::Submit` (main thread, before the curl worker sees it)
//! * world — `World::Tick` (main thread, before and after the world update)
//! * training hud — `FUN_140090150` (main thread, around the training panel)

use core::sync::atomic::{AtomicUsize, Ordering};

use hd_sdk::mem::read;

use crate::log::{log_info, log_warn};
use crate::pe::{patch_iat, Image};
use crate::preserve::{around_detour, preserving_detour};
use crate::sigs::Resolved;
use crate::{hooks, mods};

static TICK_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static UI_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static SWAP_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static EXIT_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static HTTP_SUBMIT_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static WORLD_TICK_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static TRAINING_HUD_ORIGINAL: AtomicUsize = AtomicUsize::new(0);

/// Offset of the `dt` float in the engine's delegate objects.
const DELEGATE_DT: usize = 0x38;

type DelegateThunk = unsafe extern "C" fn(delegate: usize);
type SwapBuffersFn = unsafe extern "system" fn(hdc: usize) -> i32;
type ExitFn = unsafe extern "C" fn(code: i32) -> !;

unsafe extern "C" fn tick_detour(delegate: usize) {
    let dt: f32 = read(delegate + DELEGATE_DT);
    let original: DelegateThunk = core::mem::transmute(TICK_ORIGINAL.load(Ordering::Acquire));
    original(delegate);
    mods::dispatch_tick(dt);
}

unsafe extern "C" fn ui_detour(delegate: usize) {
    let original: DelegateThunk = core::mem::transmute(UI_ORIGINAL.load(Ordering::Acquire));
    original(delegate);
    mods::dispatch_ui();
}

unsafe extern "system" fn swap_buffers_detour(hdc: usize) -> i32 {
    mods::dispatch_frame();
    let original: SwapBuffersFn = core::mem::transmute(SWAP_ORIGINAL.load(Ordering::Acquire));
    original(hdc)
}

type WorldTickFn = unsafe extern "C" fn(world: usize, dt: f32) -> u64;

// Submit is called directly and its callers keep live values in r10/r11 (see
// preserve.rs), so it gets a register-preserving entry.
unsafe extern "C" fn http_submit_handler(request: usize) -> usize {
    mods::dispatch_http_request(request);
    request
}
preserving_detour!(http_submit_entry, http_submit_handler, HTTP_SUBMIT_ORIGINAL);

unsafe extern "C" fn world_tick_detour(world: usize, dt: f32) -> u64 {
    mods::dispatch_world_tick_pre(world, dt);
    let original: WorldTickFn = core::mem::transmute(WORLD_TICK_ORIGINAL.load(Ordering::Acquire));
    let r = original(world, dt);
    mods::dispatch_world_tick(world, dt);
    r
}

// The training panel is called directly from the scene UI (LTCG), so it
// gets a register-transparent wrapper.
unsafe extern "C" fn training_hud_begin() {
    mods::dispatch_training_hud(true);
}
unsafe extern "C" fn training_hud_end() {
    mods::dispatch_training_hud(false);
}
around_detour!(training_hud_entry, training_hud_begin, training_hud_end, TRAINING_HUD_ORIGINAL);

unsafe extern "C" fn exit_detour(code: i32) -> ! {
    mods::dispatch_unload();
    let original: ExitFn = core::mem::transmute(EXIT_ORIGINAL.load(Ordering::Acquire));
    original(code)
}

pub fn install(img: &Image, r: &Resolved) {
    unsafe {
        match r.scene_tick {
            Some(t) => match hooks::install(t, tick_detour as *const () as usize, Some(&TICK_ORIGINAL)) {
                Ok(_) => log_info!("tick hook installed"),
                Err(e) => log_warn!("tick hook failed (MinHook {e}); on_tick disabled"),
            },
            None => log_warn!("tick thunk not found; on_tick disabled"),
        }
        match r.scene_ui {
            Some(t) => match hooks::install(t, ui_detour as *const () as usize, Some(&UI_ORIGINAL)) {
                Ok(_) => log_info!("ui hook installed"),
                Err(e) => log_warn!("ui hook failed (MinHook {e}); on_ui disabled"),
            },
            None => log_warn!("ui thunk not found; on_ui disabled"),
        }
        inline_hook("http_submit", r.http_submit, http_submit_entry as *const () as usize, &HTTP_SUBMIT_ORIGINAL);
        inline_hook("world_tick", r.world_tick, world_tick_detour as *const () as usize, &WORLD_TICK_ORIGINAL);
        inline_hook("training_hud", r.training_hud, training_hud_entry as *const () as usize, &TRAINING_HUD_ORIGINAL);
        iat_hook(img, "GDI32.dll", "SwapBuffers", swap_buffers_detour as *const () as usize, &SWAP_ORIGINAL);
        iat_hook(img, "api-ms-win-crt-runtime-l1-1-0.dll", "exit", exit_detour as *const () as usize, &EXIT_ORIGINAL);
    }
}

unsafe fn inline_hook(name: &str, target: Option<usize>, detour: usize, original: &AtomicUsize) {
    match target {
        Some(t) => match hooks::install(t, detour, Some(original)) {
            Ok(_) => log_info!("{name} hook installed"),
            Err(e) => log_warn!("{name} hook failed (MinHook {e})"),
        },
        None => log_warn!("{name} not found; its callback is disabled"),
    }
}

unsafe fn iat_hook(img: &Image, dll: &str, func: &str, detour: usize, original: &AtomicUsize) {
    match img.iat_slot(dll, func) {
        Some(slot) => {
            original.store(*slot, Ordering::Release);
            patch_iat(slot, detour);
            log_info!("{func} hook installed");
        }
        None => log_warn!("{dll}!{func} import not found"),
    }
}
