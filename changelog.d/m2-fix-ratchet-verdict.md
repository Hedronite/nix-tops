### Fixed

- `lib.mkChecks`: the `nixtops-<tool>` verdict checks fail on findings only under `enforce = "hard"`. Under `ratchet` and `advisory` they succeed, and gate 2 judges the findings. Before, they failed whatever the mode, so a flake that exported `mkChecks` and had a finding its baseline held failed `nix flake check`, gate 4 and `nixtops baseline`. `NIX_TOPS.md` section 1.4 says what the verdict does in each mode.
