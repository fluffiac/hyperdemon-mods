//! Byte signatures for everything the loader touches in `hyperdemon.exe`.
//!
//! Generated from build SHA-256 7612dd72…3611 (PE timestamp 0x66ECD8B5): each
//! pattern is the shortest prefix of the function (min 24 bytes) that is unique
//! in `.text`, with rip-relative displacements and call targets wildcarded.
//! The comment on each entry is the function's address in that build. See
//! `docs/REVERSE_ENGINEERING.md` for how each one was identified.

use hd_sdk::abi::GameAddrs;

use crate::log::{log_error, log_info, log_warn};
use crate::pe::{parse_pattern, scan, Image};

pub struct Sig {
    pub name: &'static str,
    pub pattern: &'static str,
}

macro_rules! sig {
    ($name:literal, $pat:literal) => {
        Sig { name: $name, pattern: $pat }
    };
}

/// Online write endpoints. All are virtual methods on the game scene that
/// build and send one HTTP request; `void (this, ...)`, except
/// `add_replay_thumb` which returns 0 when it does nothing.
pub const UPLOADS: &[Sig] = &[
    // 0x14007d5a0  add_score.php        void(Game*, float score)
    sig!("add_score", "48 8B C4 55 56 57 41 54 41 55 41 56 41 57 48 8D A8 88 FD FF FF 48 81 EC 40 03 00 00"),
    // 0x14007f5a0  add_replay.php
    sig!("add_replay", "48 8B C4 55 56 57 41 54 41 55 41 56 41 57 48 8D A8 C8 FD FF FF 48 81 EC 00 03 00 00"),
    // 0x140081060  add_replay_thumb.php
    sig!("add_replay_thumb", "48 8B C4 55 57 41 56 48 8D 68 A1 48 81 EC 90 00 00 00 48 C7 45 17 FE FF FF FF"),
    // 0x140081530  add_clip.php
    sig!("add_clip", "40 55 56 57 41 56 41 57 48 8D 6C 24 C9 48 81 EC 90 00 00 00 48 C7 45 FF FE FF FF FF"),
    // 0x140081940  add_clip_segment.php
    sig!("add_clip_segment", "48 8B C4 55 41 54 41 55 41 56 41 57 48 8D A8 38 FF FF FF 48 81 EC A0 01 00 00 48 C7 45 08 FE FF FF FF"),
    // 0x1400820a0  add_clip_finalize.php
    sig!("add_clip_finalize", "48 8B C4 55 48 8D 68 A1 48 81 EC 90 00 00 00 48 C7 45 27 FE FF FF FF 48 89 58 10"),
    // 0x140082450  add_clip_thumb.php
    sig!("add_clip_thumb", "40 55 53 56 57 41 54 41 56 41 57 48 8D 6C 24 F1 48 81 EC 90 00 00 00 48 C7 45 B7 FE FF FF FF"),
    // 0x140085010  add_clip_vote.php
    sig!("add_clip_vote", "40 55 56 57 48 8D 6C 24 B9 48 81 EC 90 00 00 00 48 C7 45 0F FE FF FF FF"),
];

// 0x1401c7090: scene-tick delegate thunk. `mov rdx,rcx; mov rcx,[rcx+30h];
// mov rax,[rcx]; movss xmm1,[rdx+38h]; jmp [rax+18h]` -> Scene::Tick(dt).
const SCENE_TICK: &str = "48 8B D1 48 8B 49 30 48 8B 01 F3 0F 10 4A 38 48 FF 60 18 CC";
// 0x1401c70b0: same shape, `jmp [rax+20h]` -> Scene::BuildUI(dt), run between
// ImGui::NewFrame and ImGui::Render for the HUD context (FUN_1401c78f0).
const SCENE_UI: &str = "48 8B D1 48 8B 49 30 48 8B 01 F3 0F 10 4A 38 48 FF 60 20 CC";

const IMGUI_BEGIN: &str = "48 8B C4 48 89 50 10 48 89 48 08 55 56 48 8D A8 48 FE FF FF 48 81 EC A8 02 00 00"; // 0x1401d9550
const IMGUI_END: &str = "48 89 6C 24 18 57 48 83 EC 20 48 8B 3D ?? ?? ?? ?? 83 BF 60 1A 00 00 01"; // 0x1401dbba0
const IMGUI_TEXTV: &str = "40 57 48 83 EC 20 48 8B 3D ?? ?? ?? ?? 48 8B 87 88 1A 00 00 C6 80 80 00 00 00 01"; // 0x1401f4b90
const IMGUI_SAME_LINE: &str = "48 8B 15 ?? ?? ?? ?? 0F 28 D8 48 8B 82 88 1A 00 00 C6 80 80 00 00 00 01"; // 0x1401dcbd0
// 0x1401d6860 ImGui::Render; `mov rbx, [GImGui]` at +0x1c.
const IMGUI_RENDER: &str = "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57 48 83 EC 50 48 8B 1D ?? ?? ?? ??";
// 0x1401c71d0 App::App; `mov [g_app], rcx` at +0x97.
const APP_CTOR: &str = "48 89 54 24 10 48 89 4C 24 08 55 53 56 57 41 54 41 56 41 57 48 8B EC 48 83 EC 30";

// 0x14020d4b0 HttpRequest::Submit: hands a request to the curl worker.
const HTTP_SUBMIT: &str = "45 33 C0 4C 8B C9 41 8B C0 44 89 41 20 48 89 41 34 BA 80 00 00 00 48 89 41 2C";
// 0x1400dbb90 World::Tick(World*, float dt), vtable-only (entry at 0x140369180).
const WORLD_TICK: &str = "48 8B C4 48 89 58 18 55 56 57 41 54 41 55 41 56 41 57 48 8D 68 88 48 81 EC 40 01 00 00";
// 0x140090150: training panel + steps (called directly from the menu
// scene's BuildUI, 0x14008ea60).
const TRAINING_HUD: &str = "48 8B C4 55 41 54 41 55 41 56 41 57 48 8D A8 C8 F8 FF FF 48 81 EC 10 08 00 00 48 C7 45 98 FE FF FF FF 48 89 58 10 48 89";
// 0x140002fd0 std::string::assign(const char*, size_t)
const STRING_ASSIGN: &str = "48 89 5C 24 10 48 89 6C 24 18 56 57 41 57 48 83 EC 20 48 8B 69 18 49 8B F0";
// 0x140088990 vector<ClipEntry>::resize
const CLIP_LIST_RESIZE: &str = "4C 89 44 24 18 53 56 57 41 54 41 55 41 56 41 57 48 83 EC 60 48 C7 44 24 38 FE FF FF FF 48 8B F2 4C 8B E1 4C 8B 49 08 4C 8B 11 4D 8B C1 4D 2B C2 49 BD 67 66 66 66 66 66 66 66";
// 0x1400e0b40 World::SpawnNearPlayer(World*, int type, const float offset[3])
const WORLD_SPAWN_NEAR_PLAYER: &str = "4C 8B DC 49 89 73 20 55 49 8D AB 68 FF FF FF 48 81 EC 90 01 00 00 48 8B 05 ?? ?? ?? ??";

// Text drawing used by the game's HUD (e.g. training hints in FUN_140090150).
const FONT_RENDER_TEXT: &str = "48 89 54 24 10 53 56 57 41 55 41 56 48 81 EC 00 01 00 00 48 8B BC 24 68 01 00 00"; // 0x1401f2ec0
const FONT_CALC_TEXT_SIZE: &str = "48 8B C4 48 89 58 08 48 89 68 10 48 89 70 18 57 41 56 41 57 48 81 EC 90 00 00 00"; // 0x1401f2c80
const IMGUI_PUSH_FONT: &str = "48 89 5C 24 10 48 89 4C 24 08 57 48 83 EC 20 48 8B 3D ?? ?? ?? ?? 48 8B D9"; // 0x1401dc100
const IMGUI_POP_FONT: &str = "40 53 48 83 EC 20 48 8B 1D ?? ?? ?? ?? 48 8B 83 88 1A 00 00 48 8B 88 B8 02 00 00"; // 0x1401dc180

/// String the end screen shows for a failed upload; the guard rewrites it.
pub const SCORE_ERROR_TEXT: &[u8] = b"Score not submitted (error)\0";

#[derive(Default)]
pub struct Resolved {
    pub addrs: GameAddrs,
    /// Upload function addresses, parallel to [`UPLOADS`]; `None` if missing.
    pub uploads: Vec<Option<usize>>,
    pub scene_tick: Option<usize>,
    pub scene_ui: Option<usize>,
    pub http_submit: Option<usize>,
    pub world_tick: Option<usize>,
    pub training_hud: Option<usize>,
    pub score_error_text: Option<usize>,
}

fn find(img: &Image, name: &str, pattern: &str) -> Option<usize> {
    let pat = parse_pattern(pattern).expect("bad built-in pattern");
    let text = img.text();
    let hits = scan(text, &pat);
    match hits.as_slice() {
        [one] => Some(text.as_ptr() as usize + one),
        [] => {
            log_warn!("signature '{name}' not found (game updated?)");
            None
        }
        many => {
            log_warn!("signature '{name}' is ambiguous ({} matches)", many.len());
            None
        }
    }
}

/// Reads the target of a `48 8B/89 xx disp32` rip-relative mov at `insn`,
/// after checking its first `expect` bytes.
fn rip_mov_target(insn: usize, expect: &[u8]) -> Option<usize> {
    let bytes = unsafe { core::slice::from_raw_parts(insn as *const u8, 7) };
    if &bytes[..expect.len()] != expect {
        return None;
    }
    let disp = i32::from_le_bytes(bytes[3..7].try_into().unwrap());
    Some((insn + 7).wrapping_add_signed(disp as isize))
}

pub fn resolve(img: &Image) -> Resolved {
    let mut r = Resolved::default();
    let a = &mut r.addrs;

    r.uploads = UPLOADS.iter().map(|s| find(img, s.name, s.pattern)).collect();
    r.scene_tick = find(img, "scene_tick", SCENE_TICK);
    r.scene_ui = find(img, "scene_ui", SCENE_UI);
    r.http_submit = find(img, "http_submit", HTTP_SUBMIT);
    r.world_tick = find(img, "world_tick", WORLD_TICK);
    r.training_hud = find(img, "training_hud", TRAINING_HUD);
    a.string_assign = find(img, "string_assign", STRING_ASSIGN).unwrap_or(0);
    a.clip_list_resize = find(img, "clip_list_resize", CLIP_LIST_RESIZE).unwrap_or(0);
    a.font_render_text = find(img, "font_render_text", FONT_RENDER_TEXT).unwrap_or(0);
    a.font_calc_text_size = find(img, "font_calc_text_size", FONT_CALC_TEXT_SIZE).unwrap_or(0);
    a.imgui_push_font = find(img, "imgui_push_font", IMGUI_PUSH_FONT).unwrap_or(0);
    a.imgui_pop_font = find(img, "imgui_pop_font", IMGUI_POP_FONT).unwrap_or(0);
    a.world_spawn_near_player = find(img, "world_spawn_near_player", WORLD_SPAWN_NEAR_PLAYER).unwrap_or(0);

    a.imgui_begin = find(img, "imgui_begin", IMGUI_BEGIN).unwrap_or(0);
    a.imgui_end = find(img, "imgui_end", IMGUI_END).unwrap_or(0);
    a.imgui_textv = find(img, "imgui_textv", IMGUI_TEXTV).unwrap_or(0);
    a.imgui_same_line = find(img, "imgui_same_line", IMGUI_SAME_LINE).unwrap_or(0);

    if let Some(f) = find(img, "imgui_render", IMGUI_RENDER) {
        a.imgui_ctx_ptr = rip_mov_target(f + 0x1c, &[0x48, 0x8B, 0x1D]).unwrap_or(0);
    }
    if let Some(f) = find(img, "app_ctor", APP_CTOR) {
        a.app_ptr = rip_mov_target(f + 0x97, &[0x48, 0x89, 0x0D]).unwrap_or(0);
    }
    if let Some(f) = r.uploads[0] {
        a.game_ptr = rip_mov_target(f + 0x62, &[0x48, 0x8B, 0x0D]).unwrap_or(0);
    }
    r.score_error_text = img.find_rdata(SCORE_ERROR_TEXT);

    for (name, v) in [
        ("game_ptr", a.game_ptr),
        ("app_ptr", a.app_ptr),
        ("imgui_ctx_ptr", a.imgui_ctx_ptr),
    ] {
        if v == 0 {
            log_error!("could not resolve {name}");
        }
    }
    let found = r.uploads.iter().filter(|u| u.is_some()).count();
    log_info!(
        "signatures: uploads {found}/{}, tick {}, ui {}, imgui {}",
        UPLOADS.len(),
        ok(r.scene_tick.is_some()),
        ok(r.scene_ui.is_some()),
        ok(a.imgui_begin != 0 && a.imgui_end != 0 && a.imgui_textv != 0 && a.imgui_ctx_ptr != 0),
    );
    r
}

fn ok(b: bool) -> &'static str {
    if b {
        "ok"
    } else {
        "MISSING"
    }
}

#[cfg(test)]
mod tests {
    //! Checks every signature against the game binary on disk. Set
    //! `HD_GAME_EXE` to override the default Steam path; skipped if absent.

    use super::*;

    const DEFAULT_EXE: &str = r"C:\Program Files (x86)\Steam\steamapps\common\hyperdemon\hyperdemon.exe";

    /// Maps sections to their RVAs the way the Windows loader would.
    fn map_image() -> Option<Vec<u8>> {
        let path = std::env::var("HD_GAME_EXE").unwrap_or_else(|_| DEFAULT_EXE.into());
        let file = std::fs::read(path).ok()?;
        let rd32 = |o: usize| u32::from_le_bytes(file[o..o + 4].try_into().unwrap()) as usize;
        let rd16 = |o: usize| u16::from_le_bytes(file[o..o + 2].try_into().unwrap()) as usize;
        let nt = rd32(0x3c);
        let size = rd32(nt + 24 + 56);
        let headers = rd32(nt + 24 + 60);
        let mut img = vec![0u8; size];
        img[..headers].copy_from_slice(&file[..headers]);
        let first = nt + 24 + rd16(nt + 20);
        for i in 0..rd16(nt + 6) {
            let s = first + i * 40;
            let (va, raw_size, raw) = (rd32(s + 12), rd32(s + 16), rd32(s + 20));
            img[va..va + raw_size.min(size - va)].copy_from_slice(&file[raw..raw + raw_size.min(size - va)]);
        }
        Some(img)
    }

    #[test]
    fn all_signatures_resolve_uniquely() {
        let Some(mapped) = map_image() else {
            eprintln!("game exe not found; skipping");
            return;
        };
        let mapped = Box::leak(mapped.into_boxed_slice());
        let img = Image { base: mapped.as_ptr() as usize, size: mapped.len() };
        let r = resolve(&img);
        let rva = |a: usize| a - img.base + 0x1_4000_0000;

        let expected_uploads =
            [0x14007d5a0, 0x14007f5a0, 0x140081060, 0x140081530, 0x140081940, 0x1400820a0, 0x140082450, 0x140085010];
        for ((sig, got), want) in UPLOADS.iter().zip(&r.uploads).zip(expected_uploads) {
            assert_eq!(got.map(rva), Some(want), "{}", sig.name);
        }
        assert_eq!(r.scene_tick.map(rva), Some(0x1401c7090));
        assert_eq!(r.scene_ui.map(rva), Some(0x1401c70b0));
        assert_eq!(r.http_submit.map(rva), Some(0x14020d4b0));
        assert_eq!(r.world_tick.map(rva), Some(0x1400dbb90));
        assert_eq!(r.training_hud.map(rva), Some(0x140090150));
        assert_eq!(rva(r.addrs.string_assign), 0x140002fd0);
        assert_eq!(rva(r.addrs.clip_list_resize), 0x140088990);
        assert_eq!(rva(r.addrs.world_spawn_near_player), 0x1400e0b40);
        assert_eq!(rva(r.addrs.font_render_text), 0x1401f2ec0);
        assert_eq!(rva(r.addrs.font_calc_text_size), 0x1401f2c80);
        assert_eq!(rva(r.addrs.imgui_push_font), 0x1401dc100);
        assert_eq!(rva(r.addrs.imgui_pop_font), 0x1401dc180);
        let a = r.addrs;
        assert_eq!(rva(a.imgui_begin), 0x1401d9550);
        assert_eq!(rva(a.imgui_end), 0x1401dbba0);
        assert_eq!(rva(a.imgui_textv), 0x1401f4b90);
        assert_eq!(rva(a.imgui_same_line), 0x1401dcbd0);
        assert_eq!(rva(a.imgui_ctx_ptr), 0x1403d99e0);
        assert_eq!(rva(a.app_ptr), 0x1403d91d8);
        assert_eq!(rva(a.game_ptr), 0x1403d91f0);
        assert_eq!(r.score_error_text.map(rva), Some(0x1403663a8));
        // SDK runtime pattern (hd_sdk::audio::play).
        let pat = crate::pe::parse_pattern(hd_sdk::audio::PLAY_SIG).unwrap();
        let hits = crate::pe::scan(img.text(), &pat);
        assert_eq!(hits.len(), 1, "sound signature must be unique");
        assert_eq!(rva(img.text().as_ptr() as usize + hits[0]), 0x1401cb1a0);
        // SDK runtime pattern (hd_sdk::imgui::Ui::fill_backdrop).
        let pat = crate::pe::parse_pattern(hd_sdk::imgui::ADD_RECT_FILLED_SIG).unwrap();
        let hits = crate::pe::scan(img.text(), &pat);
        assert_eq!(hits.len(), 1, "AddRectFilled signature must be unique");
        assert_eq!(rva(img.text().as_ptr() as usize + hits[0]), 0x1401eec40);
        // SDK runtime patterns (hd_sdk::training::hide_clip_overlay).
        for (sig, want) in
            [(hd_sdk::training::INPUT_KEY_SIG, 0x140092980), (hd_sdk::training::SCRUB_BAR_SIG, 0x1400a4e20)]
        {
            let hits = crate::pe::scan(img.text(), &crate::pe::parse_pattern(sig).unwrap());
            assert_eq!(hits.len(), 1, "clip overlay signature {sig} must be unique");
            assert_eq!(rva(img.text().as_ptr() as usize + hits[0]), want);
        }
        // SDK runtime pattern (hd_sdk::game::Bind::key_name).
        let pat = crate::pe::parse_pattern(hd_sdk::game::KEY_NAME_TABLE_SIG).unwrap();
        let hits = crate::pe::scan(img.text(), &pat);
        assert_eq!(hits.len(), 1, "key-name table signature must be unique");
        let lea = img.text().as_ptr() as usize + hits[0] + 7;
        assert_eq!(rva(rip_mov_target(lea, &[0x4C, 0x8D, 0x2D]).unwrap()), 0x1403dc2b0);
        assert_eq!(img.timestamp(), crate::KNOWN_BUILD_TIMESTAMP);
        assert!(img.iat_slot("GDI32.dll", "SwapBuffers").is_some());
        assert!(img.iat_slot("api-ms-win-crt-runtime-l1-1-0.dll", "exit").is_some());
    }
}
