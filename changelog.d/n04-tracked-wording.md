### Changed

- N-04 says what gate 1 does: no `.nix` file under the flake root that git does not track, ignored or not; ignored directories such as `.direnv` are skipped. The law text in `NIX_TOPS.md` and `laws/N-04.toml` match, and the fix no longer suggests ignoring the file. The gate is unchanged.
