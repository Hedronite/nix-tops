### Fixed

- `nixtops gate --system <s>`: a `flake check` error is scoped by the attribute in its `while checking …` trace lines and its `error:` lines only, so a system name quoted in a source excerpt under another system's error no longer refuses the run.
- `nixtops gate --base`: refused with exit 4 and a report ("the diff laws that use --base are not built in M1"), like `--max-tier` other than 1, since no diff law is built.
- `nixtops gate` on a cold Nix store: an output that needs import from derivation is named N-07 when nix says only that a `.drv` path "is not valid"; and a warning that stopped the evaluation is collected (the re-run without `abort-on-warn`, then `config.warnings` read as data) even when nix does not print "abort-on-warn is set", so a renamed NixOS option is attributed to `config-warnings` on a cold store too.
