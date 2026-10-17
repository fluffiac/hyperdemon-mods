//! Typed access to the game's root objects.
//!
//! Offsets come from static analysis of `hyperdemon.exe` (SHA-256
//! `7612dd72…3611`); each accessor notes where it was observed. They may move
//! in other builds; the loader logs a warning when its signatures fail to
//! match, and [`Game::get`] returns `None` when the root pointer is unresolved.

use crate::mem::{read, write};

/// The root game/session object (`DAT_1403d91f0`). Created once the game has
/// finished starting up; `None` before that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Game(usize);

macro_rules! field {
    ($(#[$m:meta])* $get:ident, $set:ident: $ty:ty = $off:expr) => {
        $(#[$m])*
        pub fn $get(self) -> $ty { unsafe { read(self.0 + $off) } }
        $(#[$m])*
        pub fn $set(self, v: $ty) { unsafe { write(self.0 + $off, v) } }
    };
    ($(#[$m:meta])* $get:ident: $ty:ty = $off:expr) => {
        $(#[$m])*
        pub fn $get(self) -> $ty { unsafe { read(self.0 + $off) } }
    };
}

/// Upload status shown on the end-of-run screen (`Game+0x14c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitState {
    Error(i32),
    Submitted,
    UploadingReplay,
    Retrying(i32),
}

impl Game {
    pub fn get() -> Option<Game> {
        let pp = crate::addrs().game_ptr;
        if pp == 0 {
            return None;
        }
        let p: usize = unsafe { read(pp) };
        (p != 0).then_some(Game(p))
    }

    pub fn as_ptr(self) -> usize {
        self.0
    }

    field!(
        /// Simulation speed multiplier (main loop accumulator and audio pitch
        /// are both scaled by it; observed in `FUN_1401c33c0` / `FUN_1401c78f0`).
        /// The game rewrites it itself in some modes (e.g. replay speed), so
        /// set it every tick if you want to hold a value.
        speed, set_speed: f32 = 0x20);
    field!(
        /// Second speed factor applied to the accumulator and to hit-stop decay.
        global_speed, set_global_speed: f32 = 0x24);
    field!(
        /// Extra audio pitch factor (multiplied with `speed`).
        audio_pitch, set_audio_pitch: f32 = 0x28);
    field!(
        /// Fixed simulation rate in Hz (`dt = 1 / tick_rate`).
        tick_rate, set_tick_rate: f32 = 0x2c);
    field!(
        /// Remaining hit-stop time in seconds; while > 0 `speed` is multiplied
        /// by [`Game::hitstop_scale`].
        hitstop_time, set_hitstop_time: f32 = 0x30);
    field!(
        hitstop_scale, set_hitstop_scale: f32 = 0x34);
    field!(
        /// Frame-rate cap used by the main loop's frame limiter.
        fps_limit, set_fps_limit: i32 = 0x38);
    field!(
        /// Set by the game when its own integrity checks flag a run.
        cheats_suspected: bool = 0x148);
    field!(
        /// Seconds left to show the submit status line on the end screen.
        submit_message_timer: f32 = 0x150);
    field!(
        /// "NEW HIGH SCORE!" flag on the end-of-run screen.
        new_high_score: bool = 0x330);

    field!(
        /// Whether the player is alive in a run; the game clears it to end a
        /// run (death, training completed).
        alive: bool = 0x2a0);
    field!(
        /// Game clock in seconds (the POWERUPS training schedules its powerup
        /// against it).
        time: f32 = 0x290);

    /// The run score. Stored obfuscated: `bits(score) = [+0x294] ^ [+0x298]`.
    pub fn score(self) -> f32 {
        unsafe { f32::from_bits(read::<u32>(self.0 + 0x294) ^ read::<u32>(self.0 + 0x298)) }
    }

    /// Writes the score the way the game does (World tick of the score
    /// training): a new key `(key + enc) ^ (u32)&enc`, then `enc = bits ^ key`.
    pub fn set_score(self, v: f32) {
        let enc_at = self.0 + 0x294;
        unsafe {
            let key = read::<u32>(self.0 + 0x298).wrapping_add(read::<u32>(enc_at)) ^ enc_at as u32;
            write(enc_at, v.to_bits() ^ key);
            write(self.0 + 0x298, key);
        }
    }

    /// Key code bound to `bind` (index into the game's key-name table).
    pub fn bind_code(self, bind: Bind) -> u32 {
        unsafe { read(self.0 + bind.offset()) }
    }

    pub fn submit_state(self) -> SubmitState {
        match unsafe { read::<i32>(self.0 + 0x14c) } {
            n if n < 0 => SubmitState::Error(n),
            0 => SubmitState::Submitted,
            1 => SubmitState::UploadingReplay,
            n => SubmitState::Retrying(n),
        }
    }
}

/// Rebindable game actions (`Game+0x4b8..=0x4d8`, one key code each). Names
/// follow the training hints that print them; the movement order comes from
/// the STOMP hint ("release %s %s %s %s"). `+0x4cc`/`+0x4d4` are unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    Forward,
    Back,
    Left,
    Right,
    /// Daggers (LMB by default).
    Fire,
    /// Laser / grab (RMB by default).
    Laser,
    /// Jump / dash (SPACE by default).
    Jump,
}

impl Bind {
    pub const ALL: [Bind; 7] = [Self::Forward, Self::Back, Self::Left, Self::Right, Self::Fire, Self::Laser, Self::Jump];

    pub fn name(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Back => "back",
            Self::Left => "left",
            Self::Right => "right",
            Self::Fire => "fire",
            Self::Laser => "laser",
            Self::Jump => "jump",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.name() == s)
    }

    fn offset(self) -> usize {
        match self {
            Self::Forward => 0x4b8,
            Self::Back => 0x4bc,
            Self::Left => 0x4c0,
            Self::Right => 0x4c4,
            Self::Fire => 0x4c8,
            Self::Laser => 0x4d0,
            Self::Jump => 0x4d8,
        }
    }

    /// Display name of the bound key ("SPACE", "LMB", "E"...), read from the
    /// game's key-name table (`DAT_1403dc2b0`, `const char*[]` by key code).
    pub fn key_name(self) -> Option<String> {
        let table = key_name_table()?;
        let code = Game::get()?.bind_code(self);
        if code >= 0x200 {
            return None;
        }
        unsafe {
            let s: usize = read(table + code as usize * 8);
            (s != 0).then(|| std::ffi::CStr::from_ptr(s as *const core::ffi::c_char).to_string_lossy().into_owned())
        }
    }
}

/// Finds the key-name table through the on-screen key display
/// (`0x14009302c`: `mov eax,[r11+4B8h]; lea r13,[table]; mov rax,[r13+rax*8]`).
/// The `lea` is at +7.
#[doc(hidden)]
pub const KEY_NAME_TABLE_SIG: &str = "41 8B 83 B8 04 00 00 4C 8D 2D ?? ?? ?? ?? 49 8B 44 C5 00";

fn key_name_table() -> Option<usize> {
    static TABLE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let t = *TABLE.get_or_init(|| {
        let p = unsafe { (crate::host().find_pattern)(crate::abi::StrRef::of(KEY_NAME_TABLE_SIG)) };
        if p == 0 {
            return 0;
        }
        let lea = p + 7;
        let rel: i32 = unsafe { read(lea + 3) };
        (lea as isize + 7 + rel as isize) as usize
    });
    (t != 0).then_some(t)
}

/// The engine application object (`DAT_1403d91d8`, 0x108 bytes). Its first
/// field is the active scene, whose vtable drives tick/UI/render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct App(usize);

impl App {
    pub fn get() -> Option<App> {
        let pp = crate::addrs().app_ptr;
        if pp == 0 {
            return None;
        }
        let p: usize = unsafe { read(pp) };
        (p != 0).then_some(App(p))
    }

    pub fn as_ptr(self) -> usize {
        self.0
    }

    /// Pointer to the active scene object (menus, run, replay viewer...).
    pub fn scene(self) -> usize {
        unsafe { read(self.0) }
    }

    /// Vtable of the active scene; useful to tell scenes apart.
    pub fn scene_vtable(self) -> usize {
        let s = self.scene();
        if s == 0 {
            0
        } else {
            unsafe { read(s) }
        }
    }
}
