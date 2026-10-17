//! Game sound effects, and muting everything the game plays.

use crate::mem::read;

/// `FUN_1401cb1a0`, the game's sound-effect player. Inputs: `rdx` the name
/// (`const std::string&`), `xmm2` and `[rsp+0x28]` floats, `r9b` a flag;
/// `rcx` is only scratch for it. The training panel plays its step sound
/// with `("complete", 1.0, false, 1.0)`.
#[doc(hidden)]
pub const PLAY_SIG: &str =
    "48 8B C4 48 89 48 08 57 48 83 EC 70 48 C7 40 C8 FE FF FF FF 48 89 58 10 48 89 70 18 0F 29 70 E8 41 0F B6 F1 0F 28 F2 48 8B DA";

/// MSVC `std::string` with the name in its small-string buffer.
#[repr(C)]
struct SsoString {
    buf: [u8; 16],
    size: usize,
    capacity: usize,
}

fn play_fn() -> Option<usize> {
    static F: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let f = *F.get_or_init(|| unsafe { (crate::host().find_pattern)(crate::abi::StrRef::of(PLAY_SIG)) });
    (f != 0).then_some(f)
}

/// Plays the sound effect `name` (at most 15 bytes) the way the training
/// panel plays "complete".
pub fn play(name: &str) {
    let Some(f) = play_fn() else { return };
    if name.len() > 15 || read_game() == 0 {
        return;
    }
    let mut s = SsoString { buf: [0; 16], size: name.len(), capacity: 15 };
    s.buf[..name.len()].copy_from_slice(name.as_bytes());
    unsafe {
        let f: unsafe extern "C" fn(usize, *const SsoString, f32, u64, u64, f32) = core::mem::transmute(f);
        f(0, &s, 1.0, 0, 0, 1.0);
    }
}

fn read_game() -> usize {
    let pp = crate::addrs().game_ptr;
    if pp == 0 {
        0
    } else {
        unsafe { read(pp) }
    }
}

// The game plays everything (effects and music) through OpenAL, imported
// from `OpenAL32.dll` next to the exe.
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleA(name: *const core::ffi::c_char) -> usize;
    fn GetProcAddress(module: usize, name: *const core::ffi::c_char) -> usize;
}

const AL_GAIN: i32 = 0x100A;

type ListenerfFn = unsafe extern "C" fn(param: i32, value: f32);
type GetListenerfFn = unsafe extern "C" fn(param: i32, value: *mut f32);

struct Al {
    listenerf: ListenerfFn,
    get_listenerf: GetListenerfFn,
}

fn al() -> Option<&'static Al> {
    static AL: std::sync::OnceLock<Option<Al>> = std::sync::OnceLock::new();
    AL.get_or_init(|| unsafe {
        let m = GetModuleHandleA(c"OpenAL32.dll".as_ptr());
        if m == 0 {
            return None;
        }
        let set = GetProcAddress(m, c"alListenerf".as_ptr());
        let get = GetProcAddress(m, c"alGetListenerf".as_ptr());
        (set != 0 && get != 0).then(|| Al {
            listenerf: core::mem::transmute::<usize, ListenerfFn>(set),
            get_listenerf: core::mem::transmute::<usize, GetListenerfFn>(get),
        })
    })
    .as_ref()
}

/// The listener gain to restore while muted.
static SAVED_GAIN: std::sync::Mutex<Option<f32>> = std::sync::Mutex::new(None);

/// Silences every sound the game plays, music included, by holding the
/// OpenAL listener gain at 0; unmuting restores the gain from before. Call
/// it every frame with the state you want: the gain is re-applied each time,
/// in case the game sets it too.
pub fn set_muted(muted: bool) {
    let Some(al) = al() else { return };
    let mut saved = SAVED_GAIN.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        match (muted, *saved) {
            (true, None) => {
                let mut gain = 1.0;
                (al.get_listenerf)(AL_GAIN, &mut gain);
                *saved = Some(gain);
                (al.listenerf)(AL_GAIN, 0.0);
            }
            (true, Some(_)) => (al.listenerf)(AL_GAIN, 0.0),
            (false, Some(gain)) => {
                (al.listenerf)(AL_GAIN, gain);
                *saved = None;
            }
            (false, None) => {}
        }
    }
}
