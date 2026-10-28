use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::config;

const LOG_FILE: &str = concat!(env!("CARGO_PKG_NAME"), ".log");

pub fn init(directory: std::path::PathBuf) -> color_eyre::Result<()> {
    std::fs::create_dir_all(&directory)?;
    let log_path = directory.join(LOG_FILE);
    let log_file = std::fs::File::create(&log_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&log_path, std::fs::Permissions::from_mode(0o600));
    }
    let env_filter = EnvFilter::builder().with_default_directive(tracing::Level::INFO.into());
    // `RUST_LOG` wins when set and valid; otherwise `<PROJECT>_LOG_LEVEL` (e.g. `LAZYXRP_LOG_LEVEL`),
    // whose parse errors are returned.
    let log_env = format!("{}_LOG_LEVEL", config::PROJECT_NAME.to_uppercase());
    let env_filter = env_filter
        .try_from_env()
        .or_else(|_| env_filter.with_env_var(&log_env).from_env())?;
    let file_subscriber = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(env_filter);
    tracing_subscriber::registry()
        .with(file_subscriber)
        .try_init()?;
    Ok(())
}
