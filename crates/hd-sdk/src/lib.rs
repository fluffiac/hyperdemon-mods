//! SDK for writing HyperDemon mods in Rust.
//!
//! ```ignore
//! use hd_sdk::prelude::*;
//!
//! #[derive(Default)]
//! struct Hello { ticks: u64 }
//!
//! impl Mod for Hello {
//!     const NAME: &'static str = "hello";
//!     const VERSION: &'static str = "0.1.0";
//!
//!     fn tick(&mut self, _dt: f32) { self.ticks += 1; }
//!
//!     fn ui(&mut self, ui: &Ui) {
//!         ui.window("hello").show(|ui| ui.text(&format!("ticks: {}", self.ticks)));
//!     }
//! }
//!
//! hd_sdk::hd_mod!(Hello);
//! ```

pub mod abi;
pub mod audio;
pub mod game;
pub mod hook;
pub mod hud;
pub mod imgui;
pub mod input;
pub mod mem;
pub mod msvc;
pub mod net;
pub mod training;
pub mod world;

use std::sync::OnceLock;

pub use abi::LogLevel;

pub mod prelude {
    pub use crate::game::Game;
    pub use crate::net::HttpRequest;
    pub use crate::world::{Spawnable, World};
    pub use crate::imgui::{Ui, WindowFlags};
    pub use crate::{hd_mod, log_debug, log_error, log_info, log_warn, Mod};
}

/// Implemented by a mod's state type. Register it with [`hd_mod!`].
///
/// All methods have empty defaults. `tick` and `ui` run on the game's main
/// thread, `frame` on its render thread; the instance lives behind a mutex so
/// `&mut self` is always exclusive.
pub trait Mod: Send + Sized + 'static {
    const NAME: &'static str;
    const VERSION: &'static str;
    const AUTHOR: &'static str = "";
    const DESCRIPTION: &'static str = "";

    /// Construct the mod. Runs once, before the game's main loop starts.
    fn init() -> Self;

    /// Once per fixed simulation step (`dt` is the step length in seconds).
    fn tick(&mut self, _dt: f32) {}

    /// Once per frame inside the game's HUD ImGui frame.
    fn ui(&mut self, _ui: &imgui::Ui) {}

    /// Once per frame on the render thread, GL context current, before present.
    fn frame(&mut self) {}

    /// Once at process exit.
    fn unload(&mut self) {}

    /// Before the game submits an HTTP request; may rewrite its URL.
    fn http_request(&mut self, _req: &net::HttpRequest) {}

    /// After the gameplay World ticked (only during a run).
    fn world_tick(&mut self, _world: world::World, _dt: f32) {}

    /// Before the gameplay World ticks.
    fn world_tick_pre(&mut self, _world: world::World, _dt: f32) {}

    /// Right before the game runs its training panel and steps (every frame
    /// in the menu/run scene).
    fn training_hud_begin(&mut self) {}

    /// Right after the game ran its training panel and steps.
    fn training_hud_end(&mut self) {}
}

static HOST: OnceLock<&'static abi::HostApi> = OnceLock::new();
static MOD_NAME: OnceLock<&'static str> = OnceLock::new();

/// The loader's API. Panics if called before the loader initialised the mod.
pub fn host() -> &'static abi::HostApi {
    HOST.get().expect("hd-sdk used before hdmod_entry")
}

/// Resolved game addresses (fields are 0 if a signature failed to match).
pub fn addrs() -> &'static abi::GameAddrs {
    &host().addrs
}

pub fn mods_dir() -> &'static str {
    unsafe { host().mods_dir.as_str() }
}

#[doc(hidden)]
/// # Safety
/// `host` must be null or point to a `HostApi` that lives for the process.
pub unsafe fn __set_host(host: *const abi::HostApi, name: &'static str) -> bool {
    let Some(host) = host.as_ref() else { return false };
    if host.abi_version != abi::ABI_VERSION {
        return false;
    }
    let _ = HOST.set(host);
    let _ = MOD_NAME.set(name);
    // Each mod DLL has its own std, so each needs its own hook; without it
    // panic messages go to a stderr nobody sees.
    std::panic::set_hook(Box::new(|info| log(LogLevel::Error, &format!("{info}"))));
    true
}

pub fn log(level: LogLevel, msg: &str) {
    if let Some(h) = HOST.get() {
        let module = MOD_NAME.get().copied().unwrap_or("?");
        unsafe { (h.log)(level, abi::StrRef::of(module), abi::StrRef::of(msg)) }
    }
}

#[macro_export]
macro_rules! log_error { ($($t:tt)*) => { $crate::log($crate::LogLevel::Error, &format!($($t)*)) } }
#[macro_export]
macro_rules! log_warn { ($($t:tt)*) => { $crate::log($crate::LogLevel::Warn, &format!($($t)*)) } }
#[macro_export]
macro_rules! log_info { ($($t:tt)*) => { $crate::log($crate::LogLevel::Info, &format!($($t)*)) } }
#[macro_export]
macro_rules! log_debug { ($($t:tt)*) => { $crate::log($crate::LogLevel::Debug, &format!($($t)*)) } }

/// Runs `f` against the mod instance, catching panics so a buggy mod gets
/// disabled instead of taking the game down.
#[doc(hidden)]
pub fn __with_instance<M: Mod>(
    slot: &std::sync::Mutex<Option<M>>,
    what: &str,
    f: impl FnOnce(&mut M),
) {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut guard = match slot.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(m) = guard.as_mut() {
            f(m);
        }
    }));
    if r.is_err() {
        log(LogLevel::Error, &format!("panicked in {what}; mod disabled"));
        if let Ok(mut g) = slot.lock() {
            *g = None;
        }
    }
}

/// Exports the `hdmod_entry` symbol for a type implementing [`Mod`].
#[macro_export]
macro_rules! hd_mod {
    ($ty:ty) => {
        const _: () = {
            use $crate::abi;
            static INSTANCE: ::std::sync::Mutex<Option<$ty>> = ::std::sync::Mutex::new(None);

            unsafe extern "C" fn on_tick(dt: f32) {
                $crate::__with_instance(&INSTANCE, "tick", |m| <$ty as $crate::Mod>::tick(m, dt));
            }
            unsafe extern "C" fn on_ui() {
                $crate::__with_instance(&INSTANCE, "ui", |m| {
                    let ui = $crate::imgui::Ui::__new();
                    <$ty as $crate::Mod>::ui(m, &ui)
                });
            }
            unsafe extern "C" fn on_frame() {
                $crate::__with_instance(&INSTANCE, "frame", |m| <$ty as $crate::Mod>::frame(m));
            }
            unsafe extern "C" fn on_http_request(req: *mut ::core::ffi::c_void) {
                if let Some(r) = $crate::net::HttpRequest::from_ptr(req as usize) {
                    $crate::__with_instance(&INSTANCE, "http_request", |m| {
                        <$ty as $crate::Mod>::http_request(m, &r)
                    });
                }
            }
            unsafe extern "C" fn on_world_tick(world: *mut ::core::ffi::c_void, dt: f32) {
                if let Some(w) = $crate::world::World::from_ptr(world as usize) {
                    $crate::__with_instance(&INSTANCE, "world_tick", |m| {
                        <$ty as $crate::Mod>::world_tick(m, w, dt)
                    });
                }
            }
            unsafe extern "C" fn on_world_tick_pre(world: *mut ::core::ffi::c_void, dt: f32) {
                if let Some(w) = $crate::world::World::from_ptr(world as usize) {
                    $crate::__with_instance(&INSTANCE, "world_tick_pre", |m| {
                        <$ty as $crate::Mod>::world_tick_pre(m, w, dt)
                    });
                }
            }
            unsafe extern "C" fn on_training_hud_begin() {
                $crate::__with_instance(&INSTANCE, "training_hud_begin", |m| {
                    <$ty as $crate::Mod>::training_hud_begin(m)
                });
            }
            unsafe extern "C" fn on_training_hud_end() {
                $crate::__with_instance(&INSTANCE, "training_hud_end", |m| {
                    <$ty as $crate::Mod>::training_hud_end(m)
                });
            }
            unsafe extern "C" fn on_unload() {
                $crate::__with_instance(&INSTANCE, "unload", |m| <$ty as $crate::Mod>::unload(m));
            }

            static DESCRIPTOR: abi::ModDescriptor = abi::ModDescriptor {
                abi_version: abi::ABI_VERSION,
                name: abi::StrRef::new(<$ty as $crate::Mod>::NAME),
                version: abi::StrRef::new(<$ty as $crate::Mod>::VERSION),
                author: abi::StrRef::new(<$ty as $crate::Mod>::AUTHOR),
                description: abi::StrRef::new(<$ty as $crate::Mod>::DESCRIPTION),
                callbacks: abi::ModCallbacks {
                    on_tick: Some(on_tick),
                    on_ui: Some(on_ui),
                    on_frame: Some(on_frame),
                    on_unload: Some(on_unload),
                    on_http_request: Some(on_http_request),
                    on_world_tick: Some(on_world_tick),
                    on_world_tick_pre: Some(on_world_tick_pre),
                    on_training_hud_begin: Some(on_training_hud_begin),
                    on_training_hud_end: Some(on_training_hud_end),
                },
            };

            #[no_mangle]
            pub unsafe extern "C" fn hdmod_entry(
                host: *const abi::HostApi,
            ) -> *const abi::ModDescriptor {
                if !$crate::__set_host(host, <$ty as $crate::Mod>::NAME) {
                    return ::core::ptr::null();
                }
                let ok = ::std::panic::catch_unwind(|| {
                    *INSTANCE.lock().unwrap() = Some(<$ty as $crate::Mod>::init());
                });
                if ok.is_err() {
                    $crate::log_error!("panicked in init");
                    return ::core::ptr::null();
                }
                &DESCRIPTOR
            }
        };
    };
}
