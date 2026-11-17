# Warning sources under gate 3 (evidence for G2)

Run date 2026-11-17. Eval only: no builds, never `--impure`, never an unlocked reference.

| Implementation | Version | System |
|---|---|---|
| CppNix | 2.35.2 | aarch64-darwin |
| Determinate Nix | 3.22.3 (nix 2.35.2) | aarch64-darwin |

Pins (every input by 40-hex rev, recorded in each fixture's `flake.lock`):

- nixpkgs `1c872b8c508bbbfebc1b0b74777ff391ce0a574c` (nixpkgs-unstable, 2026-11-15)
- Home Manager `9d7ba6bef3904a018d38b92f83132eaf01eb8ef3` (its `nixpkgs` follows the one above)

`run.sh <cppnix|determinate>` runs every fixture twice per command: once with gate 3's options
(`--no-update-lock-file --option abort-on-warn true --option allow-import-from-derivation false`,
NIX_TOPS.md section 3.1 line 103) and once as a control with `abort-on-warn false`. Each fixture is
copied to a temp directory and addressed as `path:`. Logs are `logs/<impl>/<fixture>.<cmd>.log`, exit
codes `*.exit` and `EXIT-CODES.txt`, versions in `VERSION.txt`. "gate" below is the gate-3 run, "ctrl"
the control.

Commands: `check` is `nix flake check --no-build --all-systems`; `drv` is `nix eval --raw` of the
build root's `drvPath` (`activationPackage` or `config.system.build.toplevel`, as gate 3 does);
`data` is `nix eval --json` of `config.warnings`.

## Results

Exit codes are identical on both implementations for every command.

| Fixture (source) | Command | Exit, gate | Exit, ctrl | What was printed |
|---|---|---|---|---|
| `builtins-warn` (1: `builtins.warn`) | check | **1** | 0 | `evaluation warning: fixture builtins.warn`; under the gate it is an error |
| | drv | **1** | 0 | same |
| `hm-standalone` (2: renamed HM option, standalone) | check | 0 | 0 | CppNix: `homeConfigurations` is listed but not evaluated; Determinate: `homeConfigurations.fixture (build skipped)` with the `trace: warning:` line |
| | drv | **0** | 0 | `trace: warning: The option 'programs.carapace.ignoreCase' ... has been renamed to ...`; **exit 0 under `abort-on-warn`** |
| | data | 0 | n/a | `config.warnings` holds the rename message, readable as data |
| `hm-nixos-module` (2: same option through HM as a NixOS module) | check | 0 | 0 | the configuration is checked, no warning, exit 0 |
| | drv | **1** | 0 | `evaluation warning: fixture profile: The option ... has been renamed ...`; error under the gate |
| | data | 0 | n/a | `config.warnings` holds the same message with the `fixture profile:` prefix |
| `app-no-meta` (3: app without `meta`) | check | 0 | 0 | CppNix: `warning: app 'apps.x86_64-linux.default' lacks attribute 'meta'`; Determinate: no warning at all |
| `unknown-output` (3: unknown output) | check | 0 | 0 | CppNix: `warning: unknown flake output 'fooBar'`; Determinate: `warning: The following flake outputs are unchecked: fooBar.` |

## What this shows

1. `abort-on-warn` catches `builtins.warn` (exit 1 on both implementations).
2. A renamed Home Manager option in a **standalone** configuration evaluates to exit 0 under gate 3's
   options: Home Manager prints it with `builtins.trace` (`modules/default.nix:16-21` at `9d7ba6b`).
   The message is only in `config.warnings`, so the runner must read that list as data.
3. The same option through Home Manager as a **NixOS module** is forwarded into NixOS `warnings` and is
   fatal, but only when the build root's `drvPath` is evaluated: `nix flake check --no-build` on the
   same flake exits 0 on both implementations. Gate 3's second step is load-bearing.
4. Diagnostics from the `nix` command itself (app without `meta`, unknown output) exit 0, differ in
   wording between implementations, and Determinate prints nothing for the app. A table classified by
   implementation is needed (`laws/diagnostics.toml`, written in G7).

## Fixture choice

`programs.zsh.enableAutosuggestions`, the option the template cites for N-17, is a **removed**
option at Home Manager `9d7ba6b` (`modules/programs/zsh/default.nix:35-39`, `mkRemovedOptionModule`):
it fails as an assertion, not a warning, so it cannot show the warning path. The fixtures use
`programs.carapace.ignoreCase`, still a `mkRenamedOptionModule` at that commit
(`modules/programs/carapace.nix:32-35`).
