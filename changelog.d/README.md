# Changelog fragments

Each protocol-change PR in a multi-PR release adds one fragment here instead of editing `CHANGELOG.md`, so two open PRs never write the same file.

- **Name:** `<release-step>.md`, for example `m0-g1.md`. One fragment per PR.
- **Content:** the same shape as a `CHANGELOG.md` entry: one or more `### Added`, `### Changed`, `### Fixed` or `### Removed` headings, each with bullets. State what changed in the protocol or the repo, not how.
- **Folding:** the release PR copies every fragment into one section of `CHANGELOG.md` and deletes the fragments. After that, only this README is left in the directory.
- **Fragments are not checked** by `scripts/spec_check.py`; a missing fragment is a review finding.
