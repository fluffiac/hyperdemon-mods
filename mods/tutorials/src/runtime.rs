//! Runs a [`Training`] the way the game ran its built-in ones.
//!
//! Mirrors `FUN_140090150` (hint panel, steps, completion) and the
//! per-training branch of the World tick `FUN_1400dbb90` (spawns, score
//! countdown). The game's own per-training code is kept out of the way by
//! hiding the clip id while it runs (see `lib.rs`), so every training,
//! built-in or custom, is driven by its data alone.

use hd_sdk::game::{Bind, Game};
use hd_sdk::hud::{self, HintRow};
use hd_sdk::prelude::*;
use hd_sdk::training;

use crate::dsl::{Training, MAX_STEPS};

/// State of one attempt (demo or run) at a training.
pub struct Run {
    /// Hint rows passed, like the game's `World+0x84`: row 0 is the title,
    /// rows 1..=5 the steps, so step `k` is current while `stage == k` and
    /// the training is done once `stage > 5`.
    pub stage: i32,
    /// All steps done; completion handed to the game.
    completed: bool,
    /// The game has had its frame with the real clip id to record the
    /// completion (see [`Run::completion_frame`]).
    completion_recorded: bool,
    /// Per spawn rule: seconds until it may spawn again.
    timers: Vec<f32>,
    /// Playing on the fallback clip (the training has no clip of its own):
    /// its demo is someone else's recording, so it is hidden and silenced
    /// and nothing it does counts.
    pub stand_in: bool,
}

impl Run {
    pub fn new(t: &Training, stand_in: bool) -> Self {
        Run { stage: 0, completed: false, completion_recorded: false, timers: vec![0.0; t.spawns.len()], stand_in }
    }

    /// The stand-in clip's demo is playing: black screen, no sound, no progress.
    pub fn hides_demo(&self) -> bool {
        self.stand_in && training::watching()
    }

    /// Keeps the game from completing the training on its own until its steps
    /// are done (with the clip id hidden it sees no objectives at all).
    pub fn holds_completion(&self) -> bool {
        !self.completed
    }

    /// True once, on the frame after the steps were done: the game's panel
    /// code must then see the real clip id, because that is where it records
    /// the completion (`menu+0x3b0[id] = 1`, then saves).
    pub fn completion_frame(&mut self) -> bool {
        if self.completed && !self.completion_recorded {
            self.completion_recorded = true;
            return true;
        }
        false
    }

    /// Per frame, from `Mod::ui`: advance steps, complete, draw.
    pub fn ui(&mut self, t: &Training, ui: &Ui) {
        let took_over = training::took_over();
        if self.hides_demo() {
            ui.fill_backdrop(0xff00_0000);
            for step in &t.steps {
                if let Some(g) = &step.goal {
                    let mut offs = Vec::new();
                    g.expr.counters(&mut offs);
                    offs.into_iter().for_each(|o| training::write_counter(o, 0));
                }
            }
        }
        self.advance(t, took_over);
        if t.has_objective() && self.stage > MAX_STEPS as i32 && took_over && !self.completed {
            log_info!("training {:#010x} complete", t.id);
            self.completed = true;
            training::allow_completion();
        }
        if self.holds_completion() {
            training::suppress_auto_complete();
        }
        self.draw(t, took_over);
    }

    /// Per World tick: spawns and the score countdown.
    pub fn world_tick(&mut self, t: &Training, world: World, dt: f32) {
        if self.holds_completion() {
            training::suppress_auto_complete();
        }
        let Some(g) = Game::get() else { return };
        if t.drain_score && g.alive() {
            g.set_score(g.score() - dt);
        }
        if !training::player_in_control() {
            return;
        }
        for (sp, timer) in t.spawns.iter().zip(&mut self.timers) {
            *timer -= dt;
            if sp.after > 0 && self.stage <= sp.after as i32 {
                continue;
            }
            if world.count(sp.kind) >= sp.count {
                *timer = sp.every;
            } else if *timer <= 0.0 {
                world.spawn_at(sp.kind, sp.at.or(sp.kind.default_position()).unwrap_or_default());
                *timer = sp.every;
            }
        }
    }

    /// The game's step loop: rows pass in order once `progress >= target`
    /// (or, during the demo, once progress is above 0). Rows without an
    /// objective pass as soon as they are reached. Counters of steps not
    /// reached yet are held at 0 so earlier play does not count.
    fn advance(&mut self, t: &Training, took_over: bool) {
        let rows = rows(t, took_over);
        for (k, step) in t.steps.iter().enumerate() {
            if self.stage < k as i32 + 1 {
                if let Some(g) = &step.goal {
                    let mut offs = Vec::new();
                    g.expr.counters(&mut offs);
                    offs.into_iter().for_each(|o| training::write_counter(o, 0));
                }
            }
        }
        for (i, &(progress, target)) in rows.iter().enumerate() {
            if i as i32 == self.stage && (target <= progress || (progress > 0 && !took_over)) {
                self.stage += 1;
                if target != 0 {
                    hd_sdk::audio::play("complete");
                }
            }
        }
    }

    fn draw(&self, t: &Training, took_over: bool) {
        let texts: Vec<String> = t
            .steps
            .iter()
            .map(|s| if s.playing_only && !took_over { String::new() } else { s.text.render(key_name) })
            .collect();
        let progress = rows(t, took_over);
        let hint_rows: Vec<HintRow> = texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let (p, target) = progress[i + 1];
                HintRow { text, progress: (target > 0).then_some((p, target)) }
            })
            .collect();
        hud::training_panel(&t.title, &hint_rows, self.stage, took_over);
        if t.show_score {
            if let Some(g) = Game::get() {
                hud::score_display(g.score(), training::score_pulse());
            }
            // The score training's panel makes this call right after the score.
            training::world_hud();
        }
    }
}

/// `(progress, target)` for the title row and rows 1..=5, like the game's
/// local array. Steps hidden until the player takes over count as empty.
fn rows(t: &Training, took_over: bool) -> [(i32, i32); MAX_STEPS + 1] {
    let mut rows = [(0, 0); MAX_STEPS + 1];
    for (i, step) in t.steps.iter().enumerate() {
        if step.playing_only && !took_over {
            continue;
        }
        if let Some(g) = &step.goal {
            let score = || Game::get().map_or(0.0, |g| g.score());
            rows[i + 1] = (g.expr.eval(&training::read_counter, &score), g.target);
        }
    }
    rows
}

/// Name of the key bound to `bind`, as the built-in hints print it.
fn key_name(bind: Bind) -> String {
    bind.key_name().unwrap_or_else(|| bind.name().to_uppercase())
}
