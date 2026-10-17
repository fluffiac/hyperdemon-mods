//! Per-enemy spawn options from the table: a spawn point (`owl@x,z`) and the
//! boss's starting phase (`boss:3`), and the skulls, which the game's table
//! cannot hold ([`spawn_skulls`]).
//!
//! The wave sequencer (`FUN_1400de900`) calls each manager's spawn method
//! through its vtable with `(variant, group id)`, where the id is
//! `wave << 4 | track`. This module swaps those vtable entries, and the
//! collision system's two "free spot near a point" queries, for wrappers:
//!
//! * trilo, snake, spider ask the collision system where to spawn (centre
//!   0,0,0, radius 43); while one of them spawns an entry with a position,
//!   the query returns that position. The game then records it in the
//!   replay itself.
//! * the owl places the boss at (0,30,0) and the first owl of a run at
//!   z = -20 after asking, so its position is written into its record after
//!   the spawn, and into the 12 bytes the spawn just wrote to the replay.
//! * phase N calls the owl manager's phase step (vtable `+0xf8`, what every
//!   wave enemy's death calls) N times with only the new boss visible.
//!
//! Vtable entries instead of inline hooks: the placement query is also
//! called directly by its sibling, and direct calls in this exe may keep
//! values in volatile registers (see the hooking caveat in the RE notes).
//! Spawns made with a replay record (playback) or with id -1 (trainings,
//! other mods) pass through untouched.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use hd_sdk::mem::{read, write};
use hd_sdk::prelude::*;

use crate::cfg::{Entry, Pos};

// Level scene (`*(world+0x130)`) systems.
const SCENE: usize = 0x130;
const COLLISION: usize = 0x58;
const OWL: usize = 0x98;
const TRILO: usize = 0x80;
const SNAKE: usize = 0x88;
const SPIDER: usize = 0xc8;

// Owl manager: `vector<record>` of 400-byte records at +0x70.
const OWL_RECORDS: usize = 0x70;
const OWL_RECORD: usize = 400;
const R_STATE: usize = 0x00; // 5 = free slot
const R_POS: usize = 0x14; // x, y, z
const R_PREV_POS: usize = 0x20;
const R_VARIANT: usize = 0x0d; // u8, 1 = boss
const R_BOSS_POS: [usize; 2] = [0x80, 0x8c];
const OWL_PHASE_STEP: usize = 0xf8;

// Game
const LAST_OWL_POS: usize = 0x2f4;
const RECORDING: usize = 0x364;
const REPLAY: usize = 0x3a0;
/// Replay class (vtable RVA); its spawn-position writer is at `+0x200`:
/// u32 size, u32 pos, u8* data at +8, flag `+0x11` (writes enabled), mode
/// `+0x18` (2 = reading).
const REPLAY_VTABLE_RVA: usize = 0x36a190;
const REPLAY_WRITER: usize = 0x200;

/// The table's per-entry options for the current run, and which entries
/// already spawned.
struct Plan {
    waves: Vec<Vec<Entry>>,
    used: HashSet<(usize, usize)>,
    /// Waves whose skulls have spawned.
    skulls_done: HashSet<usize>,
    /// State of the spawn-point generator for skulls without one.
    rng: u32,
}

static PLAN: Mutex<Option<Plan>> = Mutex::new(None);

/// Sets the options for the run that is starting (`None`: the game's table).
pub fn set_plan(waves: Option<Vec<Vec<Entry>>>) {
    let plan = waves.filter(|w| w.iter().flatten().any(|e| e.at.is_some() || e.phase > 0 || e.kind.skull_kind().is_some()));
    *PLAN.lock().unwrap_or_else(|p| p.into_inner()) =
        plan.map(|waves| Plan { waves, used: HashSet::new(), skulls_done: HashSet::new(), rng: 0x9e37_79b9 });
}

/// Height a skull spawns at when its entry gives none.
const SKULL_HEIGHT: f32 = 10.0;
/// Skulls without a spawn point appear on a ring of this radius.
const SKULL_RING: f32 = 25.0;

/// Spawns the skulls of every wave whose other enemies have spawned (so the
/// enemy cap holds them too). Skulls spawned this way do not advance the
/// waves when they die: only the game's own wave enemies do. Call after the
/// World tick.
pub fn spawn_skulls(world: World) {
    let finished = crate::run::finished_waves(world);
    let mut todo: Vec<(u32, [f32; 3])> = Vec::new();
    {
        let mut guard = PLAN.lock().unwrap_or_else(|p| p.into_inner());
        let Some(plan) = guard.as_mut() else { return };
        for wave in finished {
            if !plan.skulls_done.insert(wave) {
                continue;
            }
            let Some(entries) = wave.checked_sub(1).and_then(|i| plan.waves.get(i)) else { continue };
            for e in entries {
                let Some(kind) = e.kind.skull_kind() else { continue };
                let pos = match e.at {
                    Some(p) => [p.x, p.y.unwrap_or(SKULL_HEIGHT), p.z],
                    None => {
                        plan.rng = plan.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        let a = (plan.rng >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU;
                        [a.cos() * SKULL_RING, SKULL_HEIGHT, a.sin() * SKULL_RING]
                    }
                };
                todo.push((kind, pos));
            }
        }
    }
    // `SpawnNearPlayer` puts it at `pos + (0, 2, 0)` plus a random unit
    // vector (`FUN_1401c8bc0`), not near the player.
    for (kind, [x, y, z]) in todo {
        world.spawn_near_player(kind, [x, y - 2.0, z]);
    }
}

/// The first entry of this wave with `table_type` that has not spawned yet.
fn claim(id: i32, table_type: i32) -> Option<Entry> {
    if id < 0 || id & 0xf != 0 {
        return None;
    }
    let wave = (id >> 4) as usize;
    let mut guard = PLAN.lock().unwrap_or_else(|p| p.into_inner());
    let plan = guard.as_mut()?;
    let entries = plan.waves.get(wave.checked_sub(1)?)?;
    let i = (0..entries.len()).find(|&i| entries[i].kind.0 == table_type && !plan.used.contains(&(wave, i)))?;
    plan.used.insert((wave, i));
    Some(entries[i])
}

/// Position the next placement query returns (f32 bits of x, y or NaN, z;
/// 0 = none). Main thread only; atomics just make it a plain static.
static PENDING: [AtomicUsize; 4] = [AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)];

fn set_pending(p: Option<Pos>) {
    match p {
        Some(p) => {
            PENDING[1].store(p.x.to_bits() as usize, Ordering::Relaxed);
            PENDING[2].store(p.y.unwrap_or(f32::NAN).to_bits() as usize, Ordering::Relaxed);
            PENDING[3].store(p.z.to_bits() as usize, Ordering::Relaxed);
            PENDING[0].store(1, Ordering::Relaxed);
        }
        None => PENDING[0].store(0, Ordering::Relaxed),
    }
}

fn take_pending() -> Option<[f32; 3]> {
    (PENDING[0].swap(0, Ordering::Relaxed) != 0).then(|| [1, 2, 3].map(|i| f32::from_bits(PENDING[i].load(Ordering::Relaxed) as u32)))
}

// Original vtable targets.
static OWL_SPAWN: AtomicUsize = AtomicUsize::new(0);
static TRILO_SPAWN: AtomicUsize = AtomicUsize::new(0);
static SNAKE_SPAWN: AtomicUsize = AtomicUsize::new(0);
static SPIDER_SPAWN: AtomicUsize = AtomicUsize::new(0);
static PLACE: AtomicUsize = AtomicUsize::new(0);
static PLACE_WALL: AtomicUsize = AtomicUsize::new(0);

type OwlSpawnFn = unsafe extern "C" fn(usize, *mut u16, u64, u64, usize) -> *mut u16;
type SpawnFn = unsafe extern "C" fn(usize, u64, u64, usize);
type PlaceFn = unsafe extern "C" fn(usize, *mut f32, usize, *const f32, f32) -> *mut f32;

fn live() -> Option<Game> {
    Game::get().filter(|&g| crate::run::is_live_run(g))
}

unsafe fn spawn_with_position(orig: &AtomicUsize, table_type: [i32; 2], mgr: usize, variant: u64, id: u64, rec: usize) {
    let entry = if rec == 0 && live().is_some() { claim(id as i32, table_type[(variant & 1) as usize]) } else { None };
    set_pending(entry.and_then(|e| e.at));
    let f: SpawnFn = core::mem::transmute(orig.load(Ordering::Relaxed));
    f(mgr, variant, id, rec);
    set_pending(None);
}

unsafe extern "C" fn trilo_spawn(mgr: usize, variant: u64, id: u64, rec: usize) {
    spawn_with_position(&TRILO_SPAWN, [12, 14], mgr, variant, id, rec);
}

unsafe extern "C" fn snake_spawn(mgr: usize, variant: u64, id: u64, rec: usize) {
    spawn_with_position(&SNAKE_SPAWN, [10, 15], mgr, variant, id, rec);
}

unsafe extern "C" fn spider_spawn(mgr: usize, variant: u64, id: u64, rec: usize) {
    spawn_with_position(&SPIDER_SPAWN, [3, 8], mgr, variant, id, rec);
}

unsafe fn place(orig: &AtomicUsize, coll: usize, out: *mut f32, key: usize, center: *const f32, radius: f32) -> *mut f32 {
    let f: PlaceFn = core::mem::transmute(orig.load(Ordering::Relaxed));
    let r = f(coll, out, key, center, radius);
    if let Some([x, y, z]) = take_pending() {
        *out = x;
        if !y.is_nan() {
            *out.add(1) = y;
        }
        *out.add(2) = z;
    }
    r
}

unsafe extern "C" fn place_detour(coll: usize, out: *mut f32, key: usize, center: *const f32, radius: f32) -> *mut f32 {
    place(&PLACE, coll, out, key, center, radius)
}

unsafe extern "C" fn place_wall_detour(coll: usize, out: *mut f32, key: usize, center: *const f32, radius: f32) -> *mut f32 {
    place(&PLACE_WALL, coll, out, key, center, radius)
}

unsafe extern "C" fn owl_spawn(mgr: usize, out: *mut u16, variant: u64, id: u64, rec: usize) -> *mut u16 {
    let entry = match live() {
        Some(_) if rec == 0 => claim(id as i32, if (variant & 0xff) == 1 { 13 } else { 11 }),
        _ => None,
    };
    let f: OwlSpawnFn = core::mem::transmute(OWL_SPAWN.load(Ordering::Relaxed));
    let r = f(mgr, out, variant, id, rec);
    let (Some(e), Some(g)) = (entry, Game::get()) else { return r };
    let record = read::<usize>(mgr + OWL_RECORDS) + usize::from(*out.add(1)) * OWL_RECORD;
    if let Some(p) = e.at {
        move_owl(g, record, p);
    }
    if e.phase > 0 {
        advance_boss(mgr, record, e.phase);
    }
    r
}

/// Moves a just-spawned owl, and the position its spawn wrote to the replay.
unsafe fn move_owl(g: Game, record: usize, p: Pos) {
    let old: [f32; 3] = read(record + R_POS);
    let pos = [p.x, p.y.unwrap_or(old[1]), p.z];
    let boss = read::<u8>(record + R_VARIANT) == 1;
    for at in [R_POS, R_PREV_POS].into_iter().chain(if boss { R_BOSS_POS.to_vec() } else { vec![] }) {
        write(record + at, pos);
    }
    write(g.as_ptr() + LAST_OWL_POS, pos);
    let replay: usize = read(g.as_ptr() + REPLAY);
    if read::<u8>(g.as_ptr() + RECORDING) == 0 || replay == 0 || read::<usize>(replay) != hd_sdk::mem::rva(REPLAY_VTABLE_RVA) {
        return;
    }
    let w = replay + REPLAY_WRITER;
    let at: u32 = read(w + 4);
    if read::<u32>(w + 0x18) != 2 && read::<u8>(w + 0x11) != 0 && at >= 12 {
        // Only if the last write there is this spawn's position.
        let slot = read::<usize>(w + 8) + at as usize - 12;
        if read::<[f32; 3]>(slot) == old {
            write(slot, pos);
        }
    }
}

/// Runs the owl manager's phase step `n` times with every other boss hidden
/// from it (it advances every boss it sees).
unsafe fn advance_boss(mgr: usize, record: usize, n: u8) {
    let (b, e): (usize, usize) = (read(mgr + OWL_RECORDS), read(mgr + OWL_RECORDS + 8));
    let others: Vec<usize> = (b..e)
        .step_by(OWL_RECORD)
        .filter(|&r| r != record && read::<i32>(r + R_STATE) != 5 && read::<u8>(r + R_VARIANT) == 1)
        .collect();
    others.iter().for_each(|&r| write::<u8>(r + R_VARIANT, 0));
    let step: unsafe extern "C" fn(usize) = core::mem::transmute(read::<usize>(read::<usize>(mgr) + OWL_PHASE_STEP));
    for _ in 0..n {
        step(mgr);
    }
    others.iter().for_each(|&r| write::<u8>(r + R_VARIANT, 1));
}

/// Swaps `obj`'s vtable entry at `slot` for `detour`, keeping the original in
/// `orig`. Idempotent; the vtable is shared by every object of the class.
unsafe fn patch(obj: usize, slot: usize, detour: usize, orig: &AtomicUsize) -> bool {
    use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};
    if obj == 0 {
        return false;
    }
    let entry = read::<usize>(obj) + slot;
    let cur: usize = read(entry);
    if cur == detour {
        return true;
    }
    let mut old = 0;
    if VirtualProtect(entry as *const _, 8, PAGE_READWRITE, &mut old) == 0 {
        return false;
    }
    orig.store(cur, Ordering::Relaxed);
    write(entry, detour);
    VirtualProtect(entry as *const _, 8, old, &mut old);
    true
}

/// Installs the wrappers once the level scene's systems exist.
pub fn install(world: World) {
    static DONE: std::sync::Once = std::sync::Once::new();
    let scene: usize = unsafe { read(world.as_ptr() + SCENE) };
    if scene == 0 || DONE.is_completed() {
        return;
    }
    DONE.call_once(|| unsafe {
        let sys = |off: usize| read::<usize>(scene + off);
        let ok = [
            patch(sys(OWL), 0x88, owl_spawn as *const () as usize, &OWL_SPAWN),
            patch(sys(TRILO), 0x98, trilo_spawn as *const () as usize, &TRILO_SPAWN),
            patch(sys(SNAKE), 0xa0, snake_spawn as *const () as usize, &SNAKE_SPAWN),
            patch(sys(SPIDER), 0x80, spider_spawn as *const () as usize, &SPIDER_SPAWN),
            patch(sys(COLLISION), 0xa0, place_detour as *const () as usize, &PLACE),
            patch(sys(COLLISION), 0xa8, place_wall_detour as *const () as usize, &PLACE_WALL),
        ];
        if ok.iter().all(|&b| b) {
            log_info!("spawn wrappers installed");
        } else {
            log_error!("could not install every spawn wrapper ({ok:?}); positions and phases may not apply");
        }
    });
}
