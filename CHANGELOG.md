# Changelog

All notable changes to this project are listed here. The protocol version and the package version are separate (see `NIX_TOPS.md` §2). Package versions follow [Semantic Versioning](https://semver.org/).

## 0.2.0 (unreleased)

The 0.1.0 draft made true: the spec is checked by CI, every claim about warnings, the evaluator and the verdicts rests on a run or is marked, and the schemas, profiles and laws exist as files. No tag.

### Added

- Trial evidence, `docs/evidence/2026-11-16-gate3-trial/`: the gate-3 command against eight public flakes pinned by commit, `trial.sh` and `counters.sh`, logs and counter JSON for CppNix 2.35.2 and Determinate Nix 3.22.3, and a README with the outcome-and-cause table and the eval-counter table. Re-run on 2026-11-17: outcomes and counters match 2026-11-16.
- Warning evidence, `docs/evidence/2026-11-17-warning-sources/`: five fixture flakes (`builtins.warn`, a renamed Home Manager option standalone and as a NixOS module, an app without `meta`, an unknown output) evaluated on both implementations under gate 3's options, with logs, exit codes and a results table.
- `[[warnings.allow]]` ledger (`match`, `reason`, `until`): an ack with an expiry; an expired entry fails the gate whether or not its warning still appears.
- `examples/quickstart/`: §4's adopter flake word for word and a `stub-kit/` exporting `lib.mkChecks`, `apps.<system>.nixtops` and `formatter`; `nix flake check --no-build --all-systems` passes on both implementations and the adopter's own check survives the merge.
- The verdict, mode, baseline and ack blocks are marked as family core, with a table mapping each Nix verdict to Rust-TOPS 1.2.0 (`77056c9`), Go-TOPS 0.3.0 (`8d5b416`) and Web-TOPS 0.2.0 (`7767ae0`).
- Law catalog as data: `laws/N-01.toml` to `laws/N-49.toml`, `laws/diagnostics.toml`, `laws/README.md` and `schema/law.schema.json`. N-44 to N-49: no registry-indirect inputs; immutable fetcher refs; disabled checks carry an ack; prebuilt binaries declare provenance; `nixConfig` carries an ack; no two majors of one package in a budgeted closure.
- `.github/workflows/spec.yml`, the `spec` job, and `scripts/spec_check.py` with `links`, `catalog`, `schema`, `ids` and `all`: intra-repo links and heading anchors; the law count and judged count agree across §8.2, its target line, the §19 M3 row, the README and `laws/`; every schema is valid and the §9 example, every profile and the quickstart config validate; no internal decision label appears in the prose. Tests are in `scripts/tests/`.
- `scripts/rules_gen.py` (`render`, `write`, `check`): §3.4 is the one numbered rule list; §13, the template's rules block and the rules block of `AGENTS.md` are generated from it between marker comments, and the `spec` job fails on drift.
- `schema/nix-tops.schema.json` at protocol 0.2.0, covering every §9 key; `schema/gate-report.schema.json`, covering the §6.7 report and every §3.3 verdict; `schema/core/`, the shared definitions a family sync replaces.
- Six facet profiles in `profiles/`: `dev-flake`, `package-set`, `nixos-module` and `flake-lib` at launch; `nixos-host` and `home-config` specified and unproven. Floors only, proposed, with eval cost advisory in 0.x.
- `changelog.d/`: the fragment convention for this release.

### Changed

- Gate 3 (§3.1) names three warning sources instead of one: `builtins.warn` (fatal under `abort-on-warn`), `config.warnings` of every NixOS, Home Manager and nix-darwin configuration (read as data), and `nix` command diagnostics (classified by `laws/diagnostics.toml`). Every warning is listed in one run. N-17 and §16 no longer claim `abort-on-warn` makes every warning fatal.
- The evaluator that judges is the CppNix pinned in the kit's `flake.lock`; the host supplies the daemon and store. Determinate Nix is a named CI leg. Baseline keys are (implementation, version, system, `flake.lock` hash). `doctor` reports the host and gates only on a daemon floor.
- One locked invocation: the adopter re-exports `apps.<system>.nixtops` and runs `nix run .#nixtops -- gate`; no command resolves an unlocked reference. `lib.mkChecks` returns a per-system attrset the adopter merges with its own checks.
- Supported systems are `aarch64-darwin`, `x86_64-linux` and `aarch64-linux`; `x86_64-darwin` is unsupported and is gone from the schema. Lix 2.95.0 has `builtins.warn` and `abort-on-warn` **[verify]** and is unsupported until the M7 matrix.
- CI floor (§11): `--no-accept-flake-config`, no `pull_request_target`, `law-diff` runs the base branch's script, VM tests on x86_64 only, `eval-systems` is gone, `build-matrix` is where `deferred` outputs build, with an aarch64-darwin leg. The kit's binary cache has no owner yet.
- Facets replace the single class (§5): a facet is derived from the outputs a flake exports, `nix-tops.toml` may narrow the set with `facets = [...]` and never add one, and a facet sets floors only. `class` becomes `facet` everywhere, `init` takes `--facet`, and a profile has the shape of the config with `facet` in place of `facets`. `flake-lib` launches. Floors are per facet and proposed.
- Every §8.2 row names the triggering artifact (new "Applies to" column); a law applies when its artifact exists, otherwise it is `skip`. §8.3 metadata renames `classes` to `artifacts`. `vulnix` is at tier 2 everywhere its law applies.
- Verdicts (§3.3): `not-evaluated` on a required law fails under `hard` and `ratchet`; the baseline stores the judged-law set and losing one fails. `deferred` and `vacuous` are defined. `not-evaluated` and the report key `not_evaluated` are one name for one thing.
- Budgets: closure tolerance (proposed 1% or 5 MiB), a lock update reports deltas and re-baselines in the same PR, new store-path names and duplicate packages come before bytes. Eval cost is uncached, keyed, normalized and advisory in every profile for 0.x. The baseline records the tool configuration it was taken under; a narrower one is refused. N-07: IFD is ackable per output, default off.
- Output coverage (§6.1): `tested` names its evidence kind, non-derivation outputs get an `instantiated` level, and the claim that no Nix coverage tool exists is removed. Readers are preferred in the order lock, derivation, options tree, git, diff and `meta`, syntax. Mutation (§6.5) is planned as derivation triage.
- Law catalog (§8.2): 49 laws. N-04, N-15, N-25, N-30 and N-40 are corrected, N-26 uses the exact reader as primary, tier 1 re-fetches changed fixed-output derivations and tier 2 all.
- §9 and §18: the config key `facets`, the `nixpkgs_input` key, a required `[toolchain].systems` equal to the exported systems, `[[warnings.allow]]`; `version` and `flake` are dropped. §14, §17, §19 and §20 are rewritten from the trial and as built; §19 is the M0 to M7 plan; §20 says what the `rust-tops` flake fails and the change it waits on.
- The template has no `git add -A`, marks the rules no check judges as `[review]`, and uses a real rename warning as the N-17 example.
- Pull requests open ready for review, not as drafts (`AGENTS.md`, `README.md`). The credits name the licenses in the tree. The Web-TOPS version cited is 0.2.0.

## 0.1.0 (draft)

### Added

- `NIX_TOPS.md`: the Nix-TOPS protocol, draft 0.1.0. Gates: source, lint, eval, build and test, output coverage, facet layers, laws. 43 laws (`N-01`…`N-43`), 29 targeted for v0.1. Five launch classes: `dev-flake`, `package-set`, `nixos-module`, `nixos-host`, `home-config`.
- Appendix B of `NIX_TOPS.md`: corrections to the *Common Mistakes AI Coding Agents Make in Nix* sketch, checked against nixpkgs 26.11 with Nix 2.35.2.
- `config/AGENTS.template.md`: the `AGENTS.md` section that `nixtops init` will write.
- `AGENTS.md` and `CLAUDE.md`: standing orders for agents working on this repo.
- Dual license, MIT OR Apache-2.0.
