//! Inline function hooks, backed by the loader's shared MinHook instance so
//! several mods can hook the same game function safely.
//!
//! ```ignore
//! use hd_sdk::hook::Hook;
//!
//! type AddScore = unsafe extern "system" fn(this: usize, score: f32);
//! static HOOK: Hook = Hook::new();
//!
//! unsafe extern "system" fn my_add_score(this: usize, score: f32) {
//!     hd_sdk::log_info!("score {score}");
//!     HOOK.original::<AddScore>()(this, score)
//! }
//!
//! unsafe { HOOK.install(target, my_add_score as *const ()) }.unwrap();
//! ```

use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookError(pub i32);

impl core::fmt::Display for HookError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "hook failed (MinHook status {})", self.0)
    }
}

impl std::error::Error for HookError {}

/// A hook slot, usable as a `static`.
pub struct Hook {
    target: AtomicUsize,
    original: AtomicUsize,
}

impl Hook {
    pub const fn new() -> Self {
        Self { target: AtomicUsize::new(0), original: AtomicUsize::new(0) }
    }

    /// Detour `target` to `detour`.
    ///
    /// # Safety
    /// `target` must be the start of a function whose calling convention and
    /// signature match `detour`.
    pub unsafe fn install(&self, target: usize, detour: *const ()) -> Result<(), HookError> {
        // The host writes the trampoline into our slot before enabling the
        // hook, so the detour can call `original()` from its first invocation.
        let slot = self.original.as_ptr() as *mut *mut c_void;
        let rc = (crate::host().hook_create)(target as *mut c_void, detour as *mut c_void, slot);
        if rc != 0 {
            self.original.store(0, Ordering::Release);
            return Err(HookError(rc));
        }
        self.target.store(target, Ordering::Release);
        Ok(())
    }

    pub fn is_installed(&self) -> bool {
        self.target.load(Ordering::Acquire) != 0
    }

    /// The trampoline to the original function, typed as `F` (a fn pointer).
    ///
    /// # Safety
    /// `F` must be the function pointer type the hook was installed with, and
    /// the hook must be installed.
    pub unsafe fn original<F: Copy>(&self) -> F {
        assert_eq!(core::mem::size_of::<F>(), core::mem::size_of::<usize>());
        let p = self.original.load(Ordering::Acquire);
        assert!(p != 0, "hook not installed");
        core::mem::transmute_copy(&p)
    }

    pub fn remove(&self) -> Result<(), HookError> {
        let t = self.target.swap(0, Ordering::AcqRel);
        if t == 0 {
            return Ok(());
        }
        let rc = unsafe { (crate::host().hook_remove)(t as *mut c_void) };
        if rc != 0 {
            Err(HookError(rc))
        } else {
            Ok(())
        }
    }
}

impl Default for Hook {
    fn default() -> Self {
        Self::new()
    }
}
