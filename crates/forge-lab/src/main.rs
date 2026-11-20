//! `forge-lab`: the agent and live-connection tooling that moved out of
//! ixOS's `forge` (2026-11-15). It works on top of `forge`, which must be on
//! PATH (or named with `--forge`): every tool it serves runs a `forge`
//! command, so a capability still lives in the CLI.

mod live;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "forge-lab",
    about = "Agent and live-connection tooling on top of ixOS's forge",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Serve Forge to a coding agent over MCP (stdio): check, build, install,
    /// launch, inspect, test and diagnose tools that run `forge`, plus
    /// Forge's docs as resources (docs/forge-mcp.md). Started by the agent's
    /// MCP client
    Mcp {
        /// Tools only work inside this folder (default: the current directory)
        #[arg(long, value_name = "DIR")]
        root: Option<PathBuf>,
        /// The ixOS checkout whose docs, ledger and templates are served
        /// (default: FORGE_WORKSPACE)
        #[arg(long, value_name = "DIR", env = "FORGE_WORKSPACE")]
        workspace: PathBuf,
        /// The forge binary the tools run
        #[arg(long, value_name = "PATH", default_value = "forge")]
        forge: PathBuf,
        /// Let the agent do something that spends a phone resource: `install`
        /// lets forge_install_device install apps (an app slot and an App ID
        /// on a free team)
        #[arg(long, value_name = "WHAT", value_parser = ["install"])]
        allow: Vec<String>,
        /// Only these bundle ids may be installed (repeatable), e.g.
        /// org.forgemobile.studio, the agent sandbox id
        #[arg(long = "bundle-id", value_name = "ID")]
        bundle_ids: Vec<String>,
        /// The only phone device tools may touch
        #[arg(long)]
        udid: Option<String>,
        /// Leave out every tool that touches a phone (install, launch,
        /// screenshots, logs, UI tests), even if one is plugged in
        #[arg(long, conflicts_with_all = ["allow", "udid"])]
        no_device: bool,
    },
    /// The live connection to a development build (`forge run ios
    /// --devtools`, with the app crate depending on forge-devtools)
    Device {
        #[command(subcommand)]
        cmd: DeviceCommands,
    },
}

#[derive(clap::Args)]
struct LiveArgs {
    /// Which phone, when several are connected
    #[arg(long)]
    udid: Option<String>,
    /// The port the development build listens on
    #[arg(long, default_value_t = forge_devproto::DEFAULT_PORT, hide = true)]
    port: u16,
}

#[derive(Subcommand)]
enum DeviceCommands {
    /// Pair this computer with the development build on screen (once per app
    /// and phone)
    Pair {
        #[command(flatten)]
        live: LiveArgs,
    },
    /// Forget the pairing on the phone and on this computer
    Unpair {
        #[command(flatten)]
        live: LiveArgs,
    },
    /// The paired app's phase, uptime, probes and commands
    Status {
        #[command(flatten)]
        live: LiveArgs,
    },
    /// The values of the state probes the app registered
    Snapshot {
        #[command(flatten)]
        live: LiveArgs,
    },
    /// Run one of the app's own commands; with no name, list them
    Cmd {
        #[command(flatten)]
        live: LiveArgs,
        /// The command's name
        name: Option<String>,
        /// Its arguments
        args: Vec<String>,
    },
    /// The app's own log and lifecycle, live; carries on when the app comes
    /// back from the background. Ctrl-C stops
    Connect {
        #[command(flatten)]
        live: LiveArgs,
        /// Show this many buffered records from before connecting
        #[arg(long, value_name = "N", default_value_t = 50)]
        tail: u32,
        /// Stop after this many records
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Stop when the app goes to the background instead of waiting
        #[arg(long)]
        no_follow: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Mcp {
            root,
            workspace,
            forge,
            allow,
            bundle_ids,
            udid,
            no_device,
        } => serve_mcp(root, workspace, forge, allow, bundle_ids, udid, no_device),
        Commands::Device { cmd } => run_device(cmd),
    };
    if let Err(err) = result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn serve_mcp(
    root: Option<PathBuf>,
    workspace: PathBuf,
    forge: PathBuf,
    allow: Vec<String>,
    bundle_ids: Vec<String>,
    udid: Option<String>,
    no_device: bool,
) -> Result<(), String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root = root.map(|r| cwd.join(r)).unwrap_or(cwd);
    let root = root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", root.display()))?;
    let workspace = workspace.canonicalize().map_err(|e| {
        format!(
            "{} (FORGE_WORKSPACE, the ixOS checkout): {e}",
            workspace.display()
        )
    })?;
    eprintln!(
        "forge-lab mcp: serving {} on stdio with {} (install {})",
        root.display(),
        forge.display(),
        if no_device {
            "refused, no phone tools"
        } else if allow.iter().any(|a| a == "install") {
            "allowed"
        } else {
            "refused"
        }
    );
    forge_mcp::serve_stdio(forge_mcp::Options {
        forge,
        root,
        workspace,
        allow_install: allow.iter().any(|a| a == "install"),
        bundle_ids,
        udid,
        no_device,
    })
}

fn run_device(cmd: DeviceCommands) -> Result<(), String> {
    let backend = forge_device::UsbBackend::new().map_err(|e| e.to_string())?;
    let out = &mut std::io::stdout();
    match cmd {
        DeviceCommands::Pair { live } => with_target(&backend, &live, |t| live::pair(t, out)),
        DeviceCommands::Unpair { live } => with_target(&backend, &live, |t| live::unpair(t, out)),
        DeviceCommands::Status { live } => with_target(&backend, &live, |t| live::status(t, out)),
        DeviceCommands::Snapshot { live } => {
            with_target(&backend, &live, |t| live::snapshot(t, out))
        }
        DeviceCommands::Cmd { live, name, args } => with_target(&backend, &live, |t| {
            live::cmd(t, name.as_deref(), &args, out)
        }),
        DeviceCommands::Connect {
            live,
            tail,
            limit,
            no_follow,
        } => with_target(&backend, &live, |t| {
            live::connect(t, tail, limit, !no_follow, out)
        }),
    }
}

/// Runs a live-connection command against the default pairing store.
fn with_target(
    backend: &dyn forge_device::DeviceBackend,
    args: &LiveArgs,
    run: impl FnOnce(&live::Target) -> Result<(), String>,
) -> Result<(), String> {
    let store = forge_live::PairingStore::default_location().map_err(|e| e.to_string())?;
    run(&live::Target {
        backend,
        udid: args.udid.as_deref(),
        port: args.port,
        store: &store,
    })
}
