# HyperDemon reverse-engineering notes

Target: `hyperdemon.exe`, x86-64 PE, SHA-256
`7612dd72076002b7c0cb5cab8f7340a399176ecedefe9d3b4d0353fbdac93611`, PE
timestamp `0x66ECD8B5`, image base `0x140000000`. Analysed statically with
[REA](https://github.com/morluto/rea) 6.4.0 driving Ghidra 12.1.3 (overview
Evidence `ev_1c5345f4582028694eb267bf79126be7f780432cd8b95bd31fb5b60f0fb583fb`:
8,644 procedures, 5,895 strings), plus small Python scanners in `tools/re/`.

Everything here is static analysis. Pseudocode is Ghidra's reconstruction, and
names are mine. Nothing was observed at runtime.

## What the game is built from

| Component | Evidence |
|---|---|
| Custom C++ engine, MSVC, RTTI stripped (`/GR-`) | Only 4 RTTI type descriptors, all from the STL |
| GLFW 3 (window class `GLFW30`), OpenGL 3.3 core | GLFW error strings; `FUN_14020eb70` sets hints `0x22002=3, 0x22003=3, 0x22008=core` |
| Dear ImGui ≈1.7x–1.8x, statically linked, used for all UI | `Debug##Default`, `##MainMenuBar`, `imgui.ini`; `Begin` flag handling matches ≥1.7x |
| libcurl (static) + Schannel | curl strings, WS2_32/WLDAP32/CRYPT32 imports |
| FreeType, OpenAL, Steamworks, Discord Game SDK, Media Foundation (clip export) | strings and imports |
| One encrypted/compressed archive `asset/asset` (≈300 MB) | high-entropy header; opened by `FUN_140212490` |

## Process and threading model

```
WinMain-ish   FUN_140002980   chdir to exe dir, crash handler, then:
  FUN_140002750               SteamAPI_Init (else relaunch via steam://run/1743850),
                              single-instance mutex "HyperDemonInstanceLock"
  FUN_1401c31a0               App bootstrap: window (FUN_14020eb70), App ctor
                              (FUN_1401c71d0, 0x108 bytes, stored in DAT_1403d91d8)
    FUN_1401c33c0             MAIN LOOP (main thread)
      _beginthread(FUN_1402370e0)   worker
      timeBeginPeriod(1)            <- loader init trigger (only call site)
      loop:
        frame limiter: waitable timer, cap = Game+0x38 (int)
        accumulator += dt * Game+0x20 * Game+0x24 (hit-stop: * Game+0x34 while Game+0x30 > 0)
        up to 8x: fixed step 1/Game+0x2c  -> App[2] delegate -> Scene vtbl+0x18 (Tick)
        FUN_1401c78f0 (per-frame UI):
          FUN_1401cfe70 -> ImGui::NewFrame
          App[3] delegate -> Scene vtbl+0x20 (BuildUI)
          ImGui::Render under critical section, draw data handed to render thread
        App[5] delegate -> Scene vtbl+0x38 (Render, interpolation alpha)
Render thread FUN_1401c3070   waits for work, runs GL commands, then
  FUN_14020ee90 (present)     binds FBO 0 -> glfwSwapBuffers (FUN_14025a1d0) -> SwapBuffers
```

The App delegates are tiny thunks (Ghidra does not create functions for them):

| Thunk | Bytes | Target |
|---|---|---|
| `0x1401c7090` | `48 8B D1 48 8B 49 30 48 8B 01 F3 0F 10 4A 38 48 FF 60 18` | `scene->vtbl[3](scene, dt)` Tick |
| `0x1401c70b0` | same, `FF 60 20` | `vtbl[4](dt)` BuildUI |
| `0x1401c70d0` | `FF 60 28` | `vtbl[5]()` |
| `0x1401c70e0` | `FF 60 38`, `xmm2 = [rdx+40h]` | `vtbl[7](ctx, alpha)` Render |
| `0x1401c7100` | `FF 60 40` | `vtbl[8]` |

Each delegate object holds the scene at `+0x30` and the float argument at `+0x38`.

## Hooking caveat: non-standard register use

The exe was built with whole-program optimisation. For direct calls to small
functions, MSVC knows the callee's real register usage and keeps live values
in registers the Win64 ABI calls volatile. Confirmed case: the request retry
loop in `FUN_140072b90` holds the network manager in `r11` and a retry count
in `r10` across `call 0x14020d4b0` (`HttpRequest::Submit`, which touches only
`rax rcx rdx r8 r9`). A plain Rust detour on `Submit` clobbered `r11` and
crashed at `0x1400736f0` (`mov rax, [rax-8]`) the first time a download failed
and was retried.

Rule: any hook on a function the game calls **directly** must use the
register-preserving entry in `crates/hd-loader/src/preserve.rs`. Functions
reached only through vtables or delegates (scene Tick/UI, `World::Tick`, the
`add_*` upload methods) are called with the standard ABI.

## Globals

| Address | Meaning | Found via |
|---|---|---|
| `0x1403d91f0` | `Game*`, root game/session object | read by the main loop and by `add_score` (`mov rcx,[rip+…]` at `add_score+0x62`) |
| `0x1403d91d8` | `App*` | written in the App ctor (`mov [rip+…],rcx` at `+0x97`) |
| `0x1403d99e0` | `GImGui` (current `ImGuiContext*`) | first load in `ImGui::Render` (`+0x1c`); swapped by `FUN_1400a9cd0` |
| `0x1403daa60` | log `ostream` | `FUN_140213cb0` ("Can't find asset") |

### `Game` fields

| Offset | Type | Meaning |
|---|---|---|
| `0x20` | f32 | simulation speed (accumulator and audio pitch) |
| `0x24` | f32 | second speed factor (accumulator, hit-stop decay) |
| `0x28` | f32 | audio pitch factor (`FUN_1401ca7a0(…, +0x28 * +0x20, …)`) |
| `0x2c` | f32 | fixed tick rate, Hz |
| `0x30`/`0x34` | f32 | hit-stop timer / speed multiplier while active |
| `0x38` | i32 | frame-rate cap |
| `0x3c` | u8 | reset-accumulator flag |
| `0x40` | f32 | dt override when > 0 |
| `0x148` | u8 | "cheats suspected" (end screen) |
| `0x14c` | i32 | submit state: <0 error, 0 submitted, 1 uploading replay, >1 retry n |
| `0x150` | f32 | seconds to keep showing the submit line |
| `0x330` | u8 | "NEW HIGH SCORE!" |

## Online / leaderboards

Backend: `http://hyprd.mn/backend_dev/*.php`. Each request is one virtual
method on the scene class (vtable `0x140365668`, ctor `FUN_140089f30`), which
fills a request object and stores it at a fixed offset (`+0x228`…`+0x300`).
`FUN_140077330` (vtbl slot 3, Tick) polls these and dispatches responses.

| Endpoint | Function | Prototype |
|---|---|---|
| `add_score.php` | `0x14007d5a0` | `void (Game*, float score)` |
| `add_replay.php` | `0x14007f5a0` | `void (this, ?)` |
| `add_replay_thumb.php` | `0x140081060` | `u64 (this)`, returns 0 if nothing sent |
| `add_clip.php` | `0x140081530` | `void (this, ?)` |
| `add_clip_segment.php` | `0x140081940` | `void (this)` |
| `add_clip_finalize.php` | `0x1400820a0` | `void (this)` |
| `add_clip_thumb.php` | `0x140082450` | `void (this, …)` |
| `add_clip_vote.php` | `0x140085010` | `void (this, int)` |
| `get_scores.php` (read) | `0x1400848a0` | |

The score request is signed with obfuscated keys near `0x140365848`. These were
deliberately **not** analysed: the platform only ever *prevents* submissions.

The retry path in `FUN_140077330` (`state < 0` → negate, resend) only runs when
a request object exists at `+0x250`. The guard never creates one, so a blocked
score stays blocked.

## Dear ImGui functions

| Address | Function | Identified by |
|---|---|---|
| `0x1401d53d0` | `NewFrame` | `Begin("Debug##Default")`, "Click to break in debugger!" |
| `0x1401d9550` | `Begin(name, p_open, flags)` | `(flags & 0xC0200) == 0xC0200 → flags |= 6` (NoInputs ⇒ NoMove/NoResize) |
| `0x1401dbba0` | `End` | window-stack pop + `LogFinish` (fflush/fclose) |
| `0x1401f4b60` / `0x1401f4b90` | `Text(fmt, ...)` / `TextV` | varargs spill then `TextV(fmt, &args)` |
| `0x1401dcbd0` | `SameLine(offset, spacing)` | `CursorPos` maths on `CurrentWindow` |
| `0x1401dc100` / `0x1401dc180` | `PushFont` / `PopFont` | font stack at ctx `+0x1bd8` |
| `0x1401d6860` | `Render` | calls `EndFrame` (`0x1401d65a0`) if the frame was not ended |

`ImGuiNextWindowData` lives at ctx `+0x1b30` (Flags), `+0x1b34` PosCond,
`+0x1b38` SizeCond, `+0x1b40` PosVal, `+0x1b48` PosPivot, `+0x1b50` SizeVal,
`+0x1b90` BgAlpha. The end screen writes these directly (inlined
`SetNextWindowPos/Size/BgAlpha`), using flags `1|2|0x40`, the ≥1.78 values.
`IO.DisplaySize` is at ctx `+0x10`.

There are at least two ImGui contexts: the HUD context rendered by
`FUN_1401c78f0`, and an 854×480 context that `FUN_1400a9cd0` swaps in, builds
and renders in one go (thumbnail/clip overlay). Mod UI runs only in the HUD
frame.

**Draw order.** The context's embedded draw list at `+0x1ec8` (used by the
training hints) is the *foreground* list: `ImGui::Render` (`FUN_1401d6860`)
appends it after every window, and draws the software cursor into it. A
fill there would cover the game's buttons. `CreateNewWindow`
(`FUN_1401d7810`) inserts a window with `NoBringToFrontOnFocus` (flag bit
13) at the *front* of `g.Windows` (`ctx+0x1a30`/`+0x1a38`), so it renders
first, behind every other window; `hd_sdk::imgui::Ui::fill_backdrop` draws
into such a window. Other ImGui pieces: `g.CurrentWindow` is `ctx+0x1a88`,
`ImGuiWindow::DrawList` is `window+0x2b8`, `RenderFrame` is
`FUN_1401d3070`, and `ImDrawList::AddRectFilled(dl, &min, &max, col,
rounding, rounding_corners)` is `FUN_1401eec40` (the `rounding_corners`
argument dates this ImGui to before 1.82). `Button` is `FUN_1401f5890`,
which calls `ButtonEx` (`FUN_1401f5490`).

**Clip player UI** (`FUN_140092da0`, the TRY IT / RETRY / MAKE CLIP
screen): while a training clip's demo plays (`Game+0x370 >= 0`,
`Game+0x378 == 0`) it draws the recorded inputs, one key per call to
`FUN_140092980` (eight calls, foreground list, keys from the bindings at
`Game+0x4b8..`). When the mouse has moved recently it calls the scrub bar
`FUN_1400a4e20` (foreground list; dragging it sets the MAKE CLIP range at
`scene+0x324`/`+0x328`) and then the buttons. Both functions have no other
callers; `hd_sdk::training::hide_clip_overlay` hooks them to skip drawing.

**Audio** is OpenAL, imported from `OpenAL32.dll` in the game folder
(`alListenerf`, `alSourcef`, `alSourcePlay`, …). Holding the listener gain
(`AL_GAIN`, `0x100A`) at 0 silences everything, music included
(`hd_sdk::audio::set_muted`). Named sound effects go through
`FUN_1401cb1a0` (about 50 call sites).

## Training (tutorial) scenarios

A training scenario has three parts, and none of them lives in `asset/asset`.

1. **A clip file** (`.hdclip`) holds the recorded starting situation. It uses
   the replay format (below) with the clip header fields set. Clips are
   downloaded on demand from `http://b.hyprd.mn/hd/c/<id>.hdclip`, where `<id>`
   is the clip id in **decimal** (`FUN_1401c5350` is `to_string(u32)`;
   download = `FUN_140083510`, vtable `+0x130`). None are cached on disk.
2. **The list** in the TRAINING menu (`FUN_14009f7e0`, ImGui window
   `"clipstut"`) is clip list A on the scene object (`scene+0x180` vector,
   count `+0x1c8`), filled from the server by `get_clips.php`
   (`FUN_1400837f0`, response parser `FUN_140085ca0`). Entries are 0x50
   bytes:

   | Off | Type | Meaning |
   |---|---|---|
   | 0x00 | u32 | clip id (key for objectives and completion) |
   | 0x04 | u32 | > 0 = valid row; on play copied to `Game+0x38c` and `scene+0x70` |
   | 0x08 | u32 | ? |
   | 0x0c | u32 | duration (shown via `FUN_1401c4d80`) |
   | 0x10 | std::string | text (title) |
   | 0x30 | std::string | text |

   Response layout (all little endian): `u8 ?, u16 ?, u32 → scene+0x1e0`;
   three list headers `{i16 count, u16 offset, u16 key}`; `u16`; then for each
   of the 3 lists `{i32 total, u16 n, n × {u32 id, u32, u32, u32 duration,
   u16 len + bytes, u16 len + bytes}}`.
3. **Objectives and hint text are hardcoded per clip id** in two `switch`
   statements:
   * `FUN_1400dbb90` (World tick, vtable-only): respawns the subject enemy
     whenever none are alive, through enemy-manager virtuals on
     `*(world+0x130)` (`+0x78` dodge enemy, `+0x80`, `+0x88`, `+0x98`,
     `+0xa0`, `+0xc8`, `+0xd0`; spawn helpers `FUN_1400e0b40(world, type,
     &pos)`, `FUN_1400e0ef0(world, 0)`).
   * `FUN_140090150` (scene UI): title plus up to 5 hint lines, each with an
     optional `(progress, target)` pair; progress is an i32 counter in the
     World (`+0x90..=0x124`, stage counter `+0x84`). All of it is now
     transcribed as data in `mods/tutorials/game/trainings/builtin/*.training` (format:
     `docs/TRAINING_FORMAT.md`); details below under *Per-training logic*.

   | Clip id | Decimal | Title |
   |---|---|---|
   | `0xda100fd6` | 3658485718 | DASH+SLIDE |
   | `0x5b48affc` | 1531490300 | DODGE |
   | `0x0c75d7bc` | 209049532 | DAGGER JUMP+STOMP |
   | `0x282b60d5` | 673931477 | BUNNY HOP+FAKE |
   | `0x5fac820e` | 1605141006 | RAPID+SHOT (DAGGERS) |
   | `0x797b7874` | 2038134900 | LASER+RICOCHET |
   | `0x90a40082` | 2426667138 | GRAB+THROW |
   | `0x13bc90f6` | 331124982 | POWERUPS |
   | `0x4789e28c` | 1200218764 | ENEMY 1 |
   | `0x7ec8b39a` | 2127082394 | ENEMY 2 |
   | `0x70892b08` | 1888037640 | ENEMY 3 |
   | `0x56c4ba30` | 1455733296 | ENEMY 4 |
   | `0xeee23fd8` | 4007804888 | (untitled) "Get your score above 25" |

**Hint rendering** (after the switch in `FUN_140090150`): no ImGui window.
Text goes straight into the context's embedded draw list (`GImGui+0x1ec8`)
via `ImFont::RenderText` (`0x1401f2ec0`: `font, draw_list, size, ImVec2 pos
in r9, [stack] col, &clip_rect (draw_list+0x90), begin, end, wrap_width,
cpu_fine_clip`), measured with `ImFont::CalcTextSizeA` (`0x1401f2c80`:
`font, &out, size, max_width, [stack] wrap_width, begin, end, remaining`).
Title: `PushFont(*(Game+0x4a0))`, centred on `Game+0x170 * Game+0x190`,
y = 20, colour `0x55ffffff`. Lines: current font, centred, y = 80 + 32·i,
`0x44ffffff` (`0x66ffffff` for the active step), with an optional `"%i/%i"`
progress counter. Hint 1 (the first line under the title) is at y = 80.

**Fonts** (`FUN_1400a8920`): one typeface added to the atlas at several
sizes, with s = `Game+0x190` (1.5 at 2560 x 1440): `+0x4a0` 36·s (title,
54 px, added first so it is also ImGui's default font), `+0x480` and
`+0x488` 32, `+0x4a8` 16·s, `+0x4b0` 9·s; `+0x490` and `+0x498` alias
`+0x480`. Hint lines are the 32 px font (a built-in line measures 465 HUD
units on screen; the 32 px font gives 485 advance width, 24 px gives 367).
Mods must push the font they want: the default is the 54 px title font.
`SetCurrentFont` (`0x1401dc060`) sets `g.FontBaseSize` (`ctx+0x192c`) =
`io.FontGlobalScale (ctx+0xb0, 1.0) * font.FontSize (+0x14) * font.Scale
(+0x60)` and `g.FontSize` (`ctx+0x1928`) = base x window font scales. The
HUD is `io.DisplaySize` 1279.5 x 720 (`Game+0x170 * Game+0x190`), rendered
x2 at 2560 x 1440. `hud::log_diagnostics()` logs all of this at runtime.

**Auto-completion** (end of the hint loop in `FUN_140090150`, also
`FUN_1400e011a`): the progress object is the World (`*(*(menu+0x18)+0xb8)`,
vtable `0x140369168`). For each of 6 hint lines, if `progress >= target` and
the line is the current step, `World+0x84` (step) increments. When it exceeds
5 and `World+0x88` (completed) and `+0x8c` are 0, it sets `+0x88 = 1` and, if
the player has taken over, `Game+0x2a0 = 0` (run over) and records completion
for the clip id. **Unknown ids have all targets 0, so every step passes in the
first frame after TRY IT and the run ends immediately**; the in-run UI then
clears `Game+0x315` (input) and the cursor is released. Holding `World+0x88`
at 1 prevents this (`hd_sdk::training::suppress_auto_complete`).

`Game+0x315` drives cursor capture: while it is set, `FUN_140072b90` requests
`window+4 = 2` (GLFW_CURSOR_DISABLED) and `FUN_14020f030` applies it.
`Game+0x2a0` = player alive in a run; `Game+0x300` = current game mode
(0 menu, 2 run). Menu scene `*(Game+0x70)`: `+0x20` UI state (2 main, 3 in-run
UI, 10 TRAINING list), `+0x31d` pending takeover, `+0x320` takeover tick.

**TRY IT** (`FUN_140092da0`, shown while `Game+0x370 >= 0`) sets
`Game+0x315=1, +0x139=1, +0x20c=0.2f, +0x13c=2, +0x365=0, +0x378=1`.

**Start flow:** clicking a row (`FUN_14009d870`) calls
`scene->vtbl[0x130](id)` and sets `Game+0x368 = id`. When the download
finishes, the scene tick (`FUN_140077330`, request at `+0x260`) calls the
replay parser. On success it sets `Game+0x139=1`, `+0x13c=2`, `+0x364=0x100`
(`+0x365` = watching), and `Game+0x370 = Game+0x368`. While `Game+0x370 >= 0`,
`Game+0x365 == 0` and `Game+0x378 != 0` (the player has taken control), the
World tick runs that id's objective. Completion is stored per id in
`<save>/training` (`FUN_1400aa9f0` save, `FUN_1400aacf0` load: a map of
`u32 id → i32`).

The in-game clip editor saves clips locally as
`%APPDATA%\HyperDemon\clips\<name>_<id>.hdclip` (`FUN_14010c310`, a replay-class
method). No code path was found that *loads* clips from that folder.

### Hook points used by `mods/tutorials`

| Address | Function | Notes |
|---|---|---|
| `0x14020d4b0` | `HttpRequest::Submit(req)` | puts the request into the worker's slot table (`net+0x80`); the worker (`FUN_14020c670`) builds the curl handle from `req+0x40` (URL) only after this, so it is the last safe moment to change the URL |
| `0x14020d530` | worker: per-request curl setup | options: URL (10002), MIMEPOST, write callback, buffer sizes, FOLLOWLOCATION, MAXREDIRS 10, progress callback. **No protocol restriction**, and `file://` is compiled in |
| `0x14020c670` | worker loop | `status = 1` exactly when `CURLcode == 0`; HTTP status is never checked |
| `0x14020ced0` | `HttpRequest::New(net)` | 0xb8 bytes: `+0x20` status (-1 → resend), `+0x40` URL string, `+0xa0..+0xa8` response bytes |
| `0x140002fd0` | `std::string::assign(s, ptr, len)` | |
| `0x140088990` | `vector<ClipEntry>::resize` | new entries from `FUN_140087770`: zeroed, `valid = -1` ("not loaded") |
| `0x1400dbb90` | `World::Tick(world, dt)` | vtable-only, entry at `0x140369180` |
| `0x1400e0b40` | `World::SpawnNearPlayer(world, kind, float pos[3])` | despite the name, spawns a skull at `pos + (0,2,0)` + a random unit vector (`FUN_1401c8bc0` is a random direction, not the player). Fills a 0xe8-byte boid record (`FUN_1400266a0` init: `[0]` 1/3/5, `[1]` 1/10/100 HP for kind 0/2/3; kind 1 = kind 0 at a random point 10 from (0,15,0)) and calls boid_god (`scene+0x78`, vtable `0x140363990`) `+0x98(mgr, &rec, -1)`; `+0xa8` counts live boids of every type. Boid deaths do not call the wave trigger |
| `0x1400e0ef0` | spawn ENEMY 1 | `mgr(+0x98)->vtbl[0x88](mgr, &out, variant, -1, 0)` |
| `0x140090150` | training panel + steps | called directly (only caller `0x14008eb52` in the menu scene's BuildUI `0x14008ea60`, vtable `0x140368080+0x20`); wrapped with the loader's `around_detour` for `on_training_hud_begin/end` |
| `0x1401cb1a0` | play sound effect | inputs `rdx` name (`std::string&`), `xmm2` float, `r9b` flag, `[rsp+0x28]` float; `rcx` is scratch. Step sound: `("complete", 1.0, 0, 1.0)` |

**Taking over the built-ins**: the 13 ids occur only in `FUN_140090150` and
the World tick. `mods/tutorials` writes `Game+0x370 = 0` around both calls
(`hd_sdk::training::NEUTRAL_CLIP`), which sends them down the unknown-id path,
and restores the real id right after. The one exception is the frame after
the data says the training is done: the panel then runs with the real id so
the game records the completion under it.

Enemy managers in `*(world+0x130)` (count vslot / spawn as used by the
trainings): `+0x78` dodge (`0xa8` / `SpawnNearPlayer(2, {0,0,-35})`), `+0x98`
enemy1 (`0x98` / `0x88`), `+0x80` enemy2 (`0x90` / `0x98(mgr,0,-1,0)`), `+0x88`
enemy3 (`0x88` / `0xa0(mgr,0,-1,0)`), `+0xc8` enemy4 (`0x90` /
`0x80(mgr,0,-1,0)`), `+0xa0` target (`0xd0` / `0x80(mgr,&pos,&vel)`, pos
`{0,10,-20}`), `+0xd0` grab items (`0xc0` / `0x80(mgr,&pos,&vel)`, pos
`{-10,10,-20}`), `+0x48` the POWERUP object (`+0x28` scheduled time, `+0x30`
armed; alive while armed and `Game+0x290` clock < time + 12, re-armed with
time = clock + 3). Enemy2–4 spawns take a **fourth** argument (`r9 = 0`) that
Ghidra's prototype drops; calling without it throws
`std::length_error("vector<T> too long")`.

### Per-training logic

**World rebuilds**: when the demo loops (and at TRY IT / RETRY) the game
rebuilds the World, which zeroes `World+0x84`. The UI pass takes it back to
at least 1 the same frame (the title row always passes), so a 0 seen from
the tick or World tick means a fresh attempt.

**Steps** (`FUN_140090150`): a local array of 6 `(progress, target)` pairs,
row 0 = title (always `(0, 0)`), rows 1..=5 = hint lines. Each frame:
counters of rows `k > stage` are zeroed (so progress only counts once a row
is current), then for each row in order, if it is the current row
(`i == World+0x84`) and `target <= progress` (or, before TRY IT,
`progress > 0`), the stage increments and, for rows with a target, the
`"complete"` sound plays (`FUN_1401cb1a0`). Rows without text or target pass
immediately. `"%i/%i"` (min(progress, target)/target) is drawn left of every
reached row with a target once the player took over (`0xffffffff` on the
current row, else `0x33ffffff`).

**Completion** (stage > 5, `World+0x88 == 0 && +0x8c == 0`): `+0x88 = 1`; if
took over: `Game+0x2a0 = 0` (run ends), menu (`*(Game+0x70)`, vtable
`0x140368080`) completion map `+0x3b0` (`std::map<u32, i32>`) `[id] = 1`,
then `FUN_1400aa9f0(menu)` saves it.

**Spawns** (`FUN_1400dbb90`, only while `Game+0x365 == 0 && +0x378 != 0`):
three global cooldowns (`DAT_1403dd0cc/c8/c4`, decremented by dt every tick
while a clip is active). Pattern per rule: if the count is above 0 the
cooldown is reset to 1.0, else once it reaches 0 the thing is spawned. Rules:
DODGE dodge enemy; RAPID+SHOT target; LASER target + enemy2; GRAB target at
`{10,10,-20}` + item; POWERUPS powerup + enemy3 ("SNAKE", only once counters
`+0xe0` and `+0xe4` are above 0) + target; ENEMY 1–4 their enemy; score
training enemy1 with no cooldown. DASH+SLIDE, DAGGER JUMP and BUNNY HOP spawn
nothing.

**Score training** (`0xeee23fd8`): no title; hint 3 ("Get your score above
25", target 25, progress = (int)score) only once the player took over. The
score is drawn large (title font at 36·`World+0x30` pulse, decimal point
centred, y = 4, red at or below 0, shaking below -30), `World` vslot `0x118`
is called (not reversed), and the World tick drains the score by dt while
`Game+0x2a0` (also during the demo). Score storage: `bits = [Game+0x294] ^
[Game+0x298]`; writes use key' = `([+0x298] + [+0x294]) ^ (u32)&[+0x294]`,
enc = `bits ^ key'`.

**Key names in hints**: `%s` is `DAT_1403dc2b0[Game+bind]`, a `const char*`
table by key code, with binds `+0x4b8` forward, `+0x4bc` back, `+0x4c0`
left, `+0x4c4` right, `+0x4c8` fire, `+0x4d0` laser, `+0x4d8` jump (`+0x4cc`,
`+0x4d4` unknown). The movement order comes from the STOMP hint ("release %s
%s %s %s": `4b8, 4c0, 4bc, 4c4`). The SDK finds the table through the key
display at `0x14009302c`.

The training menu pages 3 rows at a time. `get_clips` decides whether a page
needs fetching from its **last** row's `valid` field, so injected rows start on
a fresh page after the server's total.

## Run spawner (main game)

All static analysis. `Game+0x300` is the mode: 0 menu, 1 lobby (after PLAY,
the dagger waits in the arena), 2 run. `Game+0x365` = playing back a replay or
clip, `Game+0x364` = recording, `Game+0x370 >= 0` = a training is active.

**Dagger pickup** (`FUN_1400081f0`, level scene tick; `scene+0xb8` is the
World): in mode 1, when the player is within 1.1 of `(0, 1.5, 0)`, it plays
`"daggercollect"` and sets `Game+0x139 = 1` (transition pending),
`Game+0x13c = 2` (target mode), `Game+0x378 = 0`. When the transition fade
ends (`Game+0x20c >= 0.2`), the same function calls
`World::SetMode(Game+0x13c)` (World vtable `+0x128`, `FUN_1400e0620`) if the
mode differs, clears `+0x139`, and for mode 2 stops any old recording and
**starts recording** (`Game+0x364 = 1`) unless a replay is playing or a
training is active. Tick 0 of every replay is therefore the dagger pickup.
TRY IT and training starts reuse this transition (`+0x139 = 1, +0x13c = 2`).

**`World::SetMode(world, mode)`** (`FUN_1400e0620`): `FUN_1400e0200` resets
the World (tracks, wave cursors, triggers, counters `+0x84..+0x124`), then
`Game+0x300 = mode`. For mode 2 it decrypts the spawn table, an embedded
0x354-byte blob at `0x1403d5760` XORed with the key tables `0x14035d430` /
`0x14035d4b0`, hashes it into `Game+0x304/+0x308` (anti-tamper), and loads
it. `python tools/re/spawntable.py` decrypts and prints it. There is one table
for every run. No per-arena table was found.

```
i32 version (7)
tiles header (FUN_14016f8e0, "tiles" vslot 0xc0), 15 words:
    i32 10                       tiles version
    f32 48, f32 48               -> Game+0x2ec, Game+0x2f0  arena radius (lobby: 15)
    f32 0.107                    -> Game+0x2e8              (Game+0x2e4 = Game+0x2f0)
    f32 60                       -> Game+0x1b8              (lobby: 6; not identified)
    skipped: 0 x10
i32 n (98)
n x { i32 type, f32 value }   -> track 0 vector world+0x160 (u64: hi = type, lo = value)
                                 (version < 5 adds 3-5 extra words per entry)
```

`type = -1` ends a wave (48 waves). Only `StartWave` and the sequencer read
the vector, and both use only the type, so `value` (60 for the first wave
ends, 1 later) has no effect.

**Arena.** `Game+0x2e4` is the live radius (about 40 functions read it:
player, enemies, renderer `FUN_14015a310` with `+0x2ec`). Each tick the level
scene (`FUN_1400081f0`) moves it toward `Game+0x2f0 - 22·(min(gems,149)/149)^2.8`
at 2·dt per second (4·dt growing, 10·dt and an extra `8·(score-30)` once the
score is at or below -30). `gems` is the size of `World+0x228`, the list of
gem-collect times (World vtable `+0x170`).

**Tracks** (`world+0x140 + 0x48·k`, only k = 0 is loaded). Ints are stored
obfuscated as pairs: value = `a ^ b`.

| Off | Meaning |
|---|---|
| `+0x00` | cursor: furthest entry index started |
| `+0x08` | waves started |
| `+0x10` | alive count (`+1` per spawn, `-1` in `FUN_1400dedf0`) |
| `+0x18` | spawned count |
| `+0x20` | `vector<u64>` entries |
| `+0x38` | u8 flag |
| `+0x40` | f32 timer, set to 60 per wave, `-= dt` |

**`StartWave(world, track, pos, push)`** (`FUN_1400dfa00`) increments
"waves started". It also pushes a 0x5c-byte wave cursor onto `world+0x190`,
with its timer at 0 and its start index at 0 for the first wave, else one past
the next `-1` after the track cursor.

**Per tick** (`World::Tick` `FUN_1400dbb90`, the run branch). This branch runs
only when **`Game+0x300 == 2 && Game+0x2a0` (alive) `&& Game+0x136 &&
Game+0x370 < 0`**, so never during a training, demo or TRY IT. It does the
following:

1. Advance every wave cursor (`FUN_1400de900`). A cursor is dropped after
   25 s. While enemy-1 manager vslot `0xb8` returns false, it walks entries
   from its index up to the next `-1` and spawns each one, but **only if
   `Game+0x365 == 0`**. For replay version < 39 (`Game+0x379`) entries are
   0.5 s apart. In this build (39) the whole wave spawns in one tick.
2. Score drains by dt (encrypted at `Game+0x294/0x298`).
3. If waves started == 1, start another, then if == 0, start one. **A fresh
   run therefore starts wave 1 and wave 2 on its first two ticks**: one
   ENEMY 1, then one ENEMY 2 and one ENEMY 3.
4. Triggers (`world+0x1a8`, 0x28 bytes: `+0x0c` HP, `+0x10`, `+0x14` age,
   `+0x1c` track) age. At 60 s a trigger calls `StartWave(..., push = 0)`,
   which only advances the counters. Shooting a trigger
   (`FUN_1400de3c0`, World vtable `+0x48`, damage from
   `FUN_14001c3e0(hit, version)`) to 0 HP starts the next wave with
   `push = 1` and removes the trigger (`FUN_1400e0fb0`). What creates
   triggers was not found.

**Kills start waves.** `StartWave` returns the wave's group id
`waves_started << 4 | track`, the sequencer passes it to the manager, and
each enemy keeps it (`+0xd4`, `+0x114`, ... per enemy type). When an enemy
enters its death state (`FUN_140142f60`, `FUN_140153c10`, `FUN_1400bbf28`,
`FUN_14017ed32`), it calls **World vtable `+0x88` (`FUN_1400dedf0`)** with
that id and its position. If the id is >= 0, the function plays `"trigger"`,
decrements the track's alive count, and calls `StartWave(id & 0xf, pos)`. So
each wave enemy that dies starts the next wave in the table, near where it
died: `FUN_1400df8c0` (vtable `+0xb0`) adds kill positions to the cursor with
that id, and `FUN_1400df730` (`+0xa0`) returns their average. Enemies
spawned with id -1 (trainings, mods) start nothing. In play, killing the
first ENEMY 1 brings ENEMY 2 and ENEMY 3. Step 3 starts wave 2 right away,
so its cursor is probably held by the enemy-1 manager's `0xb8` gate until
then. That is not verified.

World vtable (`0x140369168`), wave-related slots: `+0x18` Tick, `+0x48`
trigger hit, `+0x80` StartWave, `+0x88` wave enemy died, `+0x90` score/gem
pickup (`FUN_1400def40`), `+0x98` refresh a group id to the latest wave
(`FUN_1400df600`), `+0xa0` cursor position by id, `+0xb0` add kill position,
`+0x120` reset, `+0x128` SetMode, `+0x170` `&World+0x228` (gem times),
`+0x178` `&World+0x210`.

| Table type | Manager (`*(world+0x130)+…`) call | Enemy |
|---|---|---|
| 11 / 13 | `+0x98` "owl_god" vslot `0x88(mgr, &out, variant 0/1, id, 0)` | Owl = ENEMY 1 (spawner); 13 is the game's last wave (42) |
| 12 / 14 | `+0x80` "trilo_god" vslot `0x98(mgr, 0/1, id, 0)` | Trilo = ENEMY 2 (scuttles) |
| 10 / 15 | `+0x88` "snake_system" vslot `0xa0(mgr, 0/1, id, 0)` | Snake = ENEMY 3 |
| 3 / 8 | `+0xc8` "spider_system" vslot `0x80(mgr, 0/1, id, 0)` | Spider = ENEMY 4 |
| 2 / 5 / 9 | `+0xc0` "centipede_system" vslot `0x80(mgr, 0/1/2, id, 0)` | Centipede; 2 is wave 19 |
| 0, 1, 4, 6, 7 | nothing | |

Variant 1 is used only once in the game's table: `13` (owl variant 1), in
wave 42, the last wave with enemies. The level scene also has separate boss
systems (`+0xe0` trilo_boss, `+0xe8` spider_boss, `+0xf0` snake_boss, `+0xf8`
owl_boss, `+0x100` boss_god, `+0x108` boss_weak; the model viewer lists
PedeBOSS, OwlBOSS, SnakeBOSS, SpiderBOSS). Whether variant 1 of each manager
is its boss is a guess, and how those systems spawn their bosses was not
traced.

**Level scene systems** (`*(world+0x130)`, vtable `0x140362bf0`, created by
name in `FUN_1400052e0`): `+0x40` gem_system, `+0x48` retrieval (powerups),
`+0x50` projectiles, `+0x58` collision_system, `+0x60` lighting, `+0x68`
bird_god, `+0x70` tiles (arena), `+0x78` boid_god, `+0x80` trilo_god, `+0x88`
snake_system, `+0x90` unknown, `+0x98` owl_god, `+0xa0` owl_eggs (the
trainings' "gem" target), `+0xa8` particle_system, `+0xb0` viewer, `+0xb8`
progression_system (the World), `+0xc0` centipede_system, `+0xc8`
spider_system, `+0xd0` spider_egg_system (the "bomb"), `+0xd8`
spiderling_system, `+0xe0..+0x108` bosses (above), `+0x110` audio_system,
`+0x118` sky_system.

**Powerups** ("retrieval", vtable `0x14036aaa0`). There are two slots, k = 0
mercury and k = 1 sulfur: `+0x28 + 4k` f32 clock at which it appears (10000 =
none), `+0x30 + k` u8 armed, lifetime `+0xd8` (mercury, 10 s) / `+0xd4`
(sulfur, 5 s), fade `+0xdc` (2 s). Each tick `FUN_14011c490(this, k)` arms a
slot (`time = clock + 3`, armed = 1, and the replay records the time through
replay vtable `+0xb8(replay, k, time)`) when all of these hold:
`gems > max(min(prev,133) · (0.25 | 0.66), 9 | 18)` (prev = size of
`World+0x210`, a copy of the gem list made by World vtable `+0xa8`
`FUN_1400df7f0`; when that runs was not traced), clock > 15 s | 30 s, alive,
no training, and `clock + 3 < time`.

**Spawn placement.** Spawn methods (vtable-only, none called directly):

| Enemy | Method | Placement |
|---|---|---|
| owl | `FUN_1400b3ba0(mgr, u16 out[3], variant, id, rec)` | collision `+0xa0`, then y = `mgr+0xe4`; id `0x10` (first wave) → x 0, z -20; variant 1 → (0, 30, 0) |
| trilo | `FUN_140172e70(mgr, variant, id, rec)` | collision `+0xa0` |
| snake | `FUN_140139650(mgr, variant, id, rec)` | collision `+0xa0` |
| spider | `FUN_14014aac0(mgr, variant, id, rec)` | collision `+0xa8` (`FUN_14004a9e0`: adds a temporary obstacle, then calls `+0xa0` **directly**) |
| centipede | `FUN_140031410` | `FUN_1400317e0`: random angle, ring of radius 30, y 12 |

The collision system (`scene+0x58`, vtable `0x140364338`) `+0xa0` is
`FUN_14004a080(coll, float out[3], key, const float center[3], float
radius) -> out`: a free spot within `radius` (43 for every caller) of
`center` (0,0,0). `rec` is the replay record when spawning during playback
(the position is then read from the replay). Otherwise the method writes the
final position to the replay's spawn stream (`replay+0x200`: u32 size, u32
pos, data at +8, write flag `+0x11`, mode `+0x18`, 2 = read) as its last
write. The owl also stores it in `Game+0x2f4` (id -2 spawns reuse it).

**Owl records** (`*(owl_mgr+0x70)`, 400 bytes each; `out[1]` is the index):
`+0x00` state (5 free slot, 6 boss dormant, 7 boss transition, 0 normal),
`+0x04` previous state, `+0x08` state timer, `+0x0c` u8 phase / timer,
`+0x0d` u8 variant (1 = boss), `+0x14` position, `+0x20` previous position,
boss also `+0x80` and `+0x8c`, `+0x140` group id, `+0x180` sound handle.

**Boss phases.** `StartWave`'s caller on a kill, `FUN_1400dedf0`, first
calls owl manager vtable `+0xf8` (`FUN_1400b36a0`). For each boss it does
the following:
* state 6 (dormant, as spawned, phase byte `0x3c`): set state 7 and
  phase 1.
* state 7, phase 1 to 3: `FUN_1400bc3f0` (re-enters state 7), then phase + 1.
* state 7, phase 4: play sound `0x21` at the boss, set phase `0x3c`,
  `FUN_1400bad90` (state 0), `+0xa0 = 0.3`.

Skulls (boids) are not in the spawn table: the owl and the trainings
spawn them. The sequencer skips table types it does not know (any type but
2, 3, 5, 8-15) and moves on, which the `waves` mod uses to keep skull
entries in the table and spawn them itself.

So every wave enemy that dies advances every boss by one step: dormant →
1 → 2 → 3 → 4 → final.

**During playback** (`Game+0x365`), `World::Tick` instead calls
`FUN_1400de5f0`. It reads this tick's spawn events from the replay object
`Game+0x3a0` (vslot `0x240` count, `0x248` get; 0x14-byte records, byte `+8`
is the kind) and calls the same managers. It also bumps the track counters
from `id & 0xf`. Event kinds: 7 → `+0xc0` v0, 8 → `+0xc8` v0, 16 → `+0x88`
v0, 17 → `+0x98` v0, 19 → `+0x80` v0, 25/30 → `+0x98` v1, 26 → `+0xc8` v1,
31 → `+0x80` v1, 32 → `+0x88` v1. **A clip spawns exactly the events
recorded in its body**, and the sequencer cannot add any during playback or a
training.

**`Game+0x136`** ("run spawning on") is set to 1 in the `Game` constructor
(`FUN_140059630`, a dword write of `0x10000` at `+0x134`) and **never written
again**. Only two places read it: the `World::Tick` run-branch gate above, and
`World` vtable `+0x90` (`FUN_1400def40`, enemy-killed bookkeeping). That
function calls `player->vtbl[0x178](0,0,0)` when every track is empty. Writing
0 there before the dagger pickup gives a normal, recorded mode-2 run with no
waves, no triggers and no score drain. With 0 the run branch's `else` path
only mutes the three music layers (`world+0x358..+0x368`).

### Making an empty clip

* A clip recorded from a normal run cannot be empty. Recording starts at the
  pickup, and waves 1 and 2 spawn on ticks 0-1 of every run, whatever the
  arena.
* The recommended route is to record one with spawning off. Have a mod set
  `*(u8*)(Game+0x136) = 0` while in the lobby (mode 1), collect the dagger,
  stand still about 1 s, end the run, and save the clip with the editor
  (clip start at the beginning). Restore 1 afterwards; nothing else rewrites
  it. The body then has keyframes and input ticks but no opcode-0 spawn
  events.
* Synthesising a body needs the keyframe (`0x13` blob) format decoded
  first. A zero-tick clip hangs playback (division by clip length).
* Not verified at runtime: wave 1/2 timing in real play (the enemy-1
  manager `0xb8` gate may delay it), and whether the run-end and clip-editor
  paths behave with `Game+0x136 = 0`.

## Replay / clip file format (`.hdreplay`, `.hdclip`)

Header reader `FUN_140105cf0`, body in `FUN_1401062f0`. Verified against 100
local replays with `tools/re/hdreplay.py`: all 100 parse, and all 100 bodies
are zlib.

```
char[6]  "hdrpl\0"
u8       version                 (39 in this build; must be > 3)
if version > 7: u32 header_size  (offset of the zlib body)
u8       has_clip
if has_clip:
    i32  clip_start              -> (*(Game+0x70))+0x320, flag +0x31d
    u64  clip_?                  -> (*(Game+0x70))+0x324
u32      length?                 -> replay+0x194   (ticks; scales with run length)
u32      ?  (float bits)         -> Game+0x2e0
u32      ?  (float bits)         -> Game+0x2dc
u8       mode / arena index      -> Game+0x2bc     (0..11)
if version > 7:
    u32  ?  (float bits)         -> Game+0x2d8
    u32  ?                       (26332 in every sample)
    i32  replay id               (matches the file name, e.g. 0x13a5af91)
u32      ?                       -> replay+0x1fc
u32      ?
u32      ?
i16 n;   u32[n]                  -> header vector (+0x40), float bits
i32 n;   u32[n]                  -> replay+0x1c0   (seek table: offsets into body)
...      zlib stream             (miniz inflate) -> per-tick frame data
```

The body is the deterministic input/state stream and has not been fully
decoded. What is known: playback steps it one tick at a time in
`FUN_140104730` (replay vtable `0x14036a190` slot `+0x88`). The tick is a
byte-opcode stream read at `replay+0x200` (size) / `+0x204` (position) /
`+0x208` (data):

| Op | Payload | Meaning |
|---|---|---|
| `0x00` | `u8 kind, u32, u32, u32` + per-kind extra bytes (sizes at `replay+0x1a0`) | spawn event, pushed as a 0x14-byte record to `replay+0x18`; replayed by `FUN_1400de5f0` (see *Run spawner*) |
| `0x01`..`0x0c`, `0x0f` | various | other events, each into its own vector on the replay |
| `0x0d`, `0x0e` | none | `replay+0x180 += 1` |
| `0x11` | 7 × i8, 2 × i16 | per-tick player input (`replay+0x1d8..+0x1f8`) |
| `0x13` | `u32 n` | skip `n` bytes, remember the position at `replay+0x188` (state keyframe; the seek table points at these) |
| `0x14` | none | end of tick |
| `0x16` / `0x17`..`0x19` | none | skip 0x1c / 8 bytes |
| other | | end of replay (`replay+0x19c = 1`) |

Both built-in clips sampled (BUNNY HOP, ENEMY 1; version 28) start with
`0x13` and a 244-byte keyframe, so a body is not just input ticks. A
synthetic clip would need that keyframe format, which is not decoded.

## Assets

`FUN_140212490` opens `asset/asset`. `FUN_140211e70(mgr, type, std::string*
name, flags)` looks the name up in an index at `mgr+0x128` (0x58-byte records)
and loads it through `FUN_140212920`; a miss logs "Can't find asset". This is
the natural hook point for asset overrides; it is not implemented yet.

## Regenerating after a game update

1. `python tools/re/broker.py` (keeps one Ghidra session open, about 3 min to
   import), then query it with `tools/re/q.py` and `tools/re/dec.py`.
2. Re-find each function from the evidence above (strings are the quickest
   anchors: the `add_*.php` URLs, `Debug##Default`, `HyperDemonInstanceLock`).
3. `python tools/re/gensig.py <addr>...` prints the shortest unique wildcarded
   signature for each function; update `crates/hd-loader/src/sigs.rs`.
4. `cargo test -p hd-loader` checks every signature against the installed exe.

`tools/re/xr.py` hard-codes this build's section layout. Update its constants
for a new build.
