//! Run spawning and arena rules from `<game>/mods/waves/waves.cfg` ([`cfg`]).
//!
//! The game spawns a run from one table of waves (`World::SetMode(2)`
//! decrypts it into the World at the dagger pickup). The first waves start
//! on their own, and every later one starts when a wave enemy dies. This mod
//! leaves that progression to the game and changes what it works with:
//!
//! * the table: rewritten on the first World tick of a run, before any
//!   wave started ([`run::set_table`]), with per-enemy spawn points and the
//!   boss's starting phase ([`spawns`]),
//! * an enemy cap: wave cursors with enemies left to spawn are held while
//!   too many are alive ([`run::hold_pending_waves`]),
//! * an optional wave timer that calls the game's own `StartWave`,
//! * the arena radius and whether it shrinks,
//! * when the two powerups (mercury, sulfur) appear and how long they stay.
//!
//! Nothing is touched outside a live run (lobby, replays, clips, trainings).
//! F7 shows a status panel, F8 reloads the file (it also reloads when the
//! file changes).

mod cfg;
mod run;
mod spawns;

use std::path::PathBuf;
use std::time::SystemTime;

use hd_sdk::input::{vk, KeyEdge};
use hd_sdk::prelude::*;

use crate::cfg::{Config, PowerupRule, POWERUPS};
use crate::run::Powerups;

/// Ticks between checks of the file's modification time.
const RELOAD_CHECK: u32 = 60;

struct Waves {
    path: PathBuf,
    cfg: Config,
    /// Last load error, shown in the panel.
    error: Option<String>,
    mtime: Option<SystemTime>,
    since_check: u32,
    show: bool,
    toggle: KeyEdge,
    reload: KeyEdge,
    /// The World, cached from its tick for the panel.
    world: Option<World>,
    /// Per-run state, reset on the first tick of each run.
    run: RunState,
}

#[derive(Default)]
struct RunState {
    /// Seconds of World ticks in this run.
    time: f32,
    next_wave: f32,
    next_powerup: [f32; 2],
    held: usize,
    /// Enemies alive at the last World tick (the panel shows it; enemy
    /// managers are only queried inside the tick).
    alive: i32,
    table_applied: bool,
}

impl Mod for Waves {
    const NAME: &'static str = "waves";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const DESCRIPTION: &'static str = "Spawn table, enemy cap, arena and powerups from mods/waves/waves.cfg";

    fn init() -> Self {
        let path = PathBuf::from(hd_sdk::mods_dir()).join("waves").join("waves.cfg");
        let mut m = Waves {
            path,
            cfg: Config::default(),
            error: None,
            mtime: None,
            since_check: 0,
            show: false,
            toggle: KeyEdge::new(vk::F7),
            reload: KeyEdge::new(vk::F8),
            world: None,
            run: RunState::default(),
        };
        m.load();
        m
    }

    fn tick(&mut self, _dt: f32) {
        if self.toggle.pressed() {
            self.show = !self.show;
        }
        self.since_check += 1;
        if self.reload.pressed() || (self.since_check >= RELOAD_CHECK && self.mtime != modified(&self.path)) {
            self.load();
        }
        if self.since_check >= RELOAD_CHECK {
            self.since_check = 0;
        }
        // The level scene moves the arena in its own tick, whose order
        // relative to the World tick is not known: hold it here as well.
        if let Some(g) = Game::get().filter(|&g| run::is_live_run(g)) {
            if !self.cfg.arena_shrink {
                run::hold_arena(g);
            }
        }
    }

    fn world_tick_pre(&mut self, world: World, _dt: f32) {
        self.world = Some(world);
        let Some(g) = Game::get().filter(|&g| run::is_live_run(g)) else { return };
        spawns::install(world);
        if run::is_fresh(world) {
            self.start_run(world);
        }
        if let Some(r) = self.cfg.arena_radius {
            run::set_arena_base(g, r);
        }
        if !self.cfg.arena_shrink {
            run::hold_arena(g);
        }
        self.run.held = 0;
        self.run.alive = run::enemies_alive(world);
        if self.cfg.enemy_cap > 0 && self.run.alive >= self.cfg.enemy_cap as i32 {
            self.run.held = run::hold_pending_waves(world);
        }
        self.block_powerups(world);
    }

    fn world_tick(&mut self, world: World, dt: f32) {
        let Some(g) = Game::get().filter(|&g| run::is_live_run(g)) else { return };
        self.run.time += dt;
        if self.cfg.wave_every > 0.0 && g.alive() && self.run.time >= self.run.next_wave {
            run::start_wave(world);
            self.run.next_wave = self.run.time + self.cfg.wave_every;
        }
        if !self.cfg.arena_shrink {
            run::hold_arena(g);
        }
        spawns::spawn_skulls(world);
        self.block_powerups(world);
        self.arm_powerups(g, world);
    }

    fn ui(&mut self, ui: &Ui) {
        if !self.show {
            return;
        }
        ui.window("##waves").flags(WindowFlags::OVERLAY).position([4.0, 120.0], [0.0, 0.0]).bg_alpha(0.5).show(|ui| {
            ui.text("WAVES (F7 hide, F8 reload)");
            match &self.error {
                Some(e) => ui.text(&format!("waves.cfg: {e}")),
                None => ui.text(&format!("table: {}", self.table_summary())),
            }
            let Some(g) = Game::get() else { return };
            if !run::is_live_run(g) {
                ui.text("not in a run");
                return;
            }
            let a = run::arena(g);
            ui.text(&format!("arena {:.1} / {:.1}{}", a.now, a.base, if self.cfg.arena_shrink { "" } else { " (held)" }));
            let Some(world) = self.world else { return };
            let cap = match self.cfg.enemy_cap {
                0 => String::new(),
                n => format!(" / cap {n}"),
            };
            ui.text(&format!(
                "waves started {}   enemies {}{cap}{}",
                run::waves_started(world),
                self.run.alive,
                if self.run.held > 0 { format!("   holding {}", self.run.held) } else { String::new() }
            ));
            if let Some(p) = Powerups::of(world) {
                for (k, name) in POWERUPS.iter().enumerate() {
                    let s = p.slot(k);
                    let state = if !s.armed {
                        "-".to_owned()
                    } else if g.time() < s.time {
                        format!("in {:.1}s", s.time - g.time())
                    } else if g.time() < s.time + s.lasts {
                        format!("up {:.1}s", s.time + s.lasts - g.time())
                    } else {
                        "gone".to_owned()
                    };
                    ui.text(&format!("{name}: {} {state}", rule_name(self.cfg.powerups[k].rule)));
                }
            }
        });
    }
}

impl Waves {
    fn load(&mut self) {
        self.mtime = modified(&self.path);
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(format!("{}: {e}", self.path.display()));
                log_warn!("{}: {e}; using the game's rules", self.path.display());
                self.cfg = Config::default();
                return;
            }
        };
        match cfg::parse(&text) {
            Ok(c) => {
                log_info!("loaded {}: table {}", self.path.display(), summary(&c));
                self.cfg = c;
                self.error = None;
            }
            // Keep the previous rules: a half-typed edit should not change a
            // run in progress.
            Err(e) => {
                log_warn!("{}: {e}; keeping the previous rules", self.path.display());
                self.error = Some(e);
            }
        }
    }

    /// First World tick of a run: no wave has started yet.
    fn start_run(&mut self, world: World) {
        self.run = RunState::default();
        self.run.next_wave = self.cfg.wave_every;
        for (k, p) in self.cfg.powerups.iter().enumerate() {
            if let PowerupRule::Every { after, .. } = p.rule {
                self.run.next_powerup[k] = after;
            }
        }
        spawns::set_plan(self.cfg.waves.clone());
        if let (Some(table), Some(waves)) = (self.cfg.table(), &self.cfg.waves) {
            if run::table(world) == table {
                self.run.table_applied = true;
            } else {
                match run::set_table(world, &table) {
                    Ok(()) => {
                        self.run.table_applied = true;
                        let names: Vec<String> =
                            waves.iter().map(|w| w.iter().map(|e| e.kind.name()).collect::<Vec<_>>().join(" ")).collect();
                        log_info!("run table: {}", names.join(" | "));
                    }
                    Err(e) => log_error!("{e}"),
                }
            }
        }
        if let Some(p) = Powerups::of(world) {
            for (k, cfg) in self.cfg.powerups.iter().enumerate() {
                p.set_lasts(k, cfg.lasts.unwrap_or(run::POWERUP_LASTS[k]));
            }
        }
    }

    fn block_powerups(&self, world: World) {
        let Some(p) = Powerups::of(world) else { return };
        for (k, cfg) in self.cfg.powerups.iter().enumerate() {
            if cfg.rule != PowerupRule::Game {
                p.block(k);
            }
        }
    }

    fn arm_powerups(&mut self, g: Game, world: World) {
        let Some(p) = Powerups::of(world) else { return };
        for (k, cfg) in self.cfg.powerups.iter().enumerate() {
            let PowerupRule::Every { every, .. } = cfg.rule else { continue };
            let s = p.slot(k);
            let busy = s.armed && g.time() < s.time + s.lasts + s.fade;
            if g.alive() && !busy && self.run.time >= self.run.next_powerup[k] {
                // The game also gives 3 s of warning before one appears.
                p.arm(g, k, g.time() + 3.0);
                self.run.next_powerup[k] = self.run.time + every;
            }
        }
    }

    fn table_summary(&self) -> String {
        let mut s = summary(&self.cfg);
        if self.run.table_applied {
            s.push_str(" (applied)");
        }
        s
    }
}

fn summary(c: &Config) -> String {
    match &c.waves {
        None => "game's".into(),
        Some(w) => format!("{} waves", w.len()),
    }
}

fn rule_name(r: PowerupRule) -> String {
    match r {
        PowerupRule::Game => "game".into(),
        PowerupRule::Off => "off".into(),
        PowerupRule::Every { every, .. } => format!("every {every}s"),
    }
}

fn modified(p: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

hd_sdk::hd_mod!(Waves);
