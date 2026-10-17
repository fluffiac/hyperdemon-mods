//! HyperDemon mod loader, deployed next to the game as `winmm.dll`.
//!
//! All winmm exports except `timeBeginPeriod` are PE forwarders to the system
//! DLL (see `build.rs`). The game calls `timeBeginPeriod(1)` exactly once, on
//! its main thread, right before entering the main loop (`FUN_1401c33c0`);
//! that call is where the loader initialises: outside the loader lock, after
//! the window and engine exist, and before any frame runs.

mod config;
mod crash;
mod events;
mod guard;
mod hooks;
mod log;
mod mods;
mod pe;
mod preserve;
mod sigs;

use std::path::PathBuf;
use std::sync::{Once, OnceLock};

use hd_sdk::abi::{HostApi, LogLevel, StrRef, ABI_VERSION};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

use crate::log::{log_error, log_info, log_warn};
use crate::pe::{parse_pattern, scan, Image};

/// PE timestamp of the build the signatures were made from.
const KNOWN_BUILD_TIMESTAMP: u32 = 0x66ECD8B5;

static INIT: Once = Once::new();
static HOST: OnceLock<HostApi> = OnceLock::new();
static IMAGE: OnceLock<Image> = OnceLock::new();

type TimeBeginPeriodFn = unsafe extern "system" fn(u32) -> u32;

/// Replacement for `winmm!timeBeginPeriod`: initialises the loader on first
/// call, then forwards to the system implementation.
///
/// # Safety
/// Same contract as the Win32 function.
#[no_mangle]
pub unsafe extern "system" fn timeBeginPeriod(period: u32) -> u32 {
    INIT.call_once(|| {
        if std::panic::catch_unwind(init).is_err() {
            log_error!("loader panicked during init; continuing unmodded");
        }
    });
    match real_time_begin_period() {
        Some(f) => f(period),
        None => 97, // TIMERR_NOCANDO
    }
}

unsafe fn real_time_begin_period() -> Option<TimeBeginPeriodFn> {
    static REAL: OnceLock<usize> = OnceLock::new();
    let f = *REAL.get_or_init(|| {
        let mut buf = [0u16; 260];
        let n = GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) as usize;
        let mut path: Vec<u16> = buf[..n].to_vec();
        path.extend("\\winmm.dll".encode_utf16());
        path.push(0);
        let module = LoadLibraryW(path.as_ptr());
        if module.is_null() {
            return 0;
        }
        GetProcAddress(module, c"timeBeginPeriod".as_ptr().cast()).map_or(0, |f| f as usize)
    });
    (f != 0).then(|| core::mem::transmute::<usize, TimeBeginPeriodFn>(f))
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

fn init() {
    let Ok(exe) = std::env::current_exe() else { return };
    let is_game = exe.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("hyperdemon"));
    if !is_game {
        return;
    }
    let Some(dir) = exe_dir() else { return };
    let mods_dir = dir.join("mods");
    let _ = std::fs::create_dir_all(&mods_dir);

    let cfg = config::Config::load(&mods_dir.join("loader.ini"));
    log::init(&mods_dir.join("loader.log"), cfg.console, cfg.log_level);
    log_info!("hd-loader {} (ABI v{ABI_VERSION})", env!("CARGO_PKG_VERSION"));
    crash::install();

    let img = *IMAGE.get_or_init(Image::main_module);
    let ts = img.timestamp();
    if ts == KNOWN_BUILD_TIMESTAMP {
        log_info!("game build {ts:#010x} (known)");
    } else {
        log_warn!("game build {ts:#010x} differs from {KNOWN_BUILD_TIMESTAMP:#010x}; relying on signatures");
    }

    let resolved = sigs::resolve(&img);

    let candidates = mods::discover(&mods_dir, &cfg);
    if candidates.is_empty() {
        log_info!("no mods in {}; running unmodded", mods_dir.display());
        return;
    }

    // Fail closed: no guard, no mods.
    if let Err(e) = guard::install(&resolved) {
        log_error!("leaderboard guard could not be installed: {e}");
        log_error!("refusing to load mods so modded runs cannot reach the leaderboards");
        return;
    }

    let mods_dir_str: &'static str = Box::leak(mods_dir.to_string_lossy().into_owned().into_boxed_str());
    let host = HOST.get_or_init(|| HostApi {
        abi_version: ABI_VERSION,
        size: core::mem::size_of::<HostApi>() as u32,
        game_base: img.base,
        game_size: img.size,
        addrs: resolved.addrs,
        mods_dir: StrRef::of(mods_dir_str),
        log: host_log,
        hook_create: hooks::host_hook_create,
        hook_remove: hooks::host_hook_remove,
        find_pattern: host_find_pattern,
    });

    mods::load_all(&candidates, host);
    if mods::count() > 0 {
        events::install(&img, &resolved);
    }
}

unsafe extern "C" fn host_log(level: LogLevel, module: StrRef, msg: StrRef) {
    log::write(level, module.as_str(), msg.as_str());
}

unsafe extern "C" fn host_find_pattern(pattern: StrRef) -> usize {
    let (Some(img), Some(pat)) = (IMAGE.get(), parse_pattern(pattern.as_str())) else { return 0 };
    let text = img.text();
    scan(text, &pat).first().map_or(0, |off| text.as_ptr() as usize + off)
}
