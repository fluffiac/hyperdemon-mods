//! The run's spawn state in game memory. Offsets and evidence:
//! `docs/REVERSE_ENGINEERING.md`, "Run spawner". Only valid on the main
//! thread while the World is alive (the World tick callbacks).

use hd_sdk::mem::{find_pattern, read, write};
use hd_sdk::prelude::*;

// Game
const MODE: usize = 0x300;
const MODE_RUN: i32 = 2;
const RECORDING: usize = 0x364;
const WATCHING: usize = 0x365;
const ACTIVE_CLIP: usize = 0x370;
const REPLAY: usize = 0x3a0;
/// Current arena radius; the level scene moves it toward
/// `ARENA_BASE - 22·(gems/149)^2.8` every tick (`FUN_1400081f0`).
const ARENA_NOW: usize = 0x2e4;
const ARENA_EDGE: usize = 0x2ec;
const ARENA_BASE: usize = 0x2f0;

// World
const SCENE: usize = 0x130;
/// Track 0, the only one the game loads (`world+0x140 + 0x48·k`).
const TRACK: usize = 0x140;
const TRACK_SIZE: usize = 0x48;
/// Obfuscated pair (`a ^ b`): waves started on the track.
const T_STARTED: usize = 0x08;
/// `std::vector<u64>`: lo = f32 value (unused), hi = i32 type, -1 ends a wave.
const T_ENTRIES: usize = 0x20;
/// `std::vector` of 0x5c-byte wave cursors (`FUN_1400de900` walks them).
const CURSORS: usize = 0x190;
const CURSOR_SIZE: usize = 0x5c;
const C_TIMER: usize = 0x00; // spawns only while >= 0
const C_INDEX: usize = 0x04; // obfuscated pair at +0x04/+0x08: next entry
const C_TRACK: usize = 0x0c;
const C_AGE: usize = 0x10; // dropped after 25 s unless it saw kills
const C_ID: usize = 0x14; // group id: wave << 4 | track
/// World vtable: `u32 StartWave(world, track, const float pos[3], bool push)`.
const VT_START_WAVE: usize = 0x80;

// Level scene
const RETRIEVAL: usize = 0x48;
// Retrieval ("powerup") object, slot k = 0 mercury, 1 sulfur.
const P_TIME: usize = 0x28; // + 4k, f32 game clock when it appears; 10000 = none
const P_ARMED: usize = 0x30; // + k, u8
const P_LASTS: [usize; 2] = [0xd8, 0xd4];
const P_FADE: usize = 0xdc;
pub const POWERUP_LASTS: [f32; 2] = [10.0, 5.0];
/// Replay vtable: `void SetPowerupTime(replay, int slot, float time)`, the
/// call the game makes when it arms a powerup while recording.
const VT_REPLAY_POWERUP: usize = 0xb8;

/// `vector<u64>::resize(vec, n)` (`FUN_1400e1780`, called by
/// `World::SetMode` to load the table). Uses the game's allocator.
const RESIZE_SIG: &str = "4C 89 44 24 18 57 41 56 41 57 48 83 EC 40 48 C7 44 24 30 FE FF FF FF 48 89 5C 24 68 48 89 74 24 78 48 8B DA 48 8B F1 48 8B 41 08 48 8B 11 4C 8B F0 4C 2B F2 49 C1 FE 03 48 8B 49 10 48 2B CA 48 C1 F9 03 48 3B D9 0F 86 ?? ?? ?? ?? 49 B8 FF FF FF FF FF FF FF 1F 49 3B D8 0F 87 ?? ?? ?? ?? 48 8B D1 48 D1 EA 49 8B C0 48 2B C2 48 3B C8 76 05 4C 8B FB EB 0B 4C 8D 3C 0A 4C 3B FB 4C 0F 42 FB 4C 89 7C 24 70 4A 8D 04 FD 00 00 00 00 4D 3B F8 76 0C 48 C7 C2 FF FF FF FF 48 8B C2 EB 0F 48 3D 00 10 00 00 72 31";

unsafe fn vfn(obj: usize, slot: usize) -> usize {
    read::<usize>(read::<usize>(obj) + slot)
}

fn pair(addr: usize) -> u32 {
    unsafe { read::<u32>(addr) ^ read::<u32>(addr + 4) }
}

pub fn mode(g: Game) -> i32 {
    unsafe { read(g.as_ptr() + MODE) }
}

/// A run the player is playing: not a replay, clip or training.
pub fn is_live_run(g: Game) -> bool {
    unsafe { mode(g) == MODE_RUN && read::<u8>(g.as_ptr() + WATCHING) == 0 && read::<i64>(g.as_ptr() + ACTIVE_CLIP) < 0 }
}

pub fn waves_started(w: World) -> u32 {
    pair(w.as_ptr() + TRACK + T_STARTED)
}

fn cursors(w: World) -> (usize, usize) {
    unsafe { (read(w.as_ptr() + CURSORS), read(w.as_ptr() + CURSORS + 8)) }
}

/// No wave has started yet: the first World tick after `SetMode(2)`.
pub fn is_fresh(w: World) -> bool {
    let (b, e) = cursors(w);
    waves_started(w) == 0 && b == e
}

fn entries(w: World, track: usize) -> (usize, usize) {
    let v = w.as_ptr() + TRACK + track * TRACK_SIZE + T_ENTRIES;
    unsafe { (read(v), (read::<usize>(v + 8) - read::<usize>(v)) / 8) }
}

/// The types in track 0, in order (-1 ends a wave).
pub fn table(w: World) -> Vec<i32> {
    let (b, n) = entries(w, 0);
    (0..n).map(|i| unsafe { read::<i32>(b + i * 8 + 4) }).collect()
}

/// Replaces track 0. Must only run before the first wave starts: cursors
/// index into the vector.
pub fn set_table(w: World, types: &[i32]) -> Result<(), String> {
    let vec = w.as_ptr() + TRACK + T_ENTRIES;
    let n = types.len();
    unsafe {
        let begin: usize = read(vec);
        let cap = (read::<usize>(vec + 16) - begin) / 8;
        if n > cap {
            static RESIZE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
            let f = *RESIZE.get_or_init(|| find_pattern(RESIZE_SIG).unwrap_or(0));
            if f == 0 {
                return Err(format!("table of {n} entries needs vector::resize, whose signature did not match"));
            }
            let f: unsafe extern "C" fn(usize, usize) = core::mem::transmute(f);
            f(vec, n);
        } else {
            write(vec + 8, begin + n * 8);
        }
        let begin: usize = read(vec);
        for (i, &t) in types.iter().enumerate() {
            write::<u64>(begin + i * 8, (t as u32 as u64) << 32 | u64::from(1.0f32.to_bits()));
        }
    }
    Ok(())
}

/// Holds every wave cursor that still has enemies to spawn: its timer stays
/// below 0 so `FUN_1400de900` skips it, and its age at 0 so it is not
/// dropped. Returns how many are held.
pub fn hold_pending_waves(w: World) -> usize {
    let (b, e) = cursors(w);
    let mut held = 0;
    let mut c = b;
    while c + CURSOR_SIZE <= e {
        let track = unsafe { read::<i32>(c + C_TRACK) } as usize & 0xf;
        let (eb, n) = entries(w, track);
        let idx = pair(c + C_INDEX) as usize;
        if idx < n && unsafe { read::<i32>(eb + idx * 8 + 4) } != -1 {
            unsafe {
                write::<f32>(c + C_TIMER, -1.0);
                write::<f32>(c + C_AGE, 0.0);
            }
            held += 1;
        }
        c += CURSOR_SIZE;
    }
    held
}

/// Waves (1-based, track 0) whose cursor has gone through every entry: it
/// sits on the wave's -1. The id at cursor `+0x14` is `wave << 4 | track`.
pub fn finished_waves(w: World) -> Vec<usize> {
    let (b, e) = cursors(w);
    let (eb, n) = entries(w, 0);
    let mut out = Vec::new();
    let mut c = b;
    while c + CURSOR_SIZE <= e {
        let id = unsafe { read::<i32>(c + C_ID) };
        let idx = pair(c + C_INDEX) as usize;
        if id > 0 && id & 0xf == 0 && (idx >= n || unsafe { read::<i32>(eb + idx * 8 + 4) } == -1) {
            out.push((id >> 4) as usize);
        }
        c += CURSOR_SIZE;
    }
    out
}

/// Starts the next wave of track 0 the way a kill does, at the arena centre.
pub fn start_wave(w: World) {
    let pos = [0.0f32; 3];
    unsafe {
        let f: unsafe extern "C" fn(usize, u64, *const f32, u64) -> u32 = core::mem::transmute(vfn(w.as_ptr(), VT_START_WAVE));
        f(w.as_ptr(), 0, pos.as_ptr(), 1);
    }
}

/// Live enemies the cap counts. The centipede's count is not known, so it
/// is left out.
pub fn enemies_alive(w: World) -> i32 {
    [Spawnable::Spawner, Spawnable::Scuttles, Spawnable::Snakes, Spawnable::Spiders].into_iter().map(|k| w.count(k).max(0)).sum()
}

pub struct Arena {
    pub now: f32,
    pub base: f32,
}

pub fn arena(g: Game) -> Arena {
    unsafe { Arena { now: read(g.as_ptr() + ARENA_NOW), base: read(g.as_ptr() + ARENA_BASE) } }
}

pub fn set_arena_base(g: Game, r: f32) {
    unsafe {
        write(g.as_ptr() + ARENA_BASE, r);
        write(g.as_ptr() + ARENA_EDGE, r);
    }
}

pub fn hold_arena(g: Game) {
    unsafe { write::<f32>(g.as_ptr() + ARENA_NOW, read(g.as_ptr() + ARENA_BASE)) }
}

/// The two powerup slots (`*(world+0x130)+0x48`, the "retrieval" system).
#[derive(Clone, Copy)]
pub struct Powerups(usize);

#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub armed: bool,
    /// Game clock when it appears.
    pub time: f32,
    pub lasts: f32,
    pub fade: f32,
}

impl Powerups {
    pub fn of(w: World) -> Option<Powerups> {
        unsafe {
            let scene: usize = read(w.as_ptr() + SCENE);
            if scene == 0 {
                return None;
            }
            let p: usize = read(scene + RETRIEVAL);
            (p != 0).then_some(Powerups(p))
        }
    }

    pub fn slot(self, k: usize) -> Slot {
        unsafe {
            Slot {
                armed: read::<u8>(self.0 + P_ARMED + k) != 0,
                time: read(self.0 + P_TIME + 4 * k),
                lasts: read(self.0 + P_LASTS[k]),
                fade: read(self.0 + P_FADE),
            }
        }
    }

    pub fn set_lasts(self, k: usize, s: f32) {
        unsafe { write(self.0 + P_LASTS[k], s) }
    }

    /// Keeps the game from arming slot `k`: it only arms while
    /// `clock + 3 < time` (`FUN_14011c490`).
    pub fn block(self, k: usize) {
        if !self.slot(k).armed {
            unsafe { write::<f32>(self.0 + P_TIME + 4 * k, 0.0) }
        }
    }

    /// Arms slot `k` to appear at `at`, as the game does, and tells the
    /// replay recorder.
    pub fn arm(self, g: Game, k: usize, at: f32) {
        unsafe {
            write(self.0 + P_TIME + 4 * k, at);
            write::<u8>(self.0 + P_ARMED + k, 1);
            let replay: usize = read(g.as_ptr() + REPLAY);
            if read::<u8>(g.as_ptr() + RECORDING) != 0 && replay != 0 {
                let f: unsafe extern "C" fn(usize, i32, f32) = core::mem::transmute(vfn(replay, VT_REPLAY_POWERUP));
                f(replay, k as i32, at);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Checks the signature against the game binary on disk. Set
    //! `HD_GAME_EXE` to override the default Steam path; skipped if absent.

    const DEFAULT_EXE: &str = r"C:\Program Files (x86)\Steam\steamapps\common\hyperdemon\hyperdemon.exe";

    #[test]
    fn resize_signature_is_unique() {
        let path = std::env::var("HD_GAME_EXE").unwrap_or_else(|_| DEFAULT_EXE.into());
        let Ok(exe) = std::fs::read(path) else {
            eprintln!("game exe not found; skipping");
            return;
        };
        let pat: Vec<Option<u8>> =
            super::RESIZE_SIG.split_whitespace().map(|b| u8::from_str_radix(b, 16).ok()).collect();
        let hits: Vec<usize> = (0..exe.len() - pat.len())
            .filter(|&i| pat.iter().enumerate().all(|(j, b)| b.is_none_or(|b| exe[i + j] == b)))
            .collect();
        // .text is at RVA 0x1000, file offset 0x400 in build 7612dd72.
        assert_eq!(hits, vec![0x1400e1780 - 0x1_4000_0000 - 0x1000 + 0x400]);
    }
}
