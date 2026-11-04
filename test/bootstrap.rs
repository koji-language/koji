//! The bootstrap: the compiler written in Koji (`compiler/`) is built from the seed
//! (`bootstrap/kojic.c.gz`, the C it generated for itself), like `bootstrap/build.sh` does.

use std::path::Path;
use std::process::Command;

#[path = "koji/mod.rs"]
mod koji;

fn output(command: &mut Command) -> String {
    let result = command.output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    String::from_utf8(result.stdout).unwrap()
}

/// The compiler written in Koji compiles itself, from the seed: stage 1 is the seed
/// (`bootstrap/kojic.c.gz`), stage 2 the compiler of `compiler/` built by stage 1, and stage 3
/// the same compiler built by stage 2. Stage 3 must generate the same C as stage 2 (the fixed
/// point): the seed written again from stage 2 (`bootstrap/build.sh --seed`) builds the same
/// compiler. Slow (the C compiler builds the compiler once more): run it with
/// `cargo test --test bootstrap -- --ignored`.
#[test]
#[ignore]
fn the_compiler_in_koji_compiles_itself() {
    let stage2 = koji::compiler();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bootstrap-stages");
    std::fs::create_dir_all(&directory).unwrap();
    let c_of = |compiler: &Path| output(Command::new(compiler).args(["dump", "c", "compiler/main.kj"]));
    let stage2_c = c_of(stage2);
    let stage3 = directory.join("stage3");
    let built = Command::new(stage2)
        .args(["build", "compiler/main.kj", "-o"])
        .arg(&stage3)
        .env("KOJI_RUNTIME", koji::RUNTIME)
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    assert!(c_of(&stage3) == stage2_c, "stage 3 generates another C than stage 2");
}
