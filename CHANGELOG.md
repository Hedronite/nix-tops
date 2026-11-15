# Changelog

All notable changes to this project are listed here. The protocol version and the package version are separate (see `NIX_TOPS.md` §2). Package versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `NIX_TOPS.md`: the Nix-TOPS protocol, draft 0.1.0. Gates: source, lint, eval, build and test, output coverage, class layers, laws. 43 laws (`N-01`…`N-43`), 29 targeted for v0.1. Five launch classes: `dev-flake`, `package-set`, `nixos-module`, `nixos-host`, `home-config`.
- Appendix B of `NIX_TOPS.md`: corrections to the *Common Mistakes AI Coding Agents Make in Nix* sketch, checked against nixpkgs 26.11 with Nix 2.35.2.
- `config/AGENTS.template.md`: the `AGENTS.md` section that `nixtops init` will write.
- `AGENTS.md` and `CLAUDE.md`: standing orders for agents working on this repo.
- Dual license, MIT OR Apache-2.0.
