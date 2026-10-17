# hd-mods

**AI DISCLOSURE**

This project was created with the intention of exploring the reverse-engineering capabilities of Anthropic's frontier models. The code here is provided for demonstration purposes only... I have not done an extensive review on the quality of the output. Please proceed with caution. :)

---

A Rust modding platform for [HyperDemon](https://store.steampowered.com/app/1743850/)
(Steam build `7612dd72…`, PE timestamp `0x66ECD8B5`).

* **`hd-loader`** builds `winmm.dll`, a drop-in proxy the game loads from its
  own folder. It forwards every real winmm export to System32, then loads mods
  from `mods/*.dll` when the game enters its main loop.
* **`hd-sdk`** is what mods depend on: a `Mod` trait, per-tick, per-frame and
  HUD callbacks, inline hooks, the game's own Dear ImGui, and typed access to
  engine state.
* **`mods/example`** is the smallest possible mod: it loads, logs one line,
  and changes nothing in the game. Copy it to start your own.
* **`mods/tutorials`** runs TRAINING scenarios described in
  `<game>/trainings/*.training` files, including custom ones (see
  [Training scenarios](#training-scenarios)).
* **`mods/waves`** edits how a run spawns: the table of waves, an enemy
  cap, a wave timer, the arena radius and shrink, and the powerups, from
  `<game>/mods/waves/waves.cfg` (see [`docs/WAVES_FORMAT.md`](docs/WAVES_FORMAT.md)).
* **`docs/REVERSE_ENGINEERING.md`** holds everything learned about the binary,
  with addresses and evidence.

> **Status:** tested in-game, including the training injection. Every
> signature is verified against the installed exe (`cargo test`).

## Leaderboard policy

While **any** mod is loaded, the loader blocks every leaderboard write
(`add_score`, `add_replay`, `add_replay_thumb`, and all `add_clip*` uploads).
The end screen then shows *"Score not submitted (mods loaded)"*. Reading
leaderboards, watching replays and saving local replays all still work.

The guard **fails closed**. If a game update moves any upload function, the
loader refuses to load mods instead of risking a modded submission. With an
empty `mods` folder the loader changes nothing and the game runs unmodded.

## Install

### 1. Get the tools

* Windows, with HyperDemon installed from Steam.
* [Rust](https://rustup.rs/) with the MSVC toolchain, which is the default on
  Windows. Rustup will tell you if the Visual Studio C++ Build Tools are
  missing.
* Git, to clone the repo.

### 2. Build and install

```powershell
git clone <this repo>
cd hd-mods
cargo xtask install
```

`install` builds everything in release mode and copies the loader and every
mod into the game folder. Run it again after pulling changes or editing a mod.

By default it looks for the game in
`C:\Program Files (x86)\Steam\steamapps\common\hyperdemon`. If yours is in
another Steam library, find it in Steam with *right-click HyperDemon →
Manage → Browse local files*, then pass that folder:

```powershell
cargo xtask install --game-dir "D:\SteamLibrary\steamapps\common\hyperdemon"
# or set it once for this shell:
$env:HD_GAME_DIR = "D:\SteamLibrary\steamapps\common\hyperdemon"
```

If you get "access denied", the game is under `Program Files`. Run the command
from a terminal opened with *Run as administrator*.

### 3. Check it works

```powershell
cargo xtask status
```

Launch the game from Steam as usual, then open `<game>/mods/loader.log`. It
lists every mod that loaded (the `example` mod logs `loaded`) and every
signature that didn't match.

To run the signature tests against your copy of the game, run `cargo test`.
Set `HD_GAME_EXE` to the full path of `hyperdemon.exe` if it isn't at the
default Steam path; without the exe the tests are skipped.

### Uninstall

```powershell
cargo xtask uninstall
```

This removes `winmm.dll` only if it is ours. The `mods` folder is left in
place, but without the loader nothing in it runs and the game is unmodded.

### What gets installed

The game folder then looks like this:

```
hyperdemon/
  winmm.dll          loader
  mods/
    example.dll
    tutorials.dll
    waves.dll
    waves/waves.cfg  run spawn table, enemy cap, arena, powerups
    loader.ini       console / log_level / disabled mods
    loader.log       rewritten every launch
  trainings/         custom trainings (*.training + clips)
    builtin/         the game's own trainings, as data
```

A mod's `data/` folder is installed as `mods/<name>/`, and its `game/` folder
is merged into the game folder. Files already there are kept, so your edits
survive a reinstall.

To disable a mod, rename it to `_name.dll` or list it in `loader.ini`.

Reverse-engineering the game yourself needs extra tools (Ghidra, REA, Python).
Their setup is in [`tools/re/README.md`](tools/re/README.md).

## Writing a mod

Copy [`mods/example`](mods/example/) to `mods/my_mod` and rename it, or start
from scratch:

```powershell
cargo new --lib mods/my_mod
```

```toml
# mods/my_mod/Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
hd-sdk.workspace = true
```

```rust
use hd_sdk::prelude::*;

struct MyMod { runs: u32 }

impl Mod for MyMod {
    const NAME: &'static str = "my_mod";
    const VERSION: &'static str = "0.1.0";

    fn init() -> Self {
        log_info!("loaded");
        MyMod { runs: 0 }
    }

    // main thread, every fixed simulation step
    fn tick(&mut self, dt: f32) {}

    // main thread, inside the game's HUD ImGui frame
    fn ui(&mut self, ui: &Ui) {
        ui.window("##my_mod")
            .flags(WindowFlags::OVERLAY)
            .position([8.0, 8.0], [0.0, 0.0])
            .show(|ui| ui.text("hello"));
    }

    // render thread, GL context current, right before SwapBuffers
    fn frame(&mut self) {}

    // main thread, before an HTTP request is handed to the curl worker
    fn http_request(&mut self, req: &HttpRequest) {}

    // main thread, after the gameplay World ticked (during runs only)
    fn world_tick(&mut self, world: World, dt: f32) {}
}

hd_sdk::hd_mod!(MyMod);
```

Add the crate to `members` in the root `Cargo.toml`, then run
`cargo xtask install`.

### SDK overview

| Module | What it gives you |
|---|---|
| `hd_sdk::game` | `Game::get()`: speed, tick rate, fps cap, hit-stop, submit state, score, clock. `Bind::key_name()`: the key bound to an action. `App::get()`: the active scene pointer and vtable |
| `hd_sdk::imgui` | `Ui::window(...).flags().position().size().bg_alpha().show(...)`, `text`, `same_line`, `display_size`. These call the game's own ImGui, so you get its font and style |
| `hd_sdk::hook` | `Hook::install(target, detour)` / `original::<F>()` / `remove()`. Uses the loader's shared MinHook, so mods never fight over patches |
| `hd_sdk::mem` | `read`, `write`, `chain`, `rva`, `find_pattern`, `rip_relative` |
| `hd_sdk::hud` | Text drawn exactly like the game's HUD (its fonts, draw list, colours): `draw_centered`, `draw_at`, `text_width`, `training_panel`, `score_display` |
| `hd_sdk::world` | `World` (passed to `world_tick`): `count(kind)`, `spawn(kind)`, `spawn_at(kind, pos)`, `spawn_near_player` |
| `hd_sdk::training` | `active_clip()`, `player_in_control()`, `watching()`, `took_over()`, `stage()`, step counters (`COUNTERS`, `read_counter`, `write_counter`), `suppress_auto_complete()` / `allow_completion()`, `Injector` to add rows to the TRAINING menu |
| `hd_sdk::net` | `HttpRequest::url/set_url` (in `http_request`), `file_url(path)` |
| `hd_sdk::msvc` | read and assign the game's `std::string` (allocations go through the game's own `assign`) |
| `hd_sdk::audio` | `play(name)`: the game's sound effects (`"complete"`, ...) |
| `hd_sdk::input` | `is_down(vk)`, `KeyEdge` (only reports keys while the game window is focused) |
| `hd_sdk::addrs()` | every address the loader resolved (0 if a signature failed) |
| `log_info!` etc. | written to `mods/loader.log`, tagged with your mod's name |

Panics inside `init`, `tick`, `ui` or `frame` are caught. The panicking mod is
disabled and the game keeps running.

### ABI

Mods are plain `cdylib`s exporting `hdmod_entry(*const HostApi) ->
*const ModDescriptor`. Everything across that boundary is `#[repr(C)]` (see
`crates/hd-sdk/src/abi.rs`), so a mod doesn't need the loader's compiler
version. Mods in other languages work too if they implement that ABI.
`ABI_VERSION` (now 4) is checked on load, so mods must be rebuilt when it changes.

## How the loader hooks in

| Hook | Mechanism | Thread |
|---|---|---|
| init | our own `timeBeginPeriod` export (the game calls it once, right before its main loop) | main |
| `on_tick` | MinHook on the scene-tick delegate thunk | main |
| `on_ui` | MinHook on the scene-BuildUI delegate thunk (between `NewFrame` and `Render`) | main |
| `on_frame` | IAT patch of `GDI32!SwapBuffers` | render |
| `on_unload` | IAT patch of the CRT `exit` | main |
| `on_http_request` | MinHook on `HttpRequest::Submit` (before the curl worker reads the URL) | main |
| `on_world_tick_pre` / `on_world_tick` | MinHook on `World::Tick` (before / after the original) | main |
| `on_training_hud_begin` / `_end` | MinHook on the training panel (`FUN_140090150`), through a register-transparent wrapper | main |
| guard | MinHook on the 8 upload functions, plus `ImGui::TextV` for the end-screen line | main |

All targets are located by byte signatures, not fixed addresses. A minor game
patch that does not change these functions keeps working, and anything that
doesn't match is logged and disabled.

## Training scenarios

The built-in trainings are clips (`.hdclip`, the replay format) downloaded from
the game's server, plus objective logic hardcoded per clip id. `mods/tutorials`
describes trainings as data instead, in a small line-based format
([reference](docs/TRAINING_FORMAT.md)):

```text
training 0x4789E28C "ENEMY 1"
builtin

step "-Hit anywhere to defeat with daggers."                          until spawner_dagger_kill >= 1
step "-Hit the BIG GEM with a LASER to defeat instantly."             until spawner_gem_laser >= 1
step "-DOUBLE DAGGER JUMP up and touch the BIG GEM to do a GEM DASH." until spawner_gem_dash >= 1

keep 1 spawner every 1
```

All 13 built-in trainings ship in this format ([`mods/tutorials/game/trainings/builtin/`](mods/tutorials/game/trainings/builtin/))
and run from it: the mod hides the clip id from the game's hardcoded
per-training code while it runs, so editing a built-in's file changes the
built-in. Delete a built-in's file to get the game's own behaviour back.

To add a training:

1. Record the starting situation in-game and save it with the clip editor. It
   writes `%APPDATA%\HyperDemon\clips\<name>_<id>.hdclip`. This is optional:
   without a clip the training plays on the BUNNY HOP+FAKE clip, which is
   enough for drafting.
2. Copy the clip into `<game>/trainings/` and write a `.training` file
   next to it (start from [`boost_chain.training`](mods/tutorials/game/trainings/boost_chain.training)).
3. It shows up at the end of the TRAINING list. When you start it, the mod
   rewrites the download URL to `file://` and the game loads the clip like any
   built-in training. The mod then runs the steps, objectives, spawns and
   completion.

Spawn kinds are named after the trainings that use them (`spawner`, `scuttles`, `snakes`, `spiders`,
`skull3`, `gem`, `bomb`, `bluepowerup`) because their real identities haven't
been reversed yet.

## Not done yet

* Asset overrides. The lookup function is identified (`FUN_140211e70`), but
  hooking it needs the asset record layout reversed first.
* Gameplay structures (player, enemies, gems). The SDK exposes the scene
  pointer and the hook API to explore them, but no typed accessors yet.
* ImGui input widgets. The game doesn't feed mouse input to its HUD context.
