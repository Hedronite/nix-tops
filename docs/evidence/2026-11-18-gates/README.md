# `nixtops gate` over the nine gate fixtures

The kit's own command, `nixtops gate`, run over `fixtures/gates/` on three evaluators, with `run.sh <label> <nixtops> [flags]` as the whole procedure. Each fixture is copied into a scratch directory that is its own git repository, so gate 1 sees a clean tracked tree. Eval only: no build, no `--impure`, locks never updated, `--no-accept-flake-config`. The runner is the build of this repository (`nix build .#nixtops`).

| Label | Judge | Daemon host |
|---|---|---|
| `determinate-pinned-isolated` | the kit's pinned CppNix 2.35.6 | Determinate Nix 3.22.3; the host's `nix.conf` is set aside (`NIX_CONF_DIR` empty), see below |
| `determinate-pinned-as-is` | the pinned CppNix | Determinate Nix 3.22.3, host config as installed |
| `determinate-host` | `--evaluator host`: the host's Determinate Nix 3.22.3 (`lazy-trees false`, `eval-cores 1`) | same host |
| `cppnix-pinned` | the pinned CppNix 2.35.6 | CppNix 2.35.2 |

## Verdicts

`source` and `eval` for each fixture; the three evaluators that run to a verdict agree on every row. The exit is 1 on every row that is not config-invalid because `lint`, `build-test`, `coverage`, `facet-layers` and `laws` are on and not built in this version, so they are `not-run` and `not-run` fails (the layers-gap decision is open). `expected.json` states the exit with those five switched off; the Rust test `tests/gate_fixtures.rs` runs the gate with them switched off through the test-only seam and compares each fixture with `expected.json` on both the pinned judge and `--evaluator host`.

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
| `omitted-system` | pass | pass | passes today: the systems check of the config step reads the outputs and is not merged; `expected.json` carries the exit 2 it will give and a `pending` note |

## The Determinate host's own settings

The Determinate installer's `/etc/nix/nix.conf` sets `eval-cores` and `lazy-trees`, which the pinned CppNix does not know. It prints `warning: unknown setting ...` on every run, and with the table as it is those two lines are unclassified diagnostics, so every fixture fails gate 3 on that host (`determinate-pinned-as-is`: `eval` is `Refuse` for `clean`, `warn-allowed` and `omitted-system`, which pass in the other labels). That is the protocol as written: the table is `laws/diagnostics.toml` and nothing else, and the entry is open for decision. `determinate-pinned-isolated` runs with the host's config set aside to show what the verdicts are without those lines. `--evaluator host` has no such lines, because the settings are the host's own. Under `--evaluator host` the Determinate-only settings are turned off as section 2 says.

## What the tests add

- `tests/gate_run.rs` (stand-in judge, real `git`): the stop rule and `--keep-going`, N-04 (untracked, staged, ignored, `.direnv`), N-05 (not committed, modified, not current), the clean-tree rule in CI, the ledger, the collect re-run, `config.warnings`, `--only`, `--system`, a layer switched off by the config's `[layers]` (through the seam) and a host below the floor; every report it produces is validated against `schema/gate-report.schema.json`.
- `tests/gate_fixtures.rs`: the nine fixtures with the real tools (set `NIXTOPS_FX_*`; skipped otherwise), and a dirty git tree: a tracked file edited and not committed is evaluated, an untracked `.nix` file is not read and gate 1 still refuses it.
- Dirty trees: the inventory reads the flake through its own reference (`nix eval <ref>#<path> --apply`), not `builtins.getFlake`, which refuses an unlocked reference. That keeps the way `nix flake check` reads a dirty tree: tracked files only.

## Found on the way

- The unit tests of the nix build now need `git` (`nativeCheckInputs`).
- A host whose `nix` cannot be run stops the run with exit 3 and now says so on stderr.
- `nix flake check` on a dirty tree prints `warning: Git tree '...' is dirty` (CppNix) or `... has uncommitted changes` (Determinate); neither is in the table (an open item), so a dirty tree fails gate 3 as the protocol stands.
