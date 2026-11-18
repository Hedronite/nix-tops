# `nixtops outputs` over two pinned flakes

The kit's own command, run over `python-gate` at `2cb1775` and `rust-tops` at `77056c9` (full commits in `refs.txt`), once on a Determinate Nix 3.22.3 daemon host and once on a CppNix 2.35.2 daemon host. `run.sh <label> <nixtops>` is the whole procedure. Eval only: the runner uses the kit's pinned CppNix 2.35.6 with gate 3's options (`abort-on-warn` on, import from derivation off) and its own empty evaluation cache per run; no `--impure`, no unlocked reference. `nix-eval-jobs` 2.35.4 comes from the same lock. The binary is the build of this repo (`nix build .#nixtops`); on the CppNix host the same binary and its closure were copied over (the closure paths come from the binary cache; the runner itself is the file built on the Determinate host).

## Result

| Flake | Outputs | Errors | Same on both daemons |
|---|---|---|---|
| `python-gate` (`2cb1775`) | 13 | 0 | yes, byte for byte (JSON and text) |
| `rust-tops` (`77056c9`) | 35 | 6, all `x86_64-darwin` | yes, byte for byte |

`python-gate`: 3 apps, 3 checks, 3 dev shells, 3 packages (one each on `aarch64-darwin`, `aarch64-linux`, `x86_64-linux`) and the `default` template. Facets `dev-flake` and `package-set`.

`rust-tops`: 29 outputs evaluate or instantiate; 6 fail, in one run, none of them stopping the walk:

| Output | Failure |
|---|---|
| `checks.x86_64-darwin.cargo-tops`, `.law-catalog-names`, `.rust-tops-laws` | the `evaluation warning` about x86_64-darwin support, fatal under `abort-on-warn` |
| `devShells.x86_64-darwin.default` | same |
| `apps.x86_64-darwin.cargo-tops` | same (its `program` forces the package) |
| `packages.x86_64-darwin` | same, for the whole attribute set: the package set itself fails before any attribute exists, so the record is the set, not one package |

Compared with `docs/evidence/2026-11-16-gate3-trial/`: Determinate's `flake check` listed five failing `x86_64-darwin` outputs and 29 passing (the three checks, `devShells` and `apps`). The runner finds the same five and the same 29, and adds the sixth, the `packages` set, that `flake check` reports as the error that ends CppNix's run. CppNix's `flake check` stops at the first error and lists none of them.

## What the inventory does and does not check

- Derivation outputs (packages, checks, dev shells) come from `nix-eval-jobs`: one line per attribute, an `error` key on a failed one. A failure of the whole set has no line; the process exits non-zero and the runner records the set.
- Apps are evaluated (`type` is `app`, `program` is a string). Overlays are applied to the kit's locked nixpkgs and the result must evaluate; modules are evaluated with the module system of that nixpkgs; templates are copied to a scratch directory and their flake must evaluate there; configurations give their build root's `.drv`. `legacyPackages`, `lib` and names the runner does not know are listed, not evaluated. Coverage of every output is `not-evaluated` in this milestone.
- The apps check does not build the program, so "`program` resolves to a file in the built output" (section 6.1) waits for the gate step that builds.
- Template evaluation resolves the template's own inputs, which are not locked until a user instantiates it; `python-gate`'s template passed with network access.

## One difference found and fixed on the way

The first run on the CppNix host listed every `nix-eval-jobs` output as an error, `experimental Nix feature 'flakes' is disabled`: `nix-eval-jobs` reads the host's `nix.conf` and takes no `--extra-experimental-features` flag, and that host's config enables `nix-command` only. The runner now passes the same setting with `--option`. This is a host-configuration difference between the two machines, not a daemon difference; the daemon does not evaluate.
