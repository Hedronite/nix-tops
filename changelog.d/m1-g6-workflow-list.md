### Changed

- §11, §17 and the AGENTS.md state line list the `fixtures` and `trial` workflows. `fixtures` runs the gate over the corpus on every PR, push to `main` and dispatch, on both legs; `trial` runs gate 3 over the trial flakes nightly, on dispatch and on a PR that changes it.
