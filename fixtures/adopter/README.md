# Adopter fixture for the cold-start measurement

A flake shaped like an adopter's: it has its own nixpkgs input, adds the kit as `nix-tops` and re-exports the runner as `apps.<system>.nixtops` (protocol section 4). It does not call `mkChecks`.

- The kit input is `path:../..`, the tree this directory sits in. `flake.lock` records that relative path and the two nixpkgs revisions, so a run measures the checked-out head and never resolves an unlocked reference. Every command here passes `--no-update-lock-file`.
- The two nixpkgs revisions differ on purpose: an adopter has their own pin, and the kit's tools stay on the kit's pin.
- Nix does not notice when the kit's `flake.lock` moves: this lock keeps the kit's old nixpkgs and still evaluates. `measure.py` compares the two locks and fails. Run `nix flake lock` here and commit it.
- `measure.py <outdir>` times `nix run .#nixtops -- version` on a store that has neither the runner nor a Rust toolchain, and lists which store paths the run downloaded and which it built. `.github/workflows/cold-start.yml` runs it three times on each runner image.
