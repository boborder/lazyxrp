use anyhow::Result;
use vergen_gix::{BuildBuilder, Emitter, GixBuilder};

fn main() -> Result<()> {
    // crates.io / `cargo install` unpacks without `.git`. Always emit fallbacks
    // first so `env!("VERGEN_*")` in `src/cli.rs` still compiles; successful
    // vergen instructions overwrite these.
    println!("cargo:rustc-env=VERGEN_GIT_DESCRIBE=unknown");
    println!("cargo:rustc-env=VERGEN_BUILD_DATE=unknown");

    let mut emitter = Emitter::default();
    if let Ok(build) = BuildBuilder::all_build() {
        emitter.add_instructions(&build)?;
    }
    if let Ok(gix) = GixBuilder::all_git() {
        emitter.add_instructions(&gix)?;
    }
    emitter.emit()?;
    Ok(())
}
