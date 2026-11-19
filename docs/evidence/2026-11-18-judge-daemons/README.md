# The pinned CppNix judge on two daemons

Protocol section 2 says the judge is the CppNix pinned in the kit's `flake.lock`, and the host supplies only the daemon and the store. This run shows the pinned client evaluating the kit against a Determinate Nix daemon and against a CppNix daemon, with the same result.

## What ran

`run.sh <label> <clone> <rev>`, once per daemon, on the kit at commit `5651e65acc0e42d6593a465f062b4b22de7a37cd` (the last commit before this directory was added; the evaluated flake does not include it).

- **Judge:** CppNix 2.35.6, `nixVersions.nix_2_35` from the nixpkgs revision in `flake.lock` (`04906337b6d53e8bb0eaf0470ae9f070a4d4eb55`), the same store path in both runs: `/nix/store/p959r3d926iaqiyjpd0s3mbzppcxfmck-nix-2.35.6`.
- **Daemon A:** Determinate Nix 3.22.3 (reports Nix 2.35.2), `nix store info` says `daemon` 2.35.2. Log directory `logs/determinate-daemon/`.
- **Daemon B:** CppNix 2.35.2, `daemon` 2.35.2. Log directory `logs/cppnix-daemon/`.
- **Commands (eval only, no `--impure`, no unlocked reference, `--no-accept-flake-config`, an empty `XDG_CACHE_HOME` per run):** `nix flake check --no-build --all-systems --no-update-lock-file --option abort-on-warn true --option allow-import-from-derivation false`; `nix flake show --json --all-systems`; `nix eval --raw` of `packages.<system>.nixtops.drvPath` for the three systems.

## Result

| Check | Determinate daemon | CppNix daemon |
|---|---|---|
| `flake check` exit | 0 | 0 |
| `flake show --json --all-systems` | identical | identical |
| `.drv` path of `packages.<system>.nixtops` (three systems) | identical | identical |
| `flake check` log | two extra warnings, below | none |

The same flake evaluates to the same derivations under both daemons, so the verdict does not depend on the host's implementation.

## Difference recorded

The pinned CppNix client prints `warning: unknown setting 'eval-cores'` and `warning: unknown setting 'lazy-trees'` on the Determinate host: the Determinate installer's `/etc/nix/nix.conf` sets two Determinate-only settings, and CppNix does not know them. Cause: the host's managed config is read by whichever client runs. The warnings come from the config file, not from the flake, and `abort-on-warn` did not turn them into a failure (exit 0, "all checks passed!"). The run also differs by one `copying path ... source` line, a cache fetch on the first run on one host. Whether `laws/diagnostics.toml` already classifies these two lines is not checked here **[verify]**; the gate step meets them first when it runs the judge on a Determinate host.
