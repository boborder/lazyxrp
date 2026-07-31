//! `rp` short-command binary (cargo install / release packages).
//! Always runs lookup mode — does not depend on argv0 or install.sh symlinks.
#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    lazyxrp::run_rp().await
}
