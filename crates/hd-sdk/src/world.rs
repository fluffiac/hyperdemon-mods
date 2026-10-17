//! The gameplay World and its enemy managers.
//!
//! Everything here mirrors what the built-in training scenarios do in the
//! World tick (`FUN_1400dbb90`): count the things of one kind, spawn another.
//! Kinds are named after the training that uses them, because their in-game
//! identity has not been reversed yet. Only valid inside
//! [`crate::Mod::world_tick`] (main thread, World alive).

use crate::game::Game;
use crate::mem::{read, write};

/// `World+0x130`: entity container holding one manager per kind.
const ENTITIES: usize = 0x130;
/// `entities+0x48`: the single POWERUP object (`+0x28` scheduled time,
/// `+0x30` armed flag).
const POWERUP: usize = 0x48;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spawnable {
    /// Manager the DODGE training counts before spawning its enemy (35 units
    /// ahead of the player). Runtime logs show this count climbing in bursts
    /// of ~30 ms during normal play, so it may track projectiles rather than
    /// one enemy type; treat the name as provisional.
    Skull3,
    /// ENEMY 1 training: "Damage the enemy to STUN it".
    Spawner,
    /// ENEMY 2 training: "Hit the heads to deal damage".
    Scuttles,
    /// ENEMY 3 training; POWERUPS calls it the SNAKE.
    Snakes,
    /// ENEMY 4 training: "DROP to the ground / RISING".
    Spiders,
    /// Practice target used by RAPID+SHOT, LASER, GRAB and POWERUPS.
    Gem,
    /// Grabbable bomb of the GRAB+THROW training (`entities+0xd0`).
    Bomb,
    /// The POWERUP pickup. There is only one: "spawning" re-arms it 3 s
    /// ahead of the game clock and it counts as alive for 12 s after that.
    BluePowerup,
}

impl Spawnable {
    pub const ALL: [Spawnable; 8] = [
        Self::Skull3,
        Self::Spawner,
        Self::Scuttles,
        Self::Snakes,
        Self::Spiders,
        Self::Gem,
        Self::Bomb,
        Self::BluePowerup,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Skull3 => "skull3",
            Self::Spawner => "spawner",
            Self::Scuttles => "scuttles",
            Self::Snakes => "snakes",
            Self::Spiders => "spiders",
            Self::Gem => "gem",
            Self::Bomb => "bomb",
            Self::BluePowerup => "bluepowerup",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name().eq_ignore_ascii_case(s))
    }

    /// Where the built-in trainings put it, for kinds that take a position
    /// (for `Skull3` an offset from the player).
    pub fn default_position(self) -> Option<[f32; 3]> {
        match self {
            Self::Skull3 => Some([0.0, 0.0, -35.0]),
            Self::Gem => Some([0.0, 10.0, -20.0]),
            Self::Bomb => Some([-10.0, 10.0, -20.0]),
            _ => None,
        }
    }

    /// (manager offset in the entity container, vtable offset of `count()`)
    fn manager(self) -> Option<(usize, usize)> {
        Some(match self {
            Self::Skull3 => (0x78, 0xa8),
            Self::Spawner => (0x98, 0x98),
            Self::Scuttles => (0x80, 0x90),
            Self::Snakes => (0x88, 0x88),
            Self::Spiders => (0xc8, 0x90),
            Self::Gem => (0xa0, 0xd0),
            Self::Bomb => (0xd0, 0xc0),
            Self::BluePowerup => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct World(usize);

unsafe fn vfn(obj: usize, slot: usize) -> usize {
    read::<usize>(read::<usize>(obj) + slot)
}

impl World {
    pub fn from_ptr(p: usize) -> Option<Self> {
        (p != 0).then_some(Self(p))
    }

    pub fn as_ptr(self) -> usize {
        self.0
    }

    fn entities(self) -> Option<usize> {
        let e: usize = unsafe { read(self.0 + ENTITIES) };
        (e != 0).then_some(e)
    }

    fn manager(self, kind: Spawnable) -> Option<usize> {
        let m: usize = unsafe { read(self.entities()? + kind.manager()?.0) };
        (m != 0).then_some(m)
    }

    fn powerup(self) -> Option<usize> {
        let p: usize = unsafe { read(self.entities()? + POWERUP) };
        (p != 0).then_some(p)
    }

    /// Number of live things of `kind`.
    pub fn count(self, kind: Spawnable) -> i32 {
        if kind == Spawnable::BluePowerup {
            // World tick, POWERUPS: alive while armed and clock < time + 12.
            let (Some(p), Some(g)) = (self.powerup(), Game::get()) else { return 0 };
            let (armed, at) = unsafe { (read::<u8>(p + 0x30) != 0, read::<f32>(p + 0x28)) };
            return i32::from(armed && g.time() < at + 12.0);
        }
        let (Some(m), Some((_, slot))) = (self.manager(kind), kind.manager()) else { return 0 };
        unsafe {
            let f: unsafe extern "C" fn(usize) -> i32 = core::mem::transmute(vfn(m, slot));
            f(m)
        }
    }

    /// Spawn one of `kind` the way the matching training does, at its
    /// default position.
    pub fn spawn(self, kind: Spawnable) {
        self.spawn_at(kind, kind.default_position().unwrap_or_default());
    }

    /// Spawn one of `kind`. `pos` is used by `Gem`, `Bomb` and `Skull3`
    /// (world position; the skull's is raised by 2); other kinds pick their
    /// own spot.
    pub fn spawn_at(self, kind: Spawnable, pos: [f32; 3]) {
        if kind == Spawnable::Skull3 {
            return self.spawn_near_player(2, pos);
        }
        if kind == Spawnable::BluePowerup {
            let (Some(p), Some(g)) = (self.powerup(), Game::get()) else { return };
            unsafe {
                write::<f32>(p + 0x28, g.time() + 3.0);
                write::<u8>(p + 0x30, 1);
            }
            return;
        }
        let Some(m) = self.manager(kind) else { return };
        unsafe {
            match kind {
                // Integer arguments are passed as full 64-bit values exactly as
                // the game's call sites leave them (`mov r9d, -1` zero-extends):
                // the callees read whole registers that Ghidra's prototypes
                // omit, and garbage there makes them throw.
                Spawnable::Spawner => {
                    // FUN_1400e0ef0: vtbl+0x88(mgr, &out, variant, -1, 0). The game
                    // passes its 32-byte home area as `out`; match that size.
                    #[repr(C, align(16))]
                    struct Out([u8; 32]);
                    let mut out = Out([0; 32]);
                    let f: unsafe extern "C" fn(usize, *mut Out, u64, u64, u64) =
                        core::mem::transmute(vfn(m, 0x88));
                    f(m, &mut out, 0, 0xffff_ffff, 0);
                }
                Spawnable::Scuttles | Spawnable::Snakes | Spawnable::Spiders => {
                    // World tick 0x1400dc2eb / 0x1400dc75d / 0x1400dc837:
                    // `xor r9d,r9d; xor edx,edx; lea r8d,[r9-1]`, i.e. four
                    // arguments (mgr, 0, -1, 0). Leaving r9 out threw
                    // std::length_error("vector<T> too long").
                    let slot = match kind {
                        Spawnable::Scuttles => 0x98,
                        Spawnable::Snakes => 0xa0,
                        _ => 0x80,
                    };
                    let f: unsafe extern "C" fn(usize, u64, u64, u64) = core::mem::transmute(vfn(m, slot));
                    f(m, 0, 0xffff_ffff, 0);
                }
                Spawnable::Gem | Spawnable::Bomb => {
                    // vtbl+0x80(mgr, &pos, &vel) for both (GRAB+THROW calls the
                    // bomb manager's devirtualised copy of the same slot).
                    let vel = [0.0f32; 3];
                    let f: unsafe extern "C" fn(usize, *const f32, *const f32) =
                        core::mem::transmute(vfn(m, 0x80));
                    f(m, pos.as_ptr(), vel.as_ptr());
                }
                Spawnable::Skull3 | Spawnable::BluePowerup => unreachable!(),
            }
        }
    }

    /// `FUN_1400e0b40`: spawn skull `kind_id` at about `pos + (0, 2, 0)`
    /// (plus a random unit vector, `FUN_1401c8bc0`; despite the name it does
    /// not use the player position). Kinds: 0 a 1-HP skull, 1 the same at a
    /// random point around (0, 15, 0) (ignores `pos`), 2 a 10-HP skull, 3 a
    /// 100-HP skull. Called directly by the game with rcx, edx, r8: plain
    /// Win64.
    pub fn spawn_near_player(self, kind_id: u32, offset: [f32; 3]) {
        let f = crate::addrs().world_spawn_near_player;
        if f != 0 {
            unsafe {
                let f: unsafe extern "C" fn(usize, u64, *const f32) = core::mem::transmute(f);
                f(self.0, u64::from(kind_id), offset.as_ptr());
            }
        }
    }
}
