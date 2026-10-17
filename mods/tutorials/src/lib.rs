//! TRAINING scenarios as data.
//!
//! Each `<game>/trainings/*.training` file describes one training in a small
//! line-based language ([`dsl`], reference in `docs/TRAINING_FORMAT.md`):
//! its hint steps and their objectives, what to keep spawned, and for custom
//! trainings the clip and the TRAINING menu row. The built-in trainings live
//! in `<game>/trainings/builtin/`. The mod
//!
//! * adds custom rows to the TRAINING list ([`training::Injector`]),
//! * serves custom clips from disk by rewriting the game's download URL to
//!   `file://` (the game then loads them exactly like a built-in training),
//!   and a stand-in clip ([`STAND_IN_CLIP`]) to custom trainings without one,
//! * runs every training it has a file for, built-in ones included
//!   ([`runtime`]): hint panel, objectives, spawns, completion.
//!
//! The game's hardcoded per-training logic lives in two functions that
//! switch on the active clip id: the training panel (`FUN_140090150`) and the
//! World tick. While either runs, the mod swaps the id for
//! [`training::NEUTRAL_CLIP`], so the game takes its generic path (nothing
//! drawn, nothing spawned) and the data is the only logic left. The id is
//! put back right after each call, so everything else (menus, RETRY,
//! completion records) still sees the real one.

mod dsl;
mod runtime;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use hd_sdk::net::file_url;
use hd_sdk::prelude::*;
use hd_sdk::training::{self, Entry, Injector, CLIP_URL_PREFIX};

use crate::dsl::Training;
use crate::runtime::Run;

/// Stand-in for custom trainings without a clip file, next to the training
/// files: a short clip of the player standing still with nothing spawning.
/// The game only loads clips it can play back, and the body format is not
/// decoded, so the mod cannot make one (a zero-tick clip hangs the game); it
/// has to be recorded with the clip editor.
const STAND_IN_CLIP: &str = "stand_in.hdclip";

/// Served while [`STAND_IN_CLIP`] is missing: BUNNY HOP+FAKE from the game's
/// server, a movement drill with no enemies in its recording.
const FALLBACK_CLIP: u32 = 0x282B_60D5;

/// What a run is bound to; a new attempt starts when this changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Phase {
    clip: u32,
    watching: bool,
    in_control: bool,
}

struct Tutorials {
    trainings: Vec<Training>,
    entries: Vec<Entry>,
    injector: Injector,
    /// Index into `trainings` and the current attempt at it.
    run: Option<(usize, Phase, Run)>,
    /// Real clip id while it is hidden from the game's training code.
    hidden_clip: Option<i64>,
    /// Trainings whose last clip download got a stand-in clip.
    stand_ins: HashSet<u32>,
    /// `<game>/trainings/stand_in.hdclip`: served instead of [`FALLBACK_CLIP`]
    /// once it exists.
    stand_in_clip: PathBuf,
}

impl Mod for Tutorials {
    const NAME: &'static str = "tutorials";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const DESCRIPTION: &'static str = "TRAINING scenarios from <game>/trainings/*.training";

    fn init() -> Self {
        let mods = Path::new(hd_sdk::mods_dir());
        let dir = mods.parent().unwrap_or(mods).join("trainings");
        warn_old_dir(&mods.join("tutorials"), &dir);
        let mut files = training_files(&dir);
        files.extend(training_files(&dir.join("builtin")));
        let trainings = load_all(files);
        let custom = trainings.iter().filter(|t| !t.builtin).count();
        log_info!(
            "{} training(s) from {} ({custom} custom, {} built-in)",
            trainings.len(),
            dir.display(),
            trainings.len() - custom
        );
        let entries = trainings
            .iter()
            .filter(|t| !t.builtin)
            .map(|t| Entry {
                id: t.id,
                title: t.menu.title.clone(),
                subtitle: t.menu.subtitle.clone(),
                duration: t.menu.length,
            })
            .collect();
        Tutorials {
            trainings,
            entries,
            injector: Injector::default(),
            run: None,
            hidden_clip: None,
            stand_ins: HashSet::new(),
            stand_in_clip: dir.join(STAND_IN_CLIP),
        }
    }

    fn http_request(&mut self, req: &HttpRequest) {
        let url = req.url();
        let Some(rest) = url.strip_prefix(CLIP_URL_PREFIX) else { return };
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let Ok(id) = rest[..digits].parse::<u32>() else { return };
        let Some(t) = self.trainings.iter().find(|t| t.id == id && !t.builtin) else { return };
        let suffix = &rest[digits..];
        let local = if suffix == ".hdclip" { t.clip.as_ref() } else { t.thumbnail.as_ref() };
        let new = match local.filter(|p| p.is_file()) {
            Some(path) => {
                self.stand_ins.remove(&id);
                file_url(path)
            }
            None if suffix == ".hdclip" => {
                self.stand_ins.insert(id);
                if self.stand_in_clip.is_file() {
                    file_url(&self.stand_in_clip)
                } else {
                    format!("{CLIP_URL_PREFIX}{FALLBACK_CLIP}.hdclip")
                }
            }
            None => return,
        };
        log_info!("serving {url} from {new}");
        req.set_url(&new);
    }

    fn tick(&mut self, _dt: f32) {
        self.sync_run(true);
        // Every frame, so the sound comes back as soon as TRY IT is pressed
        // or the training is left.
        let hide = self.run.as_ref().is_some_and(|(_, _, run)| run.hides_demo());
        hd_sdk::audio::set_muted(hide);
        training::hide_clip_overlay(hide);
        // Without this the game treats a custom training as finished on the
        // first frame after TRY IT (it has no objectives for unknown ids).
        if let Some((_, _, run)) = &self.run {
            if run.holds_completion() {
                training::suppress_auto_complete();
            }
        }
    }

    fn world_tick_pre(&mut self, _world: World, _dt: f32) {
        self.sync_run(true);
        if self.run.is_some() {
            self.hide_clip();
        }
    }

    fn world_tick(&mut self, world: World, dt: f32) {
        self.restore_clip();
        self.sync_run(true);
        if let Some((i, _, run)) = &mut self.run {
            run.world_tick(&self.trainings[*i], world, dt);
        }
    }

    fn training_hud_begin(&mut self) {
        self.sync_run(false);
        if let Some((_, _, run)) = &mut self.run {
            // On the frame after completion the game must see the real id to
            // record it; its own logic then runs once more, with every step
            // already past.
            if !run.completion_frame() {
                self.hide_clip();
            }
        }
    }

    fn training_hud_end(&mut self) {
        self.restore_clip();
    }

    fn ui(&mut self, ui: &Ui) {
        self.injector.update(&self.entries);
        self.sync_run(false);
        if let Some((i, _, run)) = &mut self.run {
            run.ui(&self.trainings[*i], ui);
        }
    }
}

impl Tutorials {
    fn hide_clip(&mut self) {
        if self.hidden_clip.is_none() {
            self.hidden_clip = training::swap_active_clip(training::NEUTRAL_CLIP);
        }
    }

    fn restore_clip(&mut self) {
        if let Some(id) = self.hidden_clip.take() {
            training::swap_active_clip(id);
        }
    }

    /// Starts a new attempt whenever the active clip changes, its demo
    /// starts, the player (re)gains control (TRY IT, RETRY), or the game
    /// rebuilds the World (the demo loops). The last shows as the game's step
    /// counter at 0 outside the UI pass: the game only advances it in its UI,
    /// where the title row always takes it to at least 1 at once.
    fn sync_run(&mut self, outside_ui: bool) {
        let Some(clip) = training::active_clip() else {
            self.run = None;
            return;
        };
        let Some(i) = self.trainings.iter().position(|t| t.id == clip) else {
            self.run = None;
            return;
        };
        let phase = Phase {
            clip,
            watching: training::watching(),
            in_control: training::player_in_control() && Game::get().is_some_and(|g| g.alive()),
        };
        let world_reset = outside_ui && training::stage() == Some(0);
        let restart = match &self.run {
            Some((j, old, run)) => {
                *j != i
                    || old.clip != clip
                    || (phase.watching && !old.watching)
                    || (phase.in_control && !old.in_control)
                    || (world_reset && run.stage > 0)
            }
            None => true,
        };
        if restart {
            let stand_in = self.stand_ins.contains(&clip);
            self.run = Some((i, phase, Run::new(&self.trainings[i], stand_in)));
        } else if let Some((_, old, _)) = &mut self.run {
            *old = phase;
        }
    }
}

/// Every file directly in `dir`, sorted (created if missing).
fn training_files(dir: &Path) -> Vec<PathBuf> {
    let _ = std::fs::create_dir_all(dir);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    files.sort();
    files
}

/// Trainings used to live in `mods/tutorials/`; files left there are ignored.
fn warn_old_dir(old: &Path, new: &Path) {
    let stale = std::fs::read_dir(old)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "training"))
        .count();
    if stale > 0 {
        log_warn!("{}: {stale} .training file(s) ignored; trainings now load from {}", old.display(), new.display());
    }
}

fn load_all(files: Vec<PathBuf>) -> Vec<Training> {
    let mut out: Vec<Training> = Vec::new();
    for f in files {
        match f.extension().and_then(|e| e.to_str()) {
            Some("training") => {}
            Some("ini") => {
                log_warn!("{}: the .ini format was replaced by .training (docs/TRAINING_FORMAT.md); ignored", f.display());
                continue;
            }
            _ => continue,
        }
        match dsl::parse_file(&f) {
            Ok(t) if out.iter().any(|o| o.id == t.id) => {
                log_warn!("{}: id {:#010x} already used; skipped", f.display(), t.id)
            }
            Ok(t) => {
                if let Some(clip) = t.clip.as_ref().filter(|c| !c.is_file()) {
                    log_warn!("{}: clip {} not found; using a built-in clip until it exists", f.display(), clip.display());
                }
                out.push(t)
            }
            Err(e) => log_warn!("{}: {e}", f.display()),
        }
    }
    out
}

hd_sdk::hd_mod!(Tutorials);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_are_encoded() {
        let u = file_url(Path::new(r"C:\Program Files (x86)\Steam\a b.hdclip"));
        assert_eq!(u, "file:///C:/Program%20Files%20%28x86%29/Steam/a%20b.hdclip");
    }
}
