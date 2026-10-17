# REA (Reverse Engineer Anything): agent briefing

REA is a reverse-engineering CLI and MCP server backed by Ghidra. This file explains what it
is, how to set it up for this repo, and how to use it without wasting calls.

## What REA is

REA ([morluto/rea](https://github.com/morluto/rea), npm package `rea-agents`) is a CLI
and MCP server for inspecting shipped software without its source code. It can:

- open native binaries and decompile them to pseudocode
- search strings and procedure names, and list callers, callees and cross-references
- inspect PE resources, .NET assemblies, archives, Electron/JavaScript apps, APKs and firmware
- observe running browser, Electron and process behaviour (runtime tools *execute* targets)

Every result comes back as **Evidence**: an ID plus observations, inferences and known
limitations. Cite those IDs and keep the three categories separate in your conclusions.
Pseudocode is a decompiler's reconstruction, not the original source.

## Setup

Install steps for REA, Java and Ghidra are in [`tools/re/README.md`](tools/re/README.md#setup).
To use REA from Claude Code, register it as an MCP server named `rea` with `GHIDRA_INSTALL_DIR`
set in its environment:

```powershell
claude mcp add rea -e GHIDRA_INSTALL_DIR="<your Ghidra folder>" -- cmd.exe /d /c rea mcp
```

`rea doctor --provider ghidra` should report every Ghidra check as healthy. Hopper and IDA are
optional; without them Ghidra is the only native analysis engine. If the server was registered
by hand instead of with `rea setup`, `rea doctor` reports "config drift" and a missing REA
skill. The server still works, so leave them unless the user asks you to run `rea setup`.

### Limits you will hit on Windows

- **Ghidra support on Windows is experimental and read-only.** It analyses x86/x86-64 PE
  executables and DLLs. Renaming and commenting (`annotate_native_function`,
  `set_address_name`) are not available here.
- **REA runs Ghidra headless and builds its own analysis.** It does not open or update the
  user's interactive Ghidra projects. Renames, types and comments in those projects are
  invisible to REA, and REA's findings are not written back to them. Report findings to the user in text.
- Large binaries can take minutes on first analysis. The startup deadline defaults to 330 s
  and can be raised with `REA_GHIDRA_STARTUP_TIMEOUT_MS`. The JVM heap defaults to 2 GB and
  can be changed with `GHIDRA_HEADLESS_MAXMEM`.

## How to use it

### 1. Choose the first tool from the target type

| Target | First tool |
|---|---|
| Native `.exe` / `.dll` | `open_binary`, then the focused tools below |
| PE resources only (icons, version info, embedded blobs) | `inspect_pe_resources` (no need to open the binary) |
| .NET assembly | `inspect_managed_artifact` |
| ZIP / MSIX / AppX / other archive | `open_binary`, then `inspect_artifact` if the inventory helps |
| Electron ASAR or extracted JS tree | `analyze_javascript_application` (use `"detail": "summary"` for large apps) |
| Android APK | `inspect_android_package` |

If the user says "the game" or "my app" and no path has been established, ask which file
they mean. Do not guess from nearby files.

### 2. Native workflow

1. `open_binary` with the absolute path to the target.
2. `binary_overview` when you need imports, exports, sections or general context.
3. Narrow down with search: string literals and procedure names, then callers, callees and
   xrefs.
4. `analyze_function` or decompile on the specific functions that answer the question.
5. Read parts of an already-retained result with `inspect_analysis_view` and its Evidence ID
   (for example pseudocode or value-flow facets, paged with `offset`/`limit`). Don't rerun
   the analysis.
6. `close_binary` when you are finished.

`binary_session` with `{}` shows the open target, the active provider, and which tools are
available right now, with the reason for any that are not.

### 3. Handling large results

- If a result is truncated, or MCP returns `resource_constraint` with `resource: "transport"`,
  call `inspect_analysis_view` with the returned Evidence ID to get a summary, one section or
  one page. Don't repeat the original call.
- Don't repeat an identical call, or a rejected one, unless an input has changed or the
  blocker has been fixed.
- `export_evidence_bundle` writes a whole session to a file when the full record is needed.

### 4. CLI fallback

The same operations are available from the shell, which helps when MCP tools are not loaded:

```powershell
$env:GHIDRA_INSTALL_DIR = "<your Ghidra folder>"
rea analyze  "C:\path\to\target.exe" --format json             # overview
rea search   "C:\path\to\target.exe" --help                    # see exact args first
rea decompile "C:\path\to\target.exe" FUN_00401000 --format json
rea function  "C:\path\to\target.exe" 0x00401000 --format json # full function dossier
rea xrefs    "C:\path\to\target.exe" --help
```

Use `rea <command> --help` or `--schema` before you call a command you haven't used yet.
`--token-limit <n>` caps the size of the output. `rea --llms` prints a manifest of every
command in a form meant for LLMs.

## Troubleshooting

- **REA tools are missing in this session:** the session probably started before the server
  was registered. Ask the user to restart Claude Code. `claude mcp list` should show
  `rea ... ✓ Connected`.
- **Ghidra is reported as unavailable:** run
  `rea doctor --provider ghidra --json` with `GHIDRA_INSTALL_DIR` set as shown above.
- **Analysis times out:** raise `REA_GHIDRA_STARTUP_TIMEOUT_MS` in the MCP server's env.
- Read the bundled guides only when needed:
  `<npm global root>\rea-agents\skills\reverse-engineer-anything\` (`npm root -g` prints the root)
  (`SKILL.md`, then `references/native-and-artifacts.md` for PE work and
  `references/connection-and-recovery.md` for connection problems).

## Ground rules

- Use REA only for questions about compiled or shipped code. For source code you already
  have, use ordinary file tools.
- Static analysis does not show what happens at runtime. Don't claim it observed execution.
- Runtime and capture tools launch or attach to real processes. Use them only when the user
  asks for runtime evidence, and only on the target they named.
- Don't install or upgrade Node, Java, Ghidra or REA without asking the user first.
