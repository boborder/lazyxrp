//! Remove this binary plus optional `{name}.bak` in the same directory, then resolved config/data dirs.

use std::{
    collections::BTreeSet,
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use crate::config::Config;

fn backup_candidate(binary: &Path) -> Option<PathBuf> {
    let name = binary.file_name()?.to_string_lossy();
    Some(binary.with_file_name(format!("{name}.bak")))
}

/// Security: only delete config/data dirs that look like lazyxrp project dirs.
fn is_safe_uninstall_dir(path: &Path) -> bool {
    let path = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    };
    if path.as_os_str().is_empty() {
        return false;
    }
    // Never wipe filesystem root.
    if path.parent().is_none() {
        return false;
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && path == home
    {
        return false;
    }
    // Require final component to contain "lazyxrp" (default XDG / ProjectDirs layout).
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().contains("lazyxrp"))
}

fn sibling_rp_alias_safe(binary: &Path) -> Option<PathBuf> {
    let alias = binary.with_file_name("rp");
    let meta = fs::symlink_metadata(&alias).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    let target = fs::read_link(&alias).ok()?;
    let target_name = target.file_name()?.to_string_lossy();
    let bin_name = binary.file_name()?.to_string_lossy();
    if target_name == bin_name || target_name == "lazyxrp" || target_name == "lazyxrp.exe" {
        Some(alias)
    } else {
        None
    }
}

/// Paths match `./install.sh --uninstall-help` (effective config/data dirs from `Config`).
pub(crate) fn perform_self_uninstall(config: &Config, assume_yes: bool) -> color_eyre::Result<()> {
    let exe = std::env::current_exe()?;
    perform_self_uninstall_binary(config, assume_yes, &exe, &mut io::stdin().lock())
}

/// Same as [`perform_self_uninstall`] but targets an explicit binary path (tests use a temp dummy).
fn perform_self_uninstall_binary(
    config: &Config,
    assume_yes: bool,
    exe: &Path,
    input: &mut impl BufRead,
) -> color_eyre::Result<()> {
    let backup = backup_candidate(exe);
    let rp_alias = sibling_rp_alias_safe(exe);
    let resolved_cfg = config.resolved_config_dir();
    let resolved_data = config.resolved_data_dir();

    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for candidate in [resolved_cfg, resolved_data] {
        if is_safe_uninstall_dir(&candidate) {
            dirs.insert(candidate);
        } else {
            eprintln!(
                "lazyxrp: refusing to delete unsafe path '{}' (must end with a lazyxrp* directory)",
                candidate.display()
            );
        }
    }

    eprintln!("lazyxrp — self-uninstall will remove:");
    eprintln!("  binary: {}", exe.display());
    if let Some(ref b) = backup {
        eprintln!("  backup: {} (if present)", b.display());
    }
    if let Some(ref a) = rp_alias {
        eprintln!("  alias:  {} (if present)", a.display());
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
        input.read_line(&mut line)?;
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

    if let Some(ref a) = rp_alias
        && a.exists()
        && let Err(e) = fs::remove_file(a)
    {
        errors.push(format!("remove_file {} — {e}", a.display()));
    }

    if exe.exists()
        && let Err(e) = fs::remove_file(exe)
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
    use crate::test_support::{TempRootGuard, TestEnvGuard, env_lock};

    /// `Config::new()` also reads `XRPL_*`/XDG/HOME and strips secrets from the process env;
    /// pin them so a developer shell cannot change the result.
    fn isolated_config_env() -> TestEnvGuard {
        let guard = TestEnvGuard::new(&[
            "LAZYXRP_CONFIG",
            "LAZYXRP_DATA",
            "XRPL_RPC_SERVER",
            "XRPL_NETWORK",
            "XRPL_SEED",
            "XRPL_MNEMONIC",
            "XDG_CONFIG_HOME",
        ]);
        for k in [
            "XRPL_RPC_SERVER",
            "XRPL_NETWORK",
            "XRPL_SEED",
            "XRPL_MNEMONIC",
            "XDG_CONFIG_HOME",
        ] {
            guard.remove(k);
        }
        guard
    }

    #[test]
    fn backup_candidate_appends_bak_suffix() {
        let p = PathBuf::from("/opt/bin/lazyxrp");
        assert_eq!(
            backup_candidate(&p).unwrap(),
            PathBuf::from("/opt/bin/lazyxrp.bak")
        );
    }

    /// TC-150: the HOME guard refuses the home directory itself (even when its
    /// basename contains "lazyxrp"), while a lazyxrp* child of HOME and the
    /// unrelated-basename rejection still behave.
    #[test]
    fn is_safe_uninstall_dir_home_guard_branches() {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&["HOME"]);
        let root = std::env::temp_dir().join(format!("lazyxrp-tc150-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _root_guard = TempRootGuard::new(root.clone());
        let home = root.join("lazyxrp-home");
        std::fs::create_dir_all(&home).expect("create fake home");
        _test_env.set(
            "HOME",
            std::fs::canonicalize(&home).unwrap().to_str().unwrap(),
        );

        // HOME itself: basename contains "lazyxrp", yet the HOME guard must veto.
        assert!(!is_safe_uninstall_dir(&home));
        // A lazyxrp* child of HOME is a legitimate project dir.
        let child = home.join("lazyxrp");
        std::fs::create_dir_all(&child).expect("create child");
        assert!(is_safe_uninstall_dir(&child));
        // An unrelated directory (no lazyxrp in its basename) stays off-limits.
        let other = root.join("unrelated-stuff");
        std::fs::create_dir_all(&other).expect("create unrelated dir");
        assert!(!is_safe_uninstall_dir(&other));

        // Basename contract; `/tmp` and `/` lack a lazyxrp basename (`/` also has no parent).
        assert!(is_safe_uninstall_dir(Path::new("/tmp/lazyxrp")));
        assert!(is_safe_uninstall_dir(Path::new(
            "/tmp/com.kdheepak.lazyxrp"
        )));
        assert!(!is_safe_uninstall_dir(Path::new("/tmp")));
        assert!(!is_safe_uninstall_dir(Path::new("/")));
    }

    /// TC-150: `perform_self_uninstall(assume_yes)` deletes only the safe,
    /// lazyxrp-named config/data dirs plus the binary backup — integration over
    /// the guard + deletion path with a redirected LAZYXRP_CONFIG/LAZYXRP_DATA.
    #[test]
    fn perform_self_uninstall_removes_safe_dirs_and_backup() -> color_eyre::Result<()> {
        let _env_lock = env_lock();
        let _test_env = isolated_config_env();
        let root = std::env::temp_dir().join(format!("lazyxrp-tc150b-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _root_guard = TempRootGuard::new(root.clone());
        let config_dir = root.join("lazyxrp-config");
        let data_dir = root.join("lazyxrp-data");
        std::fs::create_dir_all(config_dir.join("nested"))?;
        std::fs::write(config_dir.join("nested/marker.txt"), b"x")?;
        std::fs::create_dir_all(&data_dir)?;
        _test_env.set("LAZYXRP_CONFIG", config_dir.to_string_lossy().as_ref());
        _test_env.set("LAZYXRP_DATA", data_dir.to_string_lossy().as_ref());

        // Dummy binary under temp root — never touch the running test executable.
        let dummy_exe = root.join("lazyxrp");
        std::fs::write(&dummy_exe, b"binary")?;
        let backup = backup_candidate(&dummy_exe).expect("backup path");
        std::fs::write(&backup, b"stale")?;

        perform_self_uninstall_binary(
            &crate::config::Config::new()?,
            true,
            &dummy_exe,
            &mut io::empty(),
        )?;

        assert!(!config_dir.exists(), "safe config dir must be deleted");
        assert!(!data_dir.exists(), "safe data dir must be deleted");
        assert!(!backup.exists(), "stale .bak must be deleted");
        assert!(!dummy_exe.exists(), "dummy binary must be deleted");
        Ok(())
    }

    /// TC-150: a `lazyxrp`-named symlink is judged by its canonical target, so one pointing at
    /// HOME or at an unrelated directory is refused (and `remove_dir_all` never reaches it).
    #[cfg(unix)]
    #[test]
    fn is_safe_uninstall_dir_refuses_symlink_to_home_or_unrelated() {
        let _env_lock = env_lock();
        let _test_env = TestEnvGuard::new(&["HOME"]);
        let root = std::env::temp_dir().join(format!("lazyxrp-tc150s-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _root_guard = TempRootGuard::new(root.clone());
        let home = root.join("fake-home");
        let other = root.join("precious");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(&other).expect("other");
        _test_env.set(
            "HOME",
            std::fs::canonicalize(&home).unwrap().to_str().unwrap(),
        );

        let to_home = root.join("lazyxrp");
        std::os::unix::fs::symlink(&home, &to_home).expect("symlink");
        assert!(!is_safe_uninstall_dir(&to_home), "symlink to HOME");
        let to_other = root.join("lazyxrp-link");
        std::os::unix::fs::symlink(&other, &to_other).expect("symlink");
        assert!(
            !is_safe_uninstall_dir(&to_other),
            "symlink to unrelated dir"
        );
    }

    /// TC-150: anything but `yes` at the prompt cancels without deleting anything.
    #[test]
    fn perform_self_uninstall_declined_deletes_nothing() -> color_eyre::Result<()> {
        let _env_lock = env_lock();
        let _test_env = isolated_config_env();
        let root = std::env::temp_dir().join(format!("lazyxrp-tc150c-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _root_guard = TempRootGuard::new(root.clone());
        let config_dir = root.join("lazyxrp-config");
        let data_dir = root.join("lazyxrp-data");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::create_dir_all(&data_dir)?;
        _test_env.set("LAZYXRP_CONFIG", config_dir.to_string_lossy().as_ref());
        _test_env.set("LAZYXRP_DATA", data_dir.to_string_lossy().as_ref());
        let exe = root.join("lazyxrp");
        std::fs::write(&exe, b"binary")?;
        let backup = backup_candidate(&exe).expect("backup path");
        std::fs::write(&backup, b"stale")?;
        let config = crate::config::Config::new()?;

        for answer in ["no\n", "y\n", "\n", ""] {
            perform_self_uninstall_binary(&config, false, &exe, &mut answer.as_bytes())?;
            assert!(config_dir.exists() && data_dir.exists(), "{answer:?}");
            assert!(exe.exists() && backup.exists(), "{answer:?}");
        }

        perform_self_uninstall_binary(&config, false, &exe, &mut "yes\n".as_bytes())?;
        assert!(!config_dir.exists() && !data_dir.exists() && !exe.exists() && !backup.exists());
        Ok(())
    }
}
