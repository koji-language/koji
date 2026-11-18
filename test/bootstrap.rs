//! The bootstrap: the compiler written in Koji (`compiler/`) is built from the seed
//! (`bootstrap/seed/kojic-<platform>.gz`, an executable of an earlier compiler), like
//! `bootstrap/build.sh` does.

use std::path::Path;

#[path = "koji/mod.rs"]
mod koji;

/// The compiler written in Koji compiles itself, from the seed: stage 1 is the seed, stage 2 the
/// compiler of `compiler/` built by stage 1, stage 3 the same compiler built by stage 2, and stage
/// 4 by stage 3. Stages 3 and 4 must be the same executable, byte for byte (the fixed point): the
/// seed written again from stage 2 (`bootstrap/build.sh --seed`) is stage 3. Slow: run it with
/// `cargo test --test bootstrap -- --ignored`.
#[test]
#[ignore]
fn the_compiler_in_koji_compiles_itself() {
    let stage2 = koji::compiler();
    let library = koji::library();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bootstrap-stages");
    std::fs::create_dir_all(&directory).unwrap();
    let build = |compiler: &Path, output: &Path| {
        let built = koji::finish(std::process::Command::new(compiler).env("KOJI_RUNTIME", library).args(["build", "compiler/main.kj", "-o"]).arg(output), "");
        assert_eq!(built.code, Some(0), "{}", built.stderr);
    };
    let stage3 = directory.join("stage3");
    let stage4 = directory.join("stage4");
    build(stage2, &stage3);
    build(&stage3, &stage4);
    assert!(std::fs::read(&stage3).unwrap() == std::fs::read(&stage4).unwrap(), "stage 4 is another executable than stage 3");
}
