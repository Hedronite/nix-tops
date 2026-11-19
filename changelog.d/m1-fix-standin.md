### Fixed

- The runner starts its tools (the host's `nix`, the pinned judge and `nix-eval-jobs`) through one helper that tries again, a few times and briefly, when the program is "text file busy" (`ETXTBSY`, Linux): a program written a moment ago can still be open for writing in a child another thread forked. This was a flaky failure of the stand-in tools in the test suite.
- An empty `nixosModules` (or any empty output set) does not derive its facet: §5.1 says the flake exports it.
- `.cargo/mutants.toml` sets a minimum test timeout, so a loaded host does not report a mutant as a timeout.
