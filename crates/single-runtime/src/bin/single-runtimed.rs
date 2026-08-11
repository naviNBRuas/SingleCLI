//! The headless SingleCLI runtime daemon. `single-cli` and `single-tui` are
//! both just clients of this over the Unix socket — per spec section 3,
//! the runtime must be headless and the TUI is only one client.

use single_core::SingleDirs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();

    let dirs = SingleDirs::discover()?;
    dirs.ensure_created()?;

    single_runtime::server::serve(&dirs.socket_path()).await
}
