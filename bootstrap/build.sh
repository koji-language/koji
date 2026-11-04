#!/bin/sh
# Builds kojic, the Koji compiler, from the seed: the C that the compiler written in Koji
# (compiler/) generates for itself, in bootstrap/kojic.c.gz. Needs cargo (for the runtime, written
# in Rust), gzip and a C compiler.
#
#     bootstrap/build.sh          builds build/kojic and build/libkoji_runtime.a
#     bootstrap/build.sh --seed   then writes the seed again, from the compiler just built
#
# KOJI_CC chooses the C compiler (default: cc). kojic finds the runtime library next to itself
# (or where KOJI_RUNTIME says).
set -eu

cd "$(dirname "$0")/.."
cc=${KOJI_CC:-cc}
mkdir -p build

case "$(uname -s)" in
    Darwin) libraries="-lSystem -lc -lm" ;;
    *) libraries="-lpthread -ldl -lm -lrt -lutil" ;;
esac

echo "building the runtime"
cargo rustc --quiet --release --lib --crate-type staticlib --manifest-path runtime/Cargo.toml --target-dir build/runtime
cp build/runtime/release/libkoji_runtime.a build/libkoji_runtime.a
export KOJI_RUNTIME="$PWD/build/libkoji_runtime.a"

# Stage 1, the seed: built without optimizations, which is faster, since it only builds stage 2.
echo "building stage 1 from the seed"
gzip -dc bootstrap/kojic.c.gz > build/stage1.c
# shellcheck disable=SC2086
"$cc" -O0 -o build/stage1 build/stage1.c "$KOJI_RUNTIME" $libraries

# Stage 2, the compiler of compiler/.
echo "building kojic from compiler/"
build/stage1 build compiler/main.kj -o build/kojic

if [ "${1:-}" = "--seed" ]; then
    # The seed names the files of the compiler as they are written here: run from the root.
    echo "writing the seed"
    build/kojic dump c compiler/main.kj | gzip -n -9 > bootstrap/kojic.c.gz
fi

echo "done: build/kojic, with build/libkoji_runtime.a next to it"
