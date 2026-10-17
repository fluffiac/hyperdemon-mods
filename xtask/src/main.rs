//! `cargo xtask <command>`:
//!
//! * `install [--game-dir DIR]`   build release, copy `winmm.dll` and every
//!   workspace mod into the game folder, with its `data/` folder as
//!   `<game>/mods/<name>/` and its `game/` folder merged into `<game>/`
//! * `uninstall [--game-dir DIR]` remove the loader (mods folder is kept)
//! * `status [--game-dir DIR]`    show what is installed

use std::path::{Path, PathBuf};
use std::process::{exit, Command};

const DEFAULT_GAME_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\hyperdemon";
/// Marker compiled into the loader (its version log line); used to make sure
/// we only ever delete a winmm.dll we put there.
const LOADER_MARKER: &[u8] = b"hd-loader ";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let game_dir = args
        .windows(2)
        .find(|w| w[0] == "--game-dir")
        .map(|w| PathBuf::from(&w[1]))
        .or_else(|| std::env::var_os("HD_GAME_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_GAME_DIR));

    let result = match cmd {
        "install" => install(&game_dir),
        "uninstall" => uninstall(&game_dir),
        "status" => status(&game_dir),
        _ => {
            eprintln!("usage: cargo xtask <install|uninstall|status> [--game-dir DIR]");
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        exit(1);
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn check_game_dir(dir: &Path) -> Result<(), String> {
    if !dir.join("hyperdemon.exe").is_file() {
        return Err(format!("hyperdemon.exe not found in {} (use --game-dir)", dir.display()));
    }
    Ok(())
}

fn is_our_loader(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|b| b.windows(LOADER_MARKER.len()).any(|w| w == LOADER_MARKER))
}

/// Mod crates are the workspace members under `mods/`.
fn mod_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(workspace_root().join("mods"))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("Cargo.toml").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

fn install(game_dir: &Path) -> Result<(), String> {
    check_game_dir(game_dir)?;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let ok = Command::new(cargo)
        .current_dir(workspace_root())
        .args(["build", "--release", "--workspace", "--exclude", "xtask"])
        .status()
        .map_err(|e| e.to_string())?
        .success();
    if !ok {
        return Err("build failed".into());
    }

    let out = workspace_root().join("target").join("release");
    let dest = game_dir.join("winmm.dll");
    if dest.exists() && !is_our_loader(&dest) {
        return Err(format!("{} exists and is not hd-loader; not overwriting", dest.display()));
    }
    copy(&out.join("winmm.dll"), &dest)?;

    let mods_dir = game_dir.join("mods");
    std::fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
    for name in mod_names() {
        copy(&out.join(format!("{name}.dll")), &mods_dir.join(format!("{name}.dll")))?;
        let src = workspace_root().join("mods").join(&name);
        // mods/<name>/data/* -> <game>/mods/<name>/
        copy_new(&src.join("data"), &mods_dir.join(&name))?;
        // mods/<name>/game/** -> <game>/**
        copy_new(&src.join("game"), game_dir)?;
    }
    println!("installed. Launch the game; the log is {}", mods_dir.join("loader.log").display());
    Ok(())
}

/// Copies the tree under `src` into `dest`, keeping files the user edited.
fn copy_new(src: &Path, dest: &Path) -> Result<(), String> {
    for e in std::fs::read_dir(src).into_iter().flatten().flatten() {
        let to = dest.join(e.file_name());
        if e.path().is_dir() {
            copy_new(&e.path(), &to)?;
        } else if to.exists() {
            println!("  kept existing {}", to.display());
        } else {
            std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
            copy(&e.path(), &to)?;
        }
    }
    Ok(())
}

fn uninstall(game_dir: &Path) -> Result<(), String> {
    check_game_dir(game_dir)?;
    let dest = game_dir.join("winmm.dll");
    if !dest.exists() {
        println!("loader not installed");
        return Ok(());
    }
    if !is_our_loader(&dest) {
        return Err(format!("{} is not hd-loader; leaving it alone", dest.display()));
    }
    std::fs::remove_file(&dest).map_err(|e| format!("removing {}: {e}", dest.display()))?;
    println!("removed {} (mods folder kept)", dest.display());
    Ok(())
}

fn status(game_dir: &Path) -> Result<(), String> {
    check_game_dir(game_dir)?;
    let dest = game_dir.join("winmm.dll");
    println!(
        "loader: {}",
        if !dest.exists() {
            "not installed"
        } else if is_our_loader(&dest) {
            "installed"
        } else {
            "foreign winmm.dll present"
        }
    );
    let mods_dir = game_dir.join("mods");
    for e in std::fs::read_dir(&mods_dir).into_iter().flatten().flatten() {
        println!("  {}", e.file_name().to_string_lossy());
    }
    Ok(())
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to).map(|_| println!("  {} -> {}", from.display(), to.display())).map_err(|e| {
        let hint = if e.kind() == std::io::ErrorKind::PermissionDenied {
            " (is the game running, or does this folder need an elevated shell?)"
        } else {
            ""
        };
        format!("copying {} -> {}: {e}{hint}", from.display(), to.display())
    })
}
