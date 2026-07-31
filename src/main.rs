//! `lazyxrp` binary entry.
#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    lazyxrp::run().await
}
