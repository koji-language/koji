#!/bin/sh
# Packs a release of Koji for every platform, from build/kojic (bootstrap/build.sh): in dist/,
# koji-<version>-<platform>.tar.gz (koji-<version>-windows-x64.zip), each with kojic and the runtime
# library next to it, the README and the license, and SHA256SUMS of them. Run from any platform of
# the seed: Koji builds and links for the others itself.
#
#     bootstrap/package.sh
set -eu

cd "$(dirname "$0")/.."

if [ ! -x build/kojic ]; then
    echo "no build/kojic: run bootstrap/build.sh first" >&2
    exit 1
fi

version=$(build/kojic version | sed 's/^kojic //')
rm -rf "dist"
mkdir -p dist

for platform in linux-x64 linux-arm64 macos-x64 macos-arm64 windows-x64; do
    case "$platform" in
    linux-x64) target=x64 exe= ;;
    linux-arm64) target=arm64 exe= ;;
    macos-x64) target=macos-x64 exe= ;;
    macos-arm64) target=macos-arm64 exe= ;;
    windows-x64) target=windows-x64 exe=.exe ;;
    esac

    name="koji-$version-$platform"
    echo "packing $name"
    mkdir -p "dist/$name"
    KOJI_TARGET=$target build/kojic library -o "dist/$name/libkoji_runtime.a"
    KOJI_TARGET=$target KOJI_LINKER=koji KOJI_RUNTIME="$PWD/dist/$name/libkoji_runtime.a" build/kojic build compiler/main.kj -o "dist/$name/kojic$exe"
    cp README.md LICENSE CHANGELOG.md "dist/$name/"

    if [ "$platform" = windows-x64 ]; then
        (cd dist && zip -qr "$name.zip" "$name")
    else
        (cd dist && tar -czf "$name.tar.gz" "$name")
    fi
done

cd dist

if command -v sha256sum > /dev/null; then
    sha256sum ./*.tar.gz ./*.zip | sed 's# \./# #' > SHA256SUMS
else
    shasum -a 256 ./*.tar.gz ./*.zip | sed 's# \./# #' > SHA256SUMS
fi

echo "done: dist/, version $version"
