### Fixed

- `nixtops gate --system <s>`: a `flake check` error is scoped by the attribute in its `while checking …` trace lines and its `error:` lines only, so a system name quoted in a source excerpt under another system's error no longer refuses the run.
- `nixtops gate --base`: refused with exit 4 and a report ("the diff laws that use --base are not built in M1"), like `--max-tier` other than 1, since no diff law is built.
