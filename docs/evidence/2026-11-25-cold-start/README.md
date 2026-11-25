# Cold start of the runner, empty store

How long `nix run .#nixtops -- version` takes in `fixtures/adopter/` on a machine that has Nix and nothing of the kit. **All six runs are far over 60 s: 7 min 24 s to 12 min 36 s.** The figure is reported, not gated.

## What ran

Workflow `cold-start`, run 45569398652, at commit `fb83427beee272b1feabc161b311ad53f5423df6`. Six jobs, each on a fresh hosted machine.

- **Command:** `nix run .#nixtops --no-update-lock-file --no-accept-flake-config -L -- version`, with an empty `XDG_CACHE_HOME`.
- **Nix:** CppNix 2.35.6, installed by the pinned `cachix/install-nix-action`. The time to install Nix is not counted. Determinate Nix was not measured.
- **Machines:** `ubuntu-latest` (x86_64-linux, 4 CPUs, `sandbox = true`) and `macos-15` (arm64-darwin, 3 CPUs, `sandbox = false`).
- **Substituter:** `https://cache.nixos.org/` only. No project binary cache, no prebuilt runner, no cache action.
- **Locks:** the adopter's own nixpkgs `2786a2a7`, the kit's nixpkgs `04906337`, the kit as the relative path `../..`. An adopter using a `github:` input would also download the kit's source, which is not measured here.
- **Empty store:** `lists/<image>/store-before.txt` is the store before the timed command: 72 paths (Ubuntu) and 65 (macOS), all from installing Nix. None is the runner, `rustc` or `cargo`.

## Times

Seconds. "Last fetch" is when the last substituted path started copying; the two build columns are the phases Nix reports for the `nixtops` derivation.

| Image | Run | Total | Last fetch | `buildPhase` | `checkPhase` | `checkPhase` share |
|---|---|---|---|---|---|---|
| ubuntu-latest | 1 | 755.5 | 27 | 127 | 601 | 80% |
| ubuntu-latest | 2 | 493.4 | 14 | 84 | 395 | 80% |
| ubuntu-latest | 3 | 502.3 | 26 | 84 | 390 | 78% |
| macos-15 | 1 | 443.9 | 50 | 68 | 323 | 73% |
| macos-15 | 2 | 686.7 | 99 | 105 | 478 | 70% |
| macos-15 | 3 | 536.3 | 64 | 77 | 392 | 73% |

## Fetched and built

The lists were identical in all three runs of an image, so each is kept once under `lists/<image>/`. Each line is a store path and its unpacked size in bytes.

| Image | Fetched from the cache | Download, unpacked (Nix's own plan) | Downloaded by a fixed-output build | Built on the machine |
|---|---|---|---|---|
| ubuntu-latest | 286 paths | 770 MiB, 2.6 GiB | 14 crate archives | 20 derivations |
| macos-15 | 283 paths | 798 MiB, 3.1 GiB | 14 crate archives | 26 derivations |

The plan in the log counts two paths fewer than the list: the two nixpkgs source trees are fetched during evaluation, before the plan is printed.

Built on the machine: the `nixtops` package itself, `cargo-vendor-dir`, and the unpacking of the crates that went into it. The 14 crate archives were not in the cache and came from crates.io.

The largest fetched paths are the build toolchain: `rustc` (about 1.0 GiB unpacked), LLVM (about 590 MiB on Linux; about 1.3 GiB with `clang` on macOS), `gcc` on Linux, the Apple SDK on macOS, and `python3`. The two nixpkgs source trees are about 210 MiB each.

## Cause

The runner is not substituted: no cache holds it, so the adopter compiles it. Compiling means first fetching the Rust toolchain, then running two release builds of the crate.

1. `checkPhase` is 70 to 80 percent of the total. The package runs `cargo test` in the release profile (`codegen-units = 1`, thin LTO), which compiles the crate and its test binaries again before running them. The tests themselves take seconds.
2. `buildPhase` alone is 68 to 127 s.
3. Fetching ends after 14 to 99 s.

So no run could be under 60 s while the runner is compiled on the adopter's machine: `buildPhase` by itself exceeds 60 s in every run.

## Options, for the owner to decide

None of these is applied or measured here. No cache and no prebuilt binary were added.

| Option | What it would remove | Reaches 60 s? | Cost |
|---|---|---|---|
| Do not run the tests when an adopter builds the package; run them in a separate `checks` derivation | `checkPhase` | No. Subtracting it from the totals above leaves 98 to 209 s (arithmetic, not a measurement) | A change to `nix/nixtops.nix`; the tests must then be built somewhere the kit's CI requires |
| A cheaper release profile for the packaged runner | Part of the compile time | Not known **[verify]**: the toolchain fetch and a compile remain | A slower or larger runner **[verify]** |
| A binary cache for the runner | The compile and the toolchain fetch | Possibly **[verify]** | An owner for the cache and its signing key; adopters must trust that key |
| A prebuilt binary attached to releases | The compile and the toolchain fetch | Possibly **[verify]** | Releases, and a trust and update path outside the lock |
| Keep it as it is and document the wait | Nothing | No | Each adopter machine with an empty store waits 7 to 13 minutes |

The first option can be combined with any of the others.

## Files

- `runs/<image>-<n>/result.json`: the time, versions, settings and counts of one run.
- `runs/<image>-<n>/run.log`: the command's stderr, each line prefixed with seconds since the start. Terminal control codes are removed.
- `lists/<image>/`: `store-before.txt`, `fetched.txt`, `downloaded.txt`, `built.txt`.
