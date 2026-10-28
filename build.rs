use vergen_gix::{Build, Emitter, Gix};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // crates.io / `cargo install` unpacks without `.git`. Always emit fallbacks
    // first so `env!("VERGEN_*")` in `src/cli.rs` still compiles; successful
    // vergen instructions overwrite these.
    println!("cargo:rustc-env=VERGEN_GIT_DESCRIBE=unknown");
    println!("cargo:rustc-env=VERGEN_BUILD_DATE=unknown");

    Emitter::default()
        .add_instructions(&Build::all_build())?
        .add_instructions(&Gix::all_git())?
        .emit()?;
    Ok(())
}
