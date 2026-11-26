# First adoption: the Rust-TOPS flake

Recorded 2026-11-26 from [rust-tops pull request #67](https://github.com/Hedronite/rust-tops/pull/67),
head `b2c7704b67f0ff21b65682504e5c1dc23380708e` on base `794a082`. The pull request was **open and not
merged** when this was written. The kit is pinned at
`616cd22c7ab0acb56f2b4bfa8b905424bed745f6`.

## CI at the head

| Run | Result |
|---|---|
| [`nix-tops` 45899653753](https://github.com/Hedronite/rust-tops/actions/runs/45899653753) | `gate (x86_64-linux, sandbox)` passed in 850 s; `gate (Determinate Nix)` passed in 884 s. Gates 1 to 4, `sandbox: true`, one `statix` finding held by the baseline |
| [`rust-tops` 45899653835](https://github.com/Hedronite/rust-tops/actions/runs/45899653835) | All 16 jobs passed on the second attempt. On the first, `bend-proof` stopped while fetching the Bend binary (HTTP 504), before any proof ran; only that job was run again |
| [`law-diff` 45899653571](https://github.com/Hedronite/rust-tops/actions/runs/45899653571) | Passed |

[`ci-gate.txt`](ci-gate.txt) and [`ci-determinate.txt`](ci-determinate.txt) are the gate's own lines
from the two job logs. The jobs run `nixtops gate --max-tier 1 --system x86_64-linux` (the second with
`--evaluator host`), so CI judges the `x86_64-linux` outputs only: 12.

## The report

The workflow uploads no `report.json`, so [`report.json`](report.json) was produced from a fresh
clone at the head on `aarch64-darwin`, with `CI` unset and no `--system`:

```bash
nix run .#nixtops --no-update-lock-file --no-accept-flake-config -- gate --max-tier 1
```

It passed: gates 1 to 4 under `ratchet`, 40 outputs, 15 host outputs built and 22 deferred to other
systems. `sandbox` is `false` there; the gate requires the sandbox on Linux. The report validates
against [`schema/gate-report.schema.json`](../../../schema/gate-report.schema.json).
[`baseline.json`](baseline.json) is the file the pull request commits, written by `nixtops baseline`;
running it again gave the same bytes.

## What the adoption needed

- `flake.nix`: the `nix-tops` input without `follows`, `lib.mkChecks` merged into `checks`, the runner
  re-exported, and the two Bend checks reading a `lib.fileset.toSource` of their five files.
- `flake.lock`: the repository's own `nixpkgs` moved to `2786a2a` (rustc 1.99.1). The old lock had
  rustc 1.98.1, below the workspace's `rust-version`, so the package did not build.
- `nix/cargo-tops.nix`: `doCheck = false`. The package's tests call tools the build does not provide;
  the repository's CI runs them outside the sandbox.
- `nix-tops.toml`: `ratchet`; `coverage`, `facet-layers` and `laws` switched off with a reason. `init`
  leaves them on, and a layer that is on and not built fails the gate.
- `.gitignore`: `.nix-tops/report.json`, which `init` does not add.
- `nix/AGENTS.md`: the section `init` writes. The repository's root `AGENTS.md` is a generated copy.

## What it found in the kit

- The `nixtops-<tool>` verdict checks failed on any finding whatever the mode, so gate 4 and
  `nixtops baseline` failed on a finding the baseline was to hold. Fixed before the pull request
  ([#38](https://github.com/Hedronite/nix-tops/pull/38)); the pin includes the fix.
- Gate 4 reported that failure as `Invalid BuildResult status from remote`, without naming the
  derivation.
- The `AGENTS.md` section names `nix fmt` (this flake exports no formatter) and `laws run` (not
  built), and states coverage floors while the layer is off.
