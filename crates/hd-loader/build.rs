//! Makes this cdylib a drop-in `winmm.dll` proxy: every export of the real
//! system winmm is re-exported as a PE forwarder to the absolute System32 path,
//! so the game (and OpenAL32.dll, which also imports winmm) keep working.
//!
//! `winmm_exports.txt` omits four legacy exports link.exe refuses to forward
//! (PlaySound, WOWAppExit, mciExecute, mmsystemGetVersion); nothing in the
//! game folder imports them.

fn main() {
    println!("cargo:rerun-if-changed=winmm_exports.txt");
    let exports = std::fs::read_to_string("winmm_exports.txt").unwrap();
    // timeBeginPeriod is implemented in lib.rs (it is the init trigger).
    for name in exports.lines().map(str::trim).filter(|l| !l.is_empty() && *l != "timeBeginPeriod") {
        println!(r"cargo:rustc-cdylib-link-arg=/EXPORT:{name}=C:\Windows\System32\winmm.{name}");
    }
}
