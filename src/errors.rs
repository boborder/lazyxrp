use std::env;

use tracing::error;

pub fn init() -> color_eyre::Result<()> {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default()
        .panic_section(format!(
            "This is a bug. Consider reporting it at {}",
            env!("CARGO_PKG_REPOSITORY")
        ))
        .capture_span_trace_by_default(false)
        .display_location_section(false)
        .display_env_section(false)
        .into_hooks();
    eyre_hook.install()?;
    std::panic::set_hook(Box::new(move |panic_info| {
        // Direct crossterm cleanup — never construct Tui inside panic hook
        // (it could itself panic, causing a double-panic and abort).
        // Mirror Tui::exit: leave alt screen, show cursor, then leave raw mode.
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        let _ = crossterm::terminal::disable_raw_mode();

        eprintln!("{}", panic_hook.panic_report(panic_info)); // color-eyre report to stderr
        error!("Error: {panic_info}");

        std::process::exit(1);
    }));
    Ok(())
}
