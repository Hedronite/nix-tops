# Changelog

All notable changes to this project are listed here. The protocol version and the package version are separate (see `NIX_TOPS.md` §2). Package versions follow [Semantic Versioning](https://semver.org/).

## 0.2.0 (unreleased)

The 0.1.0 draft made true: the spec is checked by CI, every claim about warnings, the evaluator and the verdicts rests on a run or is marked, and the schemas, profiles and laws exist as files. No tag.

### Added

- `docs/evidence/2026-11-16-gate3-trial/`: the gate-3 trial against eight public flakes pinned by commit (`refs.txt`), `trial.sh` and `counters.sh`, logs and counter JSON for CppNix 2.35.2 and Determinate Nix 3.22.3, and a README with the outcome-and-cause table and the eval-counter table. Re-run on 2026-11-17: outcomes and counters match 2026-11-16. No protocol text changed.
- `.github/workflows/spec.yml`: the `spec` job (checker tests, then `scripts/spec_check.py all`) on pull requests and on pushes to `main`.
- `scripts/spec_check.py` with `links`, `catalog`, `schema` and `all`: intra-repo links and heading anchors; the law count and judged count agree across §8.2, its target line and the §19 M3 row, and with `laws/` when it exists; the §9 example validates against the schema with `check-jsonschema` 0.38.0.
- `schema/nix-tops.schema.json` at protocol 0.1.0, covering every §9 key. `version` and `flake` stay optional there; this release drops both.
- `changelog.d/`: the fragment convention for this release.
- `[[warnings.allow]]` ledger (`match`, `reason`, `until`): an Ack with an expiry; an expired entry fails the gate whether or not its warning still appears. The `nix-tops.toml` key and schema land with the 0.2.0 schema; §9 is unchanged.
- `docs/evidence/2026-11-17-warning-sources/`: five fixture flakes (`builtins.warn`, a renamed Home Manager option standalone and as a NixOS module, an app without `meta`, an unknown output) evaluated on CppNix 2.35.2 and Determinate Nix 3.22.3 under gate 3's options, with logs, exit codes and a results table.
- `examples/quickstart/`: §4's adopter flake word for word and a `stub-kit/` exporting `lib.mkChecks`, `apps.<system>.nixtops` and `formatter`; `nix flake check --no-build --all-systems` passes on both implementations and the adopter's own check survives the merge.
- The verdict, mode, baseline and ack blocks are marked as family core (`<!-- tops-core:begin -->` and `<!-- tops-core:end -->`), with a table mapping each Nix verdict to Rust-TOPS 1.2.0 (`77056c9`), Go-TOPS 0.3.0 (`8d5b416`) and Web-TOPS 0.2.0 (`7767ae0`).
- N-44 to N-49: no registry-indirect inputs; immutable fetcher refs; disabled checks carry an ack; prebuilt binaries declare provenance; `nixConfig` carries an ack; no two majors of one package in a budgeted closure.
- `laws/N-01.toml` to `laws/N-49.toml`, `laws/diagnostics.toml`, `laws/README.md` and `schema/law.schema.json`: the catalog as data, one file per law, and the table of `nix` diagnostics.
- `scripts/rules_gen.py` (`render`, `write`, `check`): §3.4 is the one numbered rule list; §13, the template's rules block and the rules block of `AGENTS.md` are generated from it between marker comments, and the `spec` job fails on drift.
- `scripts/spec_check.py` also counts the README against the catalog, validates every schema, every profile and the quickstart config, and has an `ids` check that fails on an internal decision label in the prose.
- `schema/nix-tops.schema.json` at protocol 0.2.0 covers every §9 key and validates profile files too; `schema/gate-report.schema.json` covers the §6.7 report and every §3.3 verdict; `schema/core/` holds the shared definitions a family sync replaces.
- Six facet profiles in `profiles/`: `dev-flake`, `package-set`, `nixos-module` and `flake-lib` at launch; `nixos-host` and `home-config` specified and unproven. Floors only, proposed, with eval cost advisory in 0.x.
- The `[[warnings.allow]]` key, `facets` and `nixpkgs_input` are in §9 and the schema; the `warnings.allow` schema lands as the earlier ledger entry said it would.

### Changed

- `NIX_TOPS.md`: the Web-TOPS version cited in the header and the §14 table is 0.2.0; the credits name the licenses in the tree.
- `AGENTS.md`, `README.md`: pull requests open ready for review when the slice is complete, not as drafts.
- Gate 3 (§3.1) names three warning sources instead of one: `builtins.warn` (fatal under `abort-on-warn`), `config.warnings` of every NixOS, Home Manager and nix-darwin configuration (read as data), and `nix` command diagnostics (classified by the versioned table `laws/diagnostics.toml`, written with the laws). Every warning is listed in one run.
- N-17 and §16 resolved 5 no longer claim `abort-on-warn` makes every warning fatal. It covers `builtins.warn` only; Home Manager `9d7ba6b` (run) and nix-darwin `ff0e21a` (read in source) print `config.warnings` with `builtins.trace`.
- §18 prose says the same about the `abort_on_warn` constant.
- The evaluator that judges is the CppNix pinned in the kit's `flake.lock` (§2, §8.1, §10); the host supplies the daemon and store. Determinate Nix is a named CI leg with its own settings (`lazy-trees`, parallel evaluation) off. Baseline keys are (implementation, version, system, `flake.lock` hash). `doctor` reports the host and gates only on a daemon floor (§12).
- One locked invocation: the adopter re-exports `apps.<system>.nixtops` and runs `nix run .#nixtops -- gate`; no command resolves an unlocked reference (§1.3, §4, §11). `lib.mkChecks` returns a per-system attrset the adopter merges with its own checks; lint checks exist for one system (§1.4). `formatter` is per system (§4).
- Supported systems are `aarch64-darwin`, `x86_64-linux`, `aarch64-linux`; `x86_64-darwin` is unsupported (§2). The Lix line is corrected: Lix 2.95.0 has `builtins.warn` and `abort-on-warn` **[verify]**, unsupported until the M7 implementation matrix.
- CI floor (§11): `--no-accept-flake-config`, no `pull_request_target`, `law-diff` runs the base branch's script, VM tests on x86_64 only, the kit publishes a binary cache (owner open), `eval-systems` is gone (gate 3 uses `--all-systems`), `build-matrix` is where `deferred` outputs build, with an aarch64-darwin leg.
- §16 open 1 resolved (one Rust binary from M1, gated by Rust-TOPS) and open 4 resolved: `--build-all` is absent on CppNix 2.35.2 and accepted on Determinate Nix 3.22.3.
- Facets replace the single class (§5): a facet is derived from the outputs a flake exports (`devShells`, `packages` or `overlays`, `nixosModules`, `lib`, `nixosConfigurations`, `homeConfigurations`), `nix-tops.toml` may narrow the set and never add one, and a facet sets floors only. `flake-lib` launches; `nixos-host` and `home-config` are specified, unproven. The kit's own facets are stated. Floors are per facet, proposed, and calibrated in M4.
- Every §8.2 row names the artifact that triggers it (new "Applies to" column); a law applies when its artifact exists, otherwise it is `skip`. The four holes in the old tables close: N-25 and N-26 apply wherever an own fixed-output derivation is in an output's closure; N-34 names the same artifacts in §5, §8.2 and §8.3; N-43 applies to every adopter; `vulnix` is at tier 2 everywhere its law applies (`package-set`, `nixos-host`, `home-config`).
- §8.3 law metadata renames `classes` to `artifacts`. The profile shape (§5.3) uses `facet` and has no `[laws]` list; the required-law set is derived from the artifacts present.
- §16 open 6 is resolved (item 13).
- §2: the Determinate binary defaults to `lazy-trees false` and `eval-cores 1`; the installer's managed `/etc/nix/nix.conf` turns both on. The gate passes both options either way.
- Verdicts (§3.3): `not-evaluated` on a required law fails the gate under `hard` and `ratchet` (a law is required when it is judged in §8.2 and its artifact exists); the baseline stores the judged-law set and losing one fails. `deferred` (an output a `build-matrix` leg owns; CI fails while any is unbuilt) and `vacuous` (a diff law with nothing to compare; passes on an empty diff, fails in CI on a missing merge base) are defined. `not-evaluated` and the report key `not_evaluated` are one name for one thing.
- Closure budgets (§6.3): a proposed tolerance of 1% or 5 MiB, a lock update reports deltas and re-baselines in the same PR, new store-path names and duplicate packages are reported before bytes, and own derivations point at `outputChecks` **[verify attribute names]**.
- Eval cost (§6.4): always `--no-eval-cache`, keyed by (implementation, version, system, `flake.lock` hash), normalized against a reference output on the same lock (proposed), and advisory in every profile for 0.x; the §5.2 cell says so. §16 open 2 is resolved (item 14) from the trial counters.
- The baseline (§6.8) records the judged-law set and the tool configuration it was taken under; a narrower configuration is refused. The report (§6.7) counts `not_evaluated`, `deferred` and `vacuous` separately and marks outputs re-evaluated with IFD on.
- N-07: IFD is ackable per output (default off); an acked output is re-evaluated with IFD on and the report says so. §16 resolved 5 points to it.
- Output coverage (§6.1): `tested` names its evidence kind (in-build, dependent check, `passthru.tests` for packages; a non-default option definition for modules; a unit test for `lib`; a VM test for configurations). Overlays, templates, apps, `lib` and module exports get an `instantiated` level. `tested_floor` is set per facet.
- The claim that no Nix coverage tool exists is removed. `nixcov`, nix-unit's `lib.coverage.addCoverage` and `NIX_COUNT_CALLS` are cited for what each measures; option coverage and call coverage are advisory, and the module coverage question (§16, open 3) stays open for M4.
- Reader preference (§8.1): lock, derivation, options tree, then syntax. Every §8.2 row lists its readers in that order; syntax readers stay for N-01 to N-03 and N-10.
- Mutation (§6.5): the overlay-flip sketch is replaced by derivation triage, recorded as the plan for the M6 spike. No mutation layer in 0.x.
- Law catalog (§8.2): 49 laws, N-01 to N-49. N-04 is reworded (an untracked file fails where it is imported; a staged but uncommitted file passes there and fails in CI), N-15 is judged against the declared Nix floor, N-25 accepts `sha512-` and refuses `lib.fakeHash`, N-30 requires `meta.description` and `meta.license` (`meta.platforms` advised), and N-40 refuses `machine.sleep`. The v0.1 target line and the roadmap row count agree with the table.
- N-26 uses the exact reader (fetch inputs compared between the merge base and the head) as primary. Tier 1 re-fetches the changed fixed-output derivations; tier 2 re-fetches all (§3.2, §7).
- Facet layers (§7): the section is renamed from class to facet. The eval-cost budget row says advisory in 0.x, and a closing sentence in §8.2 names artifacts, not classes.
- Reader preference (§8.1): the git, diff and `meta` readers have a place in the order.
- `class` becomes `facet` everywhere: `init` takes `--facet`, config uses `facets = [...]`, and a profile has the shape of the config with `facet` in place of `facets`.
- §9 and §18: `[toolchain].systems` is required and equals the exported systems; `x86_64-darwin` is gone from the schema; `allow_ifd` stays fixed at false and IFD is allowed per output by an N-07 ack; `version` and `flake` are dropped. §18 points to the schema files.
- §3.4 is the one rule list (11 rules, each with a check tag or `[review]`). The template has no `git add -A`, marks the rules no check judges as `[review]`, and uses a real rename warning as the N-17 example.
- §14, §17, §19 and §20 are rewritten from the trial and as built; §19 is the M0 to M7 plan; §20 says what the `rust-tops` flake fails and the change it waits on. Internal decision labels are replaced with words. Version 0.2.0 everywhere. No tag.

## 0.1.0 (draft)

### Added

- `NIX_TOPS.md`: the Nix-TOPS protocol, draft 0.1.0. Gates: source, lint, eval, build and test, output coverage, class layers, laws. 43 laws (`N-01`…`N-43`), 29 targeted for v0.1. Five launch classes: `dev-flake`, `package-set`, `nixos-module`, `nixos-host`, `home-config`.
- Appendix B of `NIX_TOPS.md`: corrections to the *Common Mistakes AI Coding Agents Make in Nix* sketch, checked against nixpkgs 26.11 with Nix 2.35.2.
- `config/AGENTS.template.md`: the `AGENTS.md` section that `nixtops init` will write.
- `AGENTS.md` and `CLAUDE.md`: standing orders for agents working on this repo.
- Dual license, MIT OR Apache-2.0.
