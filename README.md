# forge-lab

Agent and live-connection tooling for [ixOS](https://github.com/GeneralKnowledge/ixOS), moved out of it on 2026-11-15 so ixOS can stay a framework for building apps. Everything here works on top of ixOS's `forge` command; nothing in ixOS depends on this repository.

It is a side project: it can grow on its own pace without adding to what ixOS builds, tests and documents.

## What's here

| Crate | What | State |
|---|---|---|
| `crates/forge-mcp` | An MCP server (stdio) whose tools run `forge --json` commands: check, build, install, launch, inspect, UI tests and diagnose, plus ixOS's docs and ledger as resources | Builds and tests. `forge_ui_explore` and `forge_ui_focus` wait for the explorer's CLI ([parked/](parked/README.md)) |
| `crates/forge-explore` | The exploratory tester: seeded walks over an app's screens that report crashes, alerts, dead ends and accessibility problems, the focus that aims a walk at a change, and a screen-graph model for walks without a phone | Library builds and tests; its CLI is parked |
| `crates/forge-devtools` | The app end of the live connection: a small server inside a development build that streams the app's log and lifecycle and answers probes and commands | Builds and tests |
| `crates/forge-live` | The computer's end: pairing secrets and connecting through usbmuxd | Builds and tests |
| `crates/forge-lab` | The `forge-lab` binary: `forge-lab mcp`, `forge-lab device pair\|unpair\|status\|snapshot\|cmd\|connect` | Builds and tests |
| `parked/` | The explorer, focus and run-record CLI code from ixOS, not compiled yet | See [parked/README.md](parked/README.md) |

The ixOS crates it uses (`forge-uitest`, `forge-device`, `forge-devproto`, `forge-runtime`) come from the ixOS repository by git; see `Cargo.toml`.

## Use

```bash
cargo install --path crates/forge-lab --locked
```

**MCP server.** Put this in an app's `.mcp.json` (ixOS's `forge init --agent` no longer writes one):

```json
{ "mcpServers": { "forge": { "command": "forge-lab", "args": ["mcp"], "env": { "FORGE_WORKSPACE": "/path/to/ixOS" } } } }
```

`forge` must be on PATH (or pass `--forge <path>`). `--allow install`, `--bundle-id`, `--udid` and `--no-device` work as before ([docs/forge-mcp.md](docs/forge-mcp.md)).

**Live connection.** In the app's `rust/Cargo.toml`:

```toml
[dependencies]
forge-devtools = { git = "https://github.com/GeneralKnowledge/forge-lab", optional = true }

[features]
devtools = ["dep:forge-devtools"]
```

Start it from the app's core when the feature is on (`forge_devtools::DevTools::start(config, runtime.lifecycle().clone())`), then:

```bash
forge run ios --devtools     # ixOS builds the app crate with its devtools feature
forge-lab device pair        # once per app and phone, with the app open
forge-lab device connect     # the app's log and lifecycle, live
```

## Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
FORGE_WORKSPACE=/path/to/ixOS cargo test --workspace   # without it, the tests that read ixOS's docs are skipped
```

## History

The code kept its history: `git log` shows the ixOS commits that built it. The design notes are in `docs/` (`forge-mcp.md`, `forge-explore.md`, `dev-loop.md`, `forge-m6.md`, `m65-run-report.md`) and `scripts/agent-runs/`; their commands still say `forge mcp`, `forge ui explore` and `forge device pair`, which are now `forge-lab mcp`, the parked explorer and `forge-lab device pair`.
