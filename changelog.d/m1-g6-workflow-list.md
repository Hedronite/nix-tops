### Changed

- §11, §17 and the AGENTS.md state line list the `fixtures` and `trial` workflows. `fixtures` runs the gate over `fixtures/corpus/` and `fixtures/gates/` on every PR, push to `main` and dispatch, on two legs (pinned CppNix, under two daemons); `trial` is eval only: gates 1 and 3 and the outputs inventory over the trial flakes, nightly, on dispatch and on a PR that changes it.
