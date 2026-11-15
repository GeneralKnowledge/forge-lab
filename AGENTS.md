# Instructions for coding agents

forge-lab is a side project of ixOS: agent tooling (the MCP server, the explorer, run records) and the live connection to development builds. It depends on ixOS by git; ixOS must never depend on it.

- A capability lives in ixOS's `forge` CLI first. Tools here run `forge --json` commands (ixOS `docs/json-output.md`) instead of reimplementing builds, installs or device access.
- Don't move code back into ixOS. If something here needs a change in ixOS, make it there in its own PR, then bump the git dependency in `Cargo.toml`.
- Check with `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `FORGE_WORKSPACE=<ixOS checkout> cargo test --workspace`.
- Safety rules are unchanged: installing needs the person's `--allow install`, and only a person records an acceptance.
