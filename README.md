# Nix-TOPS

**A protocol and drop-in kit for Nix that is pure, correct, dense, and fast.**
Flakes, packages and overlays, NixOS and Home Manager modules and configurations, and the flake inside a Rust, Go or web repo. The gate evaluates every output on every declared system with warnings fatal, then builds, tests and measures what the flake produces.

[Protocol](NIX_TOPS.md) · [Agent harness](AGENTS.md) · [Changelog](CHANGELOG.md)

## Status

Draft. Protocol **0.1.0**. The package is unreleased (0.0.0). There is no code yet.

This repo is at milestone M0: the spec. `nixtops` does not exist yet, and nothing here is ready to copy into CI.

| Milestone | Contents |
|---|---|
| **M0** spec | `NIX_TOPS.md`, schemas, profiles for five launch classes, the AGENTS template |
| M1 | Runner: `init`, `check`, `gate` for source and eval gates, `report.json` |
| M2 | Sandbox checks (`lib.mkChecks`): nixfmt, statix, deadnix, gitleaks; baseline and ratchet |
| M3 | Laws v0.1: 29 of 43 judged; `nixtops-lint`; law fixtures |
| M4 | Output coverage; closure and eval-cost budgets; VM tests |
| M5–M6 | Tier 2 layers, protocol 1.0 |

The full roadmap is in [`NIX_TOPS.md` §19](NIX_TOPS.md#19-roadmap).

## What it checks

| Layer | How |
|---|---|
| Source | Every `.nix` file tracked by git; `flake.lock` committed and current |
| Lint | nixfmt (via `nixfmt-tree`), statix, deadnix, `nixtops-lint`, gitleaks, in the build sandbox |
| Eval | Every output on every declared system, with `abort-on-warn` and import-from-derivation off. NixOS and Home Manager build roots are evaluated, so options that do not exist are refused |
| Build and test | `nix flake check`, every package, dev shell and build root; nix-unit; NixOS VM tests |
| Output coverage | Every exported output: evaluated, built, tested |
| Budgets | Closure bytes per output; eval counters per output |

Three gates differ from the other TOPS protocols, because Nix has no line coverage: coverage becomes output coverage, CRAP becomes closure and eval-cost budgets, and there is no mutation layer in 0.1. See [§6](NIX_TOPS.md#6-scoring-and-reporting).

## What is here

| Path | What it is |
|---|---|
| [`NIX_TOPS.md`](NIX_TOPS.md) | The protocol, draft 0.1.0 |
| [`config/AGENTS.template.md`](config/AGENTS.template.md) | The `AGENTS.md` section that `nixtops init` will write into an adopting repo |
| [`AGENTS.md`](AGENTS.md) | Standing orders for agents working on this repo ([`CLAUDE.md`](CLAUDE.md) imports it) |

## Related

- [Rust-TOPS](https://github.com/Hedronite/rust-tops): the original protocol.
- [Go-TOPS](https://github.com/Hedronite/go-tops): the Go protocol. Nix-TOPS starts from its 0.3.0 fixes.
- [Web-TOPS](https://github.com/Hedronite/web-tops): the web protocol.

A repo can adopt Nix-TOPS alongside its language protocol: the language protocol judges the code, and Nix-TOPS judges the flake (`dev-flake` class).

## Contributing

Read `NIX_TOPS.md` and `AGENTS.md` first. Work on a branch and open a draft PR. A change to laws, schema, profiles, floors, budgets, eval options or normative spec text is a protocol change: it carries the `protocol-change` label, a changelog entry, and a protocol version bump.

## Credits

Built by [Hedronite](https://github.com/Hedronite). Structure adapted from Rust-TOPS, Go-TOPS and Web-TOPS. The laws draw on *Common Mistakes AI Coding Agents Make in Nix* (Hedronite sketch), with the corrections in [Appendix B](NIX_TOPS.md#appendix-b-corrections-to-the-source-sketch).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
