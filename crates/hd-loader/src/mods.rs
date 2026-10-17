//! Discovering, loading and dispatching to mod DLLs in `mods/`.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use hd_sdk::abi::{EntryFn, HostApi, ModCallbacks, ABI_VERSION, ENTRY_SYMBOL};
use windows_sys::Win32::Foundation::FreeLibrary;
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH,
};

use crate::config::Config;
use crate::log::{log_error, log_info};

pub struct LoadedMod {
    pub name: String,
    pub callbacks: ModCallbacks,
}

static MODS: OnceLock<Vec<LoadedMod>> = OnceLock::new();

/// `.dll` files in `dir` that should be loaded, sorted by name.
pub fn discover(dir: &Path, cfg: &Config) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll")))
        .filter(|p| {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            if stem.starts_with('_') {
                return false;
            }
            if cfg.disabled.contains(&stem) {
                log_info!("skipping disabled mod {stem}");
                return false;
            }
            true
        })
        .collect();
    out.sort();
    out
}

pub fn load_all(paths: &[PathBuf], host: &'static HostApi) {
    let mut loaded = Vec::new();
    for path in paths {
        match unsafe { load_one(path, host) } {
            Ok(m) => loaded.push(m),
            Err(e) => log_error!("{}: {e}", path.display()),
        }
    }
    log_info!("{} mod(s) loaded", loaded.len());
    let _ = MODS.set(loaded);
}

unsafe fn load_one(path: &Path, host: &'static HostApi) -> Result<LoadedMod, String> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let module = LoadLibraryExW(wide.as_ptr(), core::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH);
    if module.is_null() {
        return Err(format!("LoadLibrary failed ({})", std::io::Error::last_os_error()));
    }
    let Some(entry) = GetProcAddress(module, ENTRY_SYMBOL.as_ptr()) else {
        FreeLibrary(module);
        return Err("not a mod (no hdmod_entry export)".into());
    };
    let entry: EntryFn = core::mem::transmute(entry);
    let desc = entry(host);
    // A mod that initialised may have installed hooks; never unload it.
    let Some(desc) = desc.as_ref() else {
        return Err("hdmod_entry failed (ABI mismatch or init error, see log)".into());
    };
    if desc.abi_version != ABI_VERSION {
        return Err(format!("built for ABI v{}, loader is v{ABI_VERSION}", desc.abi_version));
    }
    let name = desc.name.as_str().to_owned();
    log_info!("loaded {name} {} {}", desc.version.as_str(), {
        let a = desc.author.as_str();
        if a.is_empty() { String::new() } else { format!("by {a}") }
    });
    Ok(LoadedMod { name, callbacks: desc.callbacks })
}

pub fn count() -> usize {
    MODS.get().map_or(0, Vec::len)
}

fn each(f: impl Fn(&LoadedMod)) {
    if let Some(mods) = MODS.get() {
        mods.iter().for_each(f);
    }
}

pub fn dispatch_tick(dt: f32) {
    each(|m| {
        if let Some(cb) = m.callbacks.on_tick {
            unsafe { cb(dt) }
        }
    });
}

pub fn dispatch_ui() {
    each(|m| {
        if let Some(cb) = m.callbacks.on_ui {
            unsafe { cb() }
        }
    });
}

pub fn dispatch_frame() {
    each(|m| {
        if let Some(cb) = m.callbacks.on_frame {
            unsafe { cb() }
        }
    });
}

pub fn dispatch_http_request(request: usize) {
    each(|m| {
        if let Some(cb) = m.callbacks.on_http_request {
            unsafe { cb(request as *mut core::ffi::c_void) }
        }
    });
}

pub fn dispatch_world_tick(world: usize, dt: f32) {
    each(|m| {
        if let Some(cb) = m.callbacks.on_world_tick {
            unsafe { cb(world as *mut core::ffi::c_void, dt) }
        }
    });
}

pub fn dispatch_world_tick_pre(world: usize, dt: f32) {
    each(|m| {
        if let Some(cb) = m.callbacks.on_world_tick_pre {
            unsafe { cb(world as *mut core::ffi::c_void, dt) }
        }
    });
}

pub fn dispatch_training_hud(begin: bool) {
    each(|m| {
        let cb = if begin { m.callbacks.on_training_hud_begin } else { m.callbacks.on_training_hud_end };
        if let Some(cb) = cb {
            unsafe { cb() }
        }
    });
}

pub fn dispatch_unload() {
    each(|m| {
        if let Some(cb) = m.callbacks.on_unload {
            log_info!("unloading {}", m.name);
            unsafe { cb() }
        }
    });
}
