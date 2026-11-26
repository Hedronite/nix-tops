# Gate 4 (`build-test`) local Darwin and CI Linux evidence

Measured against commit `0a5642beddaf2663a0bfdeecaa903420c52cd054` on branch `m2/g1-gate-4` (PR #36, all 15 CI checks passing).

## Local `aarch64-darwin` (`sandbox = false`)

Files under `local-darwin/` were captured on `aarch64-darwin` running `Determinate Nix 3.22.3 (reports Nix 2.35.2)`, `daemon 2.35.2`, `sandbox: false`, and pinned judge `CppNix 2.35.6` (see `local-darwin/doctor.txt` and `local-darwin/version.txt`). Each fixture ran `./result/bin/nixtops gate --json` with `--layers source,eval,build-test` and `enforce = "hard"`.

| Fixture / mode | Exit | `passed` | `sandbox` | `eval` | `build-test` | Key detail | Artifact |
|---|---|---|---|---|---|---|---|
| `build-linux-only` (local) | `0` | `true` | `"false"` | `pass` | `deferred` | `1 output(s) deferred to other systems: packages.x86_64-linux.default` (`deferred: true`) | [`build-linux-only.local.json`](local-darwin/build-linux-only.local.json) |
| `build-linux-only` (`CI=true`, `hard`) | `1` | `false` | `"false"` | `pass` | `Refuse` | `deferred output(s) in CI under hard: packages.x86_64-linux.default` | [`build-linux-only.ci-hard.json`](local-darwin/build-linux-only.ci-hard.json) |
| `build-failing-check` (local) | `1` | `false` | `"false"` | `pass` | `Refuse` | `flake check build failed` on `checks.aarch64-darwin.fail`; non-host outputs marked `deferred: true` | [`build-failing-check.local.json`](local-darwin/build-failing-check.local.json) |
| `build-failing-package` (local) | `1` | `false` | `"false"` | `pass` | `Refuse` | `build failed` on `packages.aarch64-darwin.default`; non-host outputs marked `deferred: true` | [`build-failing-package.local.json`](local-darwin/build-failing-package.local.json) |
| `build-ifd` (local) | `1` | `false` | — | `Refuse` | `not-run` | `N-07: import from derivation is off`; stops `build-test` without `--keep-going` | [`build-ifd.local.json`](local-darwin/build-ifd.local.json) |

Local `aarch64-darwin` writer-gate runs at `0a5642beddaf2663a0bfdeecaa903420c52cd054`:
- `cargo-tops gate` (`cargo-tops 0.4.0`, `cargo-crap 0.9.0`): `388 tests run: 388 passed (6 slow), 0 skipped`; coverage and crap thresholds `ok`; `108 mutants tested in 6m: 90 caught, 18 unviable` (`0` missed); `ok: 8 command(s) passed`.
- Real-Nix suites: `build_fixtures` (`4 passed`), `gate_fixtures` (`8 passed`), `lint_fixtures` (`5 passed`), including `g1c_system_scoping_closes_q4_and_q9_on_pinned_and_host_evaluators` on both pinned `CppNix 2.35.6` and host `Determinate Nix 3.22.3`.
- Full fixture harness (`scripts/fixtures.sh raw` and `scripts/fixtures.sh gate`): all corpus and gate fixtures passed (`gates/build-failing-check exit 1 ok`, `gates/build-failing-package exit 1 ok`, `gates/build-ifd exit 1 ok`, `gates/build-linux-only exit 0 ok`).
- Self-gate `./result/bin/nixtops gate --max-tier 1`: `gate: passed` (23 outputs evaluated).

## PR #36 CI (`x86_64-linux` with `sandbox = true`, and `aarch64-darwin`)

All 15 checks passed at `0a5642beddaf2663a0bfdeecaa903420c52cd054`:

| Workflow | Run ID | Job (ID) | Host / Nix | Reported `sandbox` | Observed result |
|---|---|---|---|---|---|
| `fixtures` | `45753406582` | `gate over the fixtures (CppNix, x86_64-linux)` (`139686276430`) | `ubuntu-26.04`, `cachix/install-nix-action` (`sandbox = true`) | `true` (`extra_nix_config: sandbox = true`; verified by `real_host_clean_flake_builds_outputs_and_records_sandbox` and `linux_requires_sandbox_true_and_reports_validate_against_schema`) | All unit + real-Nix tests (`build_fixtures` 4/4, `gate_fixtures`, `lint_fixtures`) and `scripts/fixtures.sh` passed (`gates/build-failing-check exit 1 ok`, `gates/build-failing-package exit 1 ok`, `gates/build-ifd exit 1 ok`, `gates/build-linux-only exit 0 ok`, where `x86_64-linux` is native host) |
| `fixtures` | `45753406582` | `gate over the fixtures (Determinate Nix)` (`139686276308`) | `ubuntu-26.04`, `DeterminateSystems/determinate-nix-action` (`start-daemon: true`) | `true` (verified on-runner by `real_host_clean_flake_builds_outputs_and_records_sandbox` and `linux_requires_sandbox_true_and_reports_validate_against_schema`) | All unit + real-Nix tests (`build_fixtures` 4/4, `gate_fixtures`, `lint_fixtures`) and `scripts/fixtures.sh` passed (`gates/build-failing-check exit 1 ok`, `gates/build-failing-package exit 1 ok`, `gates/build-ifd exit 1 ok`, `gates/build-linux-only exit 0 ok`) |
| `kit` | `45753406596` | `Determinate Nix` (`139686275939`) | `x86_64-linux`, `Determinate Nix 3.24.2 (reports Nix 2.35.3)`, `daemon 2.35.3`, judge `CppNix 2.35.6` | `sandbox: true` (`nixtops doctor`) | `nix flake check`, `nix build .#nixtops`, `build_fixtures` 4/4, `nixtops doctor` passed |
| `kit` | `45753406596` | `x86_64-linux (sandbox)` (`139686276307`) | `x86_64-linux`, `CppNix 2.35.6`, `daemon 2.35.6` | `sandbox: true` (`nixtops doctor`) | `nix flake check`, `nix build .#nixtops`, `build_fixtures` 4/4, `nixtops doctor` passed |
| `kit` | `45753406596` | `aarch64-darwin` (`139686276183`) | `aarch64-darwin`, `CppNix 2.35.6`, `daemon 2.35.6` | `sandbox: false` (`nixtops doctor`) | `nix flake check`, `nix build .#nixtops`, `build_fixtures` 4/4, `nixtops doctor` passed |
| `nix-tops` | `45753406639` | `gate (x86_64-linux, sandbox)` (`139686276342`) | `x86_64-linux`, `extra_nix_config: sandbox = true` | `true` | `gate: passed`; `layers_ran: all 7 layers accounted for (5 switched off)` |
| `nix-tops` | `45753406639` | `gate (Determinate Nix)` (`139686276523`) | `x86_64-linux`, `DeterminateSystems/determinate-nix-action` (`start-daemon: true`) | `true` | `gate: passed`; `layers_ran: all 7 layers accounted for (5 switched off)` |
| `nix-tops` | `45753406639` / `45753406650` | `law-diff (base branch script)` (`139686276580`, `139686276124`) | `ubuntu` | — | Passed |
| Rust suite (`45753406586`) | `45753406586` | `nextest`, `tops-gate`, `fmt-clippy-deny`, `feature-matrix (nixtops, --no-default-features)`, `coverage-crap`, `msrv` | `ubuntu-26.04` | — | `nextest`: `388 tests run: 388 passed, 0 skipped`; `tops-gate`: `388 tests run: 388 passed, 0 skipped`, `108 mutants tested in 16m: 90 caught, 18 unviable`, `ok: 8 command(s) passed` |
| `spec` | `45753406591` | `spec` (`139686275903`) | `ubuntu` | — | Passed |
