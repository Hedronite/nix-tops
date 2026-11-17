# Gate 3 trial, 2026-11-16 (re-run 2026-11-17)

The first run of the gate-3 command in `NIX_TOPS.md` section 3.1 against real flakes, kept as evidence and reproducible from this directory.

- **Command (verbatim from section 3.1, gate 3, eval only):**
  `nix flake check --no-build --all-systems --no-update-lock-file --option abort-on-warn true --option allow-import-from-derivation false github:<owner>/<repo>/<sha>`
- **Subjects:** eight public flakes, each pinned by a 40-hex commit in `refs.txt`.
- **Implementations:** CppNix 2.35.2 and Determinate Nix 3.22.3 (reports Nix 2.35.2). The raw `nix --version` line is in `logs/<impl>/version.txt`.
- **Runs:** first on 2026-11-16, re-run for this directory on 2026-11-17 on both implementations. Logs and counter JSON here are from the 2026-11-17 re-run.
- **Safety:** eval only (`--no-build`), no `--impure`, no unlocked reference. Flakes and `nix-command` are enabled with `--extra-experimental-features`, which CppNix needs and Determinate accepts.

## Reproduce

```
./trial.sh cppnix         # or: ./trial.sh determinate
./counters.sh cppnix      # or: ./counters.sh determinate
```

`trial.sh` refuses a ref that is not 40-hex, writes `logs/<impl>/gate3-<flake>.log`, `exit-codes.txt` and `summary.txt` (the matching warning and error lines, counted). `counters.sh` writes `counters/<impl>/` (`s*.json`, `summary.txt`). Both need network access to GitHub and the NixOS binary cache, and write nothing outside this directory except the Nix store and caches, plus a throwaway cache directory that `counters.sh` removes.

## Gate 3 outcome per flake

Exit codes are in `logs/<impl>/exit-codes.txt`. "Fail" means exit 1. The count of failing outputs on Determinate comes from the `❌` lines of its log; CppNix stops at the first error and prints no per-output verdicts.

| Flake (commit) | CppNix 2.35.2 | Determinate 3.22.3 | Cause |
|---|---|---|---|
| `rust-tops` (`77056c9`) | **fail** | **fail**: 5 outputs (all `x86_64-darwin`), 29 pass | The `nixpkgs-26.05-darwin` input prints `evaluation warning: Nixpkgs 26.05 will be the last release to support x86_64-darwin`; `abort-on-warn` makes it fatal |
| `bezel` (`a2c9740`) | **fail** | **fail**: 29 outputs (all `x86_64-darwin`), 88 pass | Same warning |
| `luminosi-ts` (`c86e840`) | **fail** | **fail**: 1 output (`devShells.x86_64-darwin.default`), 3 pass | Declares `x86_64-darwin` on `nixos-unstable`; nixpkgs throws `Nixpkgs 26.11 has dropped support for x86_64-darwin` |
| `lapis-lattice` (`42aa3b8`) | **fail** | **fail**: 5 outputs (all `x86_64-darwin`), 15 pass | Same throw |
| `hedronetes-h3s` (`50eb729`) | **fail** | **fail** | `packages."x86_64-linux"` throws by design (`bundled runtime artifacts are aarch64-linux only`) |
| `python-gate` (`2cb1775`) | pass, with 3 warnings on screen (`app 'apps.<system>.default' lacks attribute 'meta'`) | pass, no warning | The three warnings are not fatal under `abort-on-warn`; Determinate does not print them |
| `Omahedron` (`35dda32`) | pass, with `warning: unknown flake output 'homeManagerModules'` | pass, with `warning: The following flake outputs are unchecked: homeManagerModules.` | Output the evaluator does not check, reported with different wording; exit 0 on both |
| `facet` (`398fa4c`) | pass | pass | |

Two flakes pass with warnings printed and the two implementations print different warnings for the same flake, so "no warning" is not what `abort-on-warn` delivers. CppNix stops at the first error; Determinate checks every output and keeps going, so one implementation names one failing output and the other names all of them.

## Eval counters

`counters.sh` evaluates the `rust-tops` devShell drvPath (`rust-tops` at `77056c9`) with `NIX_SHOW_STATS=1`. Counters below are `nrFunctionCalls / nrThunks`; the JSON also holds `nrLookups` and `nrPrimOpCalls`. File names are in `counters/<impl>/`.

| Run (files) | CppNix 2.35.2 | Determinate 3.22.3 |
|---|---|---|
| A: `aarch64-darwin` devShell, no eval cache, three runs (`sa1`-`sa3`) | 283,686 / 779,909, identical in all three | 283,739 / 780,032, identical in all three |
| B: same, `--option eval-cores 1`, two runs (`sb1`, `sb2`) | 283,686 / 779,909, same as A (the option has no effect) | 283,739 / 780,032, same as A |
| C: eval cache on, empty cache directory, cold then warm (`sc1`, `sc2`) | 283,686 / 779,909 then 283,686 / 779,909: **no change** | 283,739 / 780,032 then 262,075 / 639,170: **warm run -7.6% calls, -18.1% thunks** |
| D: `x86_64-linux` devShell, no eval cache, two runs (`sd1`, `sd2`) | 170,309 / 552,385, identical | 170,362 / 552,508, identical |
| Cross-system delta (A against D) | 170,309 calls against 283,686: **-40.0%** | 170,362 calls against 283,739: **-40.0%** |
| E: `NIX_COUNT_CALLS=1` (`se`) | 1,622 function entries, 21 outside nixpkgs | 1,628 function entries, 23 outside nixpkgs |

Reading: counters are deterministic from run to run on each implementation, differ between the two implementations (Determinate reports 53 more function calls and 123 more thunks for the same output), differ with the eval cache on Determinate and not on CppNix, and differ by system. The eval-cache effect on Determinate is only visible when the cache starts empty; `counters.sh` points `XDG_CACHE_HOME` at a fresh directory for run C for that reason.

## Differences from 2026-11-16

None in outcome or counters. Every flake has the same exit code on both implementations, the same warning and error text, and every counter above equals the 2026-11-16 figure (CppNix 283,686 / 779,909, Determinate 283,739 / 780,032, warm Determinate 262,075 / 639,170, `x86_64-linux` 170,309 on CppNix).

Differences in the log text only:

- Fewer `unpacking ... into the Git cache` and `copying path ... from 'https://cache.nixos.org'` lines in the 2026-11-17 logs, because the flake sources and inputs were already in the store and fetch cache. Cause: warm caches. No effect on any verdict.
- Determinate logs list the same per-output lines in a different order. Cause: Determinate evaluates outputs in parallel (`eval-cores = 0`), so line order varies. The set of outputs and verdicts is unchanged.

Not carried over from the 2026-11-16 directory: the `e1.sh` first draft (it ran the same command against unpinned `main`) and the lint-option, `lib.warn` alias, tool-version and `x86_64-darwin` probes (`e3.sh` sections F to J). The assessment cites those; they are not part of the section 0 table or the counter table.
