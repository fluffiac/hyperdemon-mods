# RE tooling

Helpers used to reverse `hyperdemon.exe` with [REA](https://github.com/morluto/rea)
plus Ghidra. You only need these to research the binary, not to build or play
the mods.

## Setup

You need:

* Python 3.10 or newer
* Node.js 20 or newer, for REA
* A JDK that your Ghidra version supports (Ghidra 12.x needs JDK 21 or newer)
* [Ghidra](https://github.com/NationalSecurityAgency/ghidra/releases), unzipped
  anywhere. These notes were written against 12.1.3.

1. Install REA:

   ```powershell
   npm install -g rea-agents
   ```

2. Point it at Ghidra. Use the folder you unzipped, the one that contains
   `support\analyzeHeadless.bat`:

   ```powershell
   $env:GHIDRA_INSTALL_DIR = "D:\tools\ghidra_12.1.3_PUBLIC"   # your path here
   ```

   To keep it set for new shells, run
   `setx GHIDRA_INSTALL_DIR "D:\tools\ghidra_12.1.3_PUBLIC"`.

3. Check that REA can see Ghidra:

   ```powershell
   rea doctor --provider ghidra
   ```

4. If the game isn't at the default Steam path, set `HD_GAME_EXE` as well:

   ```powershell
   $env:HD_GAME_EXE = "D:\SteamLibrary\steamapps\common\hyperdemon\hyperdemon.exe"
   ```

## Usage

On Windows the `rea` CLI re-imports the binary on every call (about 3
minutes), so `broker.py` keeps one REA MCP session open and serves queries on
`127.0.0.1:47811`. Start it in its own terminal and wait for `READY`:

```powershell
python tools/re/broker.py
```

Then, from another terminal:

```powershell
python tools/re/dec.py 0x140090150        # decompile into tools/re/pc/
python tools/re/q.py read_function_instructions '{"procedure":"0x140090150"}'
python tools/re/q.py binary_session '{}'  # shows which tools are available
```

The offline scripts read the exe directly and don't need the broker.

| Script | Use |
|---|---|
| `broker.py` | Start first (background). Opens the exe once, prints `READY` |
| `q.py <tool> '<json args>'` | Call any REA MCP tool, print `normalized_result` (e.g. `procedure_callers`, `xrefs`, `resolve_containing_procedure`) |
| `dec.py <addr>...` | `batch_decompile` into `pc/<addr>.c` |
| `gensig.py <addr>...` | Shortest unique wildcarded signature (≥ 24 bytes) per function |
| `xr.py "<string>"...` | Offline: functions that reference a string literal (rip-relative scan) |
| `callers.py <addr>...` | Offline: direct `call`/`jmp rel32` sites of a function |
| `hdreplay.py <file>...` | Offline: parse `.hdreplay`/`.hdclip` headers and inflate the body |
| `vt.py <addr> [n]` | Offline: dump `n` vtable slots (target and first bytes) |
| `spawntable.py` | Offline: decrypt and print the run spawn table (waves, enemy types) |
| `mcp.py` | Minimal MCP stdio client used by the above |

Environment variables:

| Variable | Needed by | Default |
|---|---|---|
| `GHIDRA_INSTALL_DIR` | `broker.py` and anything using `mcp.py` | none, required |
| `HD_GAME_EXE` | all scripts | the default Steam install path |
| `REA_GHIDRA_STARTUP_TIMEOUT_MS` | `broker.py` | `1200000` (20 min) |
| `GHIDRA_HEADLESS_MAXMEM` | `broker.py` | `6G` |

`xr.py` hard-codes this build's section layout.
