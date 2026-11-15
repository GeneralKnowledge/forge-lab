# Parked: CLI glue not yet wired into `forge-lab`

These files are the `forge` command-line parts of the explorer, its focus and
run records, exactly as they were in ixOS's `crates/forge-cli` when they moved
out (2026-11-15). They are kept for reference and are not compiled: they call
into `forge-cli` internals (`report`, `runner`, `evidence`, `config`, the UI
session) that are not a library.

| File | Was | Library it drives |
|---|---|---|
| `forge-cli/ui_explore.rs` | `forge ui explore` (options, config, focus, walks, the any-app walk) | `forge-explore` (`explore`, `focus`, `model`) |
| `forge-cli/evidence_explore.rs` | the explore evidence bundle | |
| `forge-cli/focus.rs` | `forge ui focus` (which controls a change touches) | `forge-explore::focus` |
| `forge-cli/run_record.rs` | `forge verify --run` (the run record and `report.md`) | reads ixOS's evidence bundles, `.forge/history.jsonl` and the MCP journal |

To bring one back: add a `forge-lab` subcommand that calls `forge --json` for
what it needs from ixOS (the UI session, the app's forge.toml, the evidence
bundle format in ixOS `docs/evidence.md`) and the library here for the rest,
then move the file out of `parked/`. Until then the MCP tools
`forge_ui_explore` and `forge_ui_focus` call commands `forge` no longer has,
and fail with an unknown-command error.
