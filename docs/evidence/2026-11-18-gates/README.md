# `nixtops gate` over the nine gate fixtures

The kit's own command, `nixtops gate`, run over `fixtures/gates/` on three evaluators, with `run.sh <label> <nixtops> [flags]` as the whole procedure. Each fixture is copied into a scratch directory that is its own git repository, so gate 1 sees a clean tracked tree. Eval only: no build, no `--impure`, locks never updated, `--no-accept-flake-config`. The runner is the build of this repository (`nix build .#nixtops`).

| Label | Judge | Daemon host |
|---|---|---|
| `determinate-pinned` | the kit's pinned CppNix 2.35.6 | Determinate Nix 3.22.3, host config as installed |
| `determinate-host` | `--evaluator host`: the host's Determinate Nix 3.22.3 (`lazy-trees false`, `eval-cores 1`) | same host |
| `cppnix-pinned` | the pinned CppNix 2.35.6 | CppNix 2.35.2 |

## Verdicts

`source` and `eval` for each fixture, and the exit code of the real command; the three runs agree on every row, and every row equals the `exit` and `layers` of its `expected.json`. Each fixture's `nix-tops.toml` switches `lint`, `build-test`, `coverage`, `facet-layers` and `laws` off with the reason "not built in M1" (the `[layers]` keys), so those layers are `skip` and a passing fixture exits 0. `tests/gate_fixtures.rs` runs the same fixtures with the real tools on both the pinned judge and `--evaluator host` and compares them with `expected.json`.

| Fixture | `source` | `eval` | What the report says |
|---|---|---|---|
| `clean` | pass | pass | 4 outputs evaluated |
| `nixos-renamed-option` | pass | Refuse | a `config-warnings` entry: the renamed-option notice is in the configuration's `warnings` list, which `abort-on-warn` does not stop (this corrects the plan, which expected `builtins-warn`) |
| `hm-standalone-renamed` | pass | Refuse | a `config-warnings` entry (`flake check` exits 0 on this flake) |
| `nixos-unknown-option` | pass | Refuse | "The option `nonexistent' does not exist" at the configuration's build root |
| `ifd` | pass | Refuse | N-07 |
| `warn-allowed` | pass | pass | a `builtins-warn` entry, verdict `Ack`, with its ledger entry and expiry; `acks.count` 1 |
| `warn-expired` | not-run | not-run | exit 2: the config check refuses an expired entry before any layer; the report is written with every layer `not-run` and the reason |
| `diag-unclassified` | pass | Refuse | `warning: ignoring untrusted flake configuration setting 'extra-substituters'.` is in no table entry: an unclassified diagnostic |
| `omitted-system` | not-run | not-run | exit 2: the config check reads the outputs (`evaluate_with_outputs`) and `aarch64-linux`, declared in `[toolchain].systems`, is not exported; the report is written with every layer `not-run` and the reason |

## The Determinate host's own settings

The Determinate installer's `/etc/nix/nix.conf` sets `eval-cores`, `lazy-trees`, `bash-prompt-prefix` and `upgrade-nix-store-path-url`, which the pinned CppNix does not know; it prints `warning: unknown setting ...` for each. `laws/diagnostics.toml` classifies exactly those four lines (D-06 to D-09, effect `report`), so `determinate-pinned` runs on that host as installed and passes where the flake is clean. A different unknown setting would be an unclassified diagnostic and fail gate 3. `--evaluator host` has no such lines, because the settings are the host's own; it turns the Determinate-only settings off as section 2 says. The dirty-tree warnings of both implementations are D-10 and D-11.

## What the tests add

- `tests/gate_run.rs` (stand-in judge, real `git`): the stop rule and `--keep-going`, N-04 (untracked, staged, ignored, `.direnv`), N-05 (not committed, modified, not current), the clean-tree rule in CI, the ledger, the collect re-run, `config.warnings`, `--only`, `--system`, every layer switched off by the config's `[layers]` (each is `skip` with its own reason, and removing the mapping turns the tests red), a switch on gate 1 or gate 3 (the schema refuses it: exit 2), a reason that is only whitespace (exit 2), the text line that laws are not enforced when the laws layer is off and a host below the floor and a flake with no outputs; every report it produces is validated against `schema/gate-report.schema.json`.
- `tests/gate_fixtures.rs`: the nine fixtures with the real tools (set `NIXTOPS_FX_*`; skipped otherwise), and a dirty git tree: a tracked file edited and not committed is evaluated, an untracked `.nix` file is not read and gate 1 still refuses it.
- Dirty trees: the inventory reads the flake through its own reference (`nix eval <ref>#<path> --apply`), not `builtins.getFlake`, which refuses an unlocked reference. That keeps the way `nix flake check` reads a dirty tree: tracked files only.

## Found on the way

- The unit tests of the nix build now need `git` (`nativeCheckInputs`).
- A host whose `nix` cannot be run stops the run with exit 3 and now says so on stderr.
- The fixtures were never run in place in the repository tree: every run is from a scratch copy that is its own git repository.
