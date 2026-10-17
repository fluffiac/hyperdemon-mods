//! The single MinHook instance shared by the loader and all mods.

use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

use minhook::MinHook;

/// Create and enable a hook. `original` receives the trampoline *before* the
/// hook goes live, so a detour can never observe it unset.
pub unsafe fn install(target: usize, detour: usize, original: Option<&AtomicUsize>) -> Result<usize, i32> {
    let tramp = MinHook::create_hook(target as *mut c_void, detour as *mut c_void).map_err(|e| e as i32)?;
    if let Some(slot) = original {
        slot.store(tramp as usize, Ordering::Release);
    }
    if let Err(e) = MinHook::enable_hook(target as *mut c_void) {
        let _ = MinHook::remove_hook(target as *mut c_void);
        return Err(e as i32);
    }
    Ok(tramp as usize)
}

pub unsafe extern "C" fn host_hook_create(
    target: *mut c_void,
    detour: *mut c_void,
    original: *mut *mut c_void,
) -> i32 {
    // Mods get the trampoline through `original`; write it before enabling.
    let tramp = match MinHook::create_hook(target, detour) {
        Ok(t) => t,
        Err(e) => return e as i32,
    };
    if !original.is_null() {
        core::ptr::write_volatile(original, tramp);
    }
    match MinHook::enable_hook(target) {
        Ok(()) => 0,
        Err(e) => {
            let _ = MinHook::remove_hook(target);
            e as i32
        }
    }
}

pub unsafe extern "C" fn host_hook_remove(target: *mut c_void) -> i32 {
    if let Err(e) = MinHook::disable_hook(target) {
        return e as i32;
    }
    match MinHook::remove_hook(target) {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
