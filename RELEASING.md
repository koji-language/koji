# Releasing Koji

1.  The version is `version` in `compiler/main.kj` (`kojic version` prints it): set it, and write
    its section in `CHANGELOG.md` (`## <version>`, whose text becomes the notes of the release).
2.  Check that everything passes: `sh bootstrap/build.sh`, then `build/kojic test test/*.kj`, and
    the runs of CI on the other platforms (Linux on arm64, macOS, Windows).
3.  `bootstrap/package.sh` makes the packages in `dist/` locally, to try them before (CI makes them
    too, on each push, as the artifact `koji-packages`).
4.  Push the tag `v<version>` (`git tag v0.1.0 && git push origin v0.1.0`): the workflow
    `.github/workflows/release.yml` builds, tests and packs Koji again, and makes a draft release
    with the packages, `SHA256SUMS` and the notes. Read it, then publish it on GitHub.
