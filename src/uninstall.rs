//! Remove this binary plus optional `{name}.bak` in the same directory, then resolved config/data dirs.

use std::{
    collections::BTreeSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use crate::config::Config;

fn backup_candidate(binary: &Path) -> Option<PathBuf> {
    let name = binary.file_name()?.to_string_lossy();
    Some(binary.with_file_name(format!("{name}.bak")))
}

/// Paths match `./install.sh --uninstall-help` (effective config/data dirs from `Config`).
pub(crate) fn run_self_uninstall(config: &Config, assume_yes: bool) -> color_eyre::Result<()> {
    let exe = std::env::current_exe()?;
    let backup = backup_candidate(&exe);
    let resolved_cfg = config.resolved_config_dir();
    let resolved_data = config.resolved_data_dir();

    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    dirs.insert(resolved_cfg);
    dirs.insert(resolved_data);

    eprintln!("lazyxrp — self-uninstall will remove:");
    eprintln!("  binary: {}", exe.display());
    if let Some(ref b) = backup {
        eprintln!("  backup: {} (if present)", b.display());
    }
    for d in &dirs {
        eprintln!("  directory: {}", d.display());
    }
    eprintln!();
    eprintln!(
        "`cargo uninstall` metadata is untouched; rerun it yourself if your install layout needs it."
    );

    if !assume_yes {
        eprint!("Type \"yes\" to continue: ");
        io::stderr().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        if line.trim() != "yes" {
            eprintln!("Cancelled.");
            return Ok(());
        }
    }

    let mut errors: Vec<String> = Vec::new();

    for d in dirs {
        if !d.exists() {
            continue;
        }
        if let Err(e) = fs::remove_dir_all(&d) {
            errors.push(format!("remove_dir_all {} — {e}", d.display()));
        }
    }

    if let Some(ref b) = backup
        && b.exists()
        && let Err(e) = fs::remove_file(b)
    {
        errors.push(format!("remove_file {} — {e}", b.display()));
    }

    if exe.exists()
        && let Err(e) = fs::remove_file(&exe)
    {
        errors.push(format!("remove_file {} — {e}", exe.display()));
    }

    if errors.is_empty() {
        eprintln!("lazyxrp: self-uninstall finished.");
        Ok(())
    } else {
        for line in &errors {
            eprintln!("lazyxrp: {line}");
        }
        color_eyre::eyre::bail!("self-uninstall completed with errors");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_candidate_appends_bak_suffix() {
        let p = PathBuf::from("/opt/bin/lazyxrp");
        assert_eq!(
            backup_candidate(&p).unwrap(),
            PathBuf::from("/opt/bin/lazyxrp.bak")
        );
        let p = PathBuf::from(r"C:\bin\lazyxrp.exe");
        assert_eq!(
            backup_candidate(&p).unwrap(),
            PathBuf::from(r"C:\bin\lazyxrp.exe.bak")
        );
    }
}
