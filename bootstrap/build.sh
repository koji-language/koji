#!/bin/sh
# Builds kojic, the Koji compiler, from the seed: the C that the compiler written in Koji
# (compiler/) generates for itself, in bootstrap/kojic.c.gz. Needs gzip, ar and a C compiler. The
# runtime, the library written in Koji (std/koji/), is in bootstrap/libkoji.c.gz, the C that the
# compiler generates for it (`kojic dump library`), archived as build/libkoji_runtime.a.
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

echo "building the runtime"
gzip -dc bootstrap/libkoji.c.gz > build/libkoji.c
# The tasks of the library switch between stacks (std/koji/task.kj), which the shadow stack of
# x86-64 (CET) forbids: without its mark on the library, no program asks the system for one.
flags=-O2
case "$(uname -m)" in
x86_64 | amd64)
    if "$cc" -fcf-protection=none -E -x c /dev/null > /dev/null 2>&1; then flags="$flags -fcf-protection=none"; fi
    ;;
esac
"$cc" $flags -c -o build/libkoji.o build/libkoji.c
rm -f build/libkoji_runtime.a
ar rcs build/libkoji_runtime.a build/libkoji.o
export KOJI_RUNTIME="$PWD/build/libkoji_runtime.a"

# Stage 1, the seed: built without optimizations, which is faster, since it only builds stage 2.
echo "building stage 1 from the seed"
gzip -dc bootstrap/kojic.c.gz > build/stage1.c
"$cc" -O0 -o build/stage1 build/stage1.c "$KOJI_RUNTIME" -lm

# Stage 2, the compiler of compiler/.
echo "building kojic from compiler/"
build/stage1 build compiler/main.kj -o build/kojic

if [ "${1:-}" = "--seed" ]; then
    # The seed names the files of the compiler as they are written here: run from the root.
    echo "writing the seed"
    build/kojic dump c compiler/main.kj | gzip -n -9 > bootstrap/kojic.c.gz
    build/kojic dump library | gzip -n -9 > bootstrap/libkoji.c.gz
fi

echo "done: build/kojic, with build/libkoji_runtime.a next to it"
