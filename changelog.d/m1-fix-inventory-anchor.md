### Fixed

- The output inventory (`nixtops outputs`, and gate 3's read of the outputs) anchors every attribute path at the flake's outputs (`url#."devShells"`, `url#.`). Unanchored, nix looked under `packages.<host>` first, so a throwing `packages.<host>` failed every top-level listing and a member named like an output (`checks`, `devShells`) was read as that output. Both evaluators (the pinned judge and `--evaluator host`).
- The inventory's `nix` and `nix-eval-jobs` runs turn the evaluation cache off (`eval-cache false`), so a failure is reported with its real error and not as a cached failure.
