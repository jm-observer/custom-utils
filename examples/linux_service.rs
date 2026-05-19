//! One unified entry for the whole Linux deploy stack — self-update, rootless
//! user-level systemd install, watchdog, workspace resolution and CLI dispatch.
//!
//! Run it:
//!
//! ```text
//! cargo run --example linux_service --features updater                 # service run mode
//! cargo run --example linux_service --features updater -- --version
//! cargo run --example linux_service --features updater -- --help
//! cargo run --example linux_service --features updater -- install --dry-run
//! cargo run --example linux_service --features updater -- update --force
//! ```
//!
//! Everything lands under the current login user:
//! `~/.local/bin/<bin>`, workspace `~/.config/<app>`,
//! unit `~/.config/systemd/user/<app>.service` — no root, ever.

use custom_utils::updater::{CliAction, DeployCommand, LinuxService};

/// The host CLI owns its top level and *embeds* the library command as a
/// pass-through variant — the library never reads argv or writes stdout.
enum AppCmd {
    Serve,
    Deploy(DeployCommand), // 透传: forwarded as-is to `LinuxService::dispatch`
}

/// Configure the deployment once, then route argv: ask the library whether
/// it's a deploy command; if not, it's the host's own command.
fn boot() -> (LinuxService, AppCmd) {
    let svc = LinuxService::new(
        "alarm-server",            // app: unit name + ~/.config/alarm-server
        "jm-observer",             // GitHub owner
        "alarm",                   // GitHub repo
        env!("CARGO_PKG_VERSION"), // current version
    )
    .description("Alarm Server")
    .extra_bins(["alarm-cli"]) // shipped/installed alongside
    .watchdog_sec(30); // Type=notify + WatchdogSec=30

    let cmd = match svc.parse_deploy() {
        Some(cmd) => AppCmd::Deploy(cmd),
        None => AppCmd::Serve,
    };
    (svc, cmd)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (svc, cmd) = boot();

    match cmd {
        // The library command, forwarded transparently.
        AppCmd::Deploy(cmd) => match svc.dispatch(cmd).await? {
            // Text outcomes: the library did no I/O; the host prints them and
            // can splice in its own usage around `Help`.
            CliAction::DryRun(t) | CliAction::Version(t) | CliAction::Help(t) => {
                println!("{t}");
            }
            // install / update already ran (logged via `log`).
            CliAction::Handled => {}
            // `dispatch` of a deploy command never returns Run.
            CliAction::Run { .. } => unreachable!(),
        },

        // The host's own command: run the service.
        AppCmd::Serve => {
            let workspace = svc.workspace()?; // == args::workspace(&arg, "alarm-server")
            let _wd = svc.spawn_watchdog(); // keep alive for the process lifetime
            println!("running; workspace = {}", workspace.display());
            // (a real service would loop here)
        }
    }

    // Zero-config alternative — equivalent to the whole match above:
    //
    //     match svc.handle_cli().await? {
    //         CliAction::Run { workspace } => { let _wd = svc.spawn_watchdog(); serve(workspace).await? }
    //         CliAction::DryRun(t) | CliAction::Version(t) | CliAction::Help(t) => println!("{t}"),
    //         CliAction::Handled => {}
    //     }

    Ok(())
}
