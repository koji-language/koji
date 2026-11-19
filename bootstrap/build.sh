#!/bin/sh
# Builds kojic, the Koji compiler, from the seed: an executable of the compiler for this platform,
# in bootstrap/seed/kojic-<platform>.gz (linux-x64, linux-arm64, macos-x64, macos-arm64; windows-x64
# for build.cmd). Needs gzip only: the seed builds the runtime library (`kojic library`, the library
# written in Koji, std/koji/) and the compiler of compiler/ with it, machine code and linking
# included, without a C compiler.
#
#     bootstrap/build.sh          builds build/kojic and build/libkoji_runtime.a
#     bootstrap/build.sh --seed   then writes the seeds of every platform it can build for again,
#                                 from the compiler built by itself (the fixed point)
#
# kojic finds the runtime library next to itself (or where KOJI_RUNTIME says). The seeds are
# written again only when the compiler of compiler/ needs a newer one to build (see
# doc/compiler-plan.md, "The seed").
set -eu

cd "$(dirname "$0")/.."

case "$(uname -s)-$(uname -m)" in
Linux-x86_64 | Linux-amd64) platform=linux-x64 ;;
Linux-aarch64 | Linux-arm64) platform=linux-arm64 ;;
Darwin-x86_64) platform=macos-x64 ;;
Darwin-arm64 | Darwin-aarch64) platform=macos-arm64 ;;
*)
    echo "no seed for $(uname -s) on $(uname -m)" >&2
    exit 1
    ;;
esac

mkdir -p build

# Stage 1, the seed, and the runtime library it builds.
echo "unpacking the seed for $platform"
gzip -dc "bootstrap/seed/kojic-$platform.gz" > build/stage1
chmod +x build/stage1
echo "building the runtime with the seed"
build/stage1 library -o build/libkoji_stage1.a

# Stage 2, the compiler of compiler/, and the runtime library built by it, next to it.
echo "building kojic from compiler/"
KOJI_RUNTIME="$PWD/build/libkoji_stage1.a" build/stage1 build compiler/main.kj -o build/kojic
echo "building the runtime with kojic"
build/kojic library -o build/libkoji_runtime.a

if [ "${1:-}" = "--seed" ]; then
    # Stage 3, the compiler built by itself: the seed of each platform, the same from one
    # generation to the next. The seed names the files of the compiler as they are written here:
    # run from the root.
    for target in linux-x64 linux-arm64 macos-x64 macos-arm64 windows-x64; do
        echo "writing the seed for $target"
        case "$target" in
        linux-x64) name=x64 ;;
        linux-arm64) name=arm64 ;;
        macos-x64) name=macos-x64 ;;
        macos-arm64) name=macos-arm64 ;;
        windows-x64) name=windows-x64 ;;
        esac
        KOJI_TARGET=$name build/kojic library -o "build/libkoji-$target.a"
        KOJI_TARGET=$name KOJI_RUNTIME="$PWD/build/libkoji-$target.a" build/kojic build compiler/main.kj -o "build/seed-$target"
        gzip -n -9 < "build/seed-$target" > "bootstrap/seed/kojic-$target.gz"
    done
fi

echo "done: build/kojic, with build/libkoji_runtime.a next to it"
