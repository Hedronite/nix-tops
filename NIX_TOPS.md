# Nix-TOPS: Nix Testing and Optimization Protocol Suite

**Spec status:** Draft 0.1.0 · **Date:** 2026-11-14 · **Owner:** Hedronite
**Modeled on:** [Rust-TOPS](https://github.com/Hedronite/rust-tops), [Go-TOPS](https://github.com/Hedronite/go-tops) 0.3.0 and [Web-TOPS](https://github.com/Hedronite/web-tops) 0.2.0
**Source of the laws:** *Common Mistakes AI Coding Agents Make in Nix* (Hedronite sketch). Appendix B lists where this spec corrects it.

> **How to read this.** Nix-TOPS reuses the shape of the other TOPS protocols: a written spec, a
> JSON Schema, per-class profiles, an orchestrator, a CI floor, and a laws layer. It starts from
> the Go-TOPS 0.3.0 fixes: `not-run` fails, ratchet for adopters, laws check that their tool
> config is live, no formal layer. Nix is not a general-purpose language with line coverage, so
> three gates change shape (§6): coverage becomes **output coverage**, CRAP is replaced by
> **closure and eval-cost budgets**, and there is **no mutation layer** in 0.1. Values labeled
> **proposed** are defaults to tune on real flakes. **[verify]** marks a tool behavior not yet
> confirmed at pin time. Versions in §2 were read from nixpkgs 26.11 (`nixpkgs-weekly`,
> 2026-11-01) on 2026-11-14. Statements marked **(checked)** were run on that pin with Nix 2.35.2.

---

## 1. What Nix-TOPS is

**Nix-TOPS** is a protocol and drop-in kit for Nix code that is pure, correct, dense and fast. It
covers the Nix language and what it produces: flakes, packages and overlays, NixOS, Home Manager
and nix-darwin modules and configurations, and the flake that sits in a Rust, Go or web repository.

Nix is a lazy functional language with a package set of more than 100,000 attributes and a
network-isolated build sandbox. Agents get it wrong in predictable ways: impure lookups, files
that git does not track, invented `lib` functions and option names, fabricated or stale hashes,
network access in build phases, secrets copied into the world-readable store. Most of those
mistakes are mechanically detectable. Nix-TOPS makes them laws.

### 1.1 Goals

- Make "done" measurable: a change is acceptable when the gate passes, and the gate is the same
  for humans and agents.
- Judge what Nix judges: pure evaluation on every declared system, with warnings fatal and
  import-from-derivation off, is the cheapest strong check Nix has. Use it everywhere.
- Encode Nix's real hazards (impurity, stale fixed-output hashes, store-path context loss,
  secrets in the store, sandbox escapes) as checkable **laws**.
- Keep artifacts dense: closure size and evaluation cost are budgets, ratcheted.
- Be honest: a layer that was not evaluated is reported as not evaluated, never as a pass.

### 1.2 Non-goals

- Not a deployment tool. Nix-TOPS does not replace `nixos-rebuild`, colmena or deploy-rs.
- Not a style guide. Formatting belongs to nixfmt; Nix-TOPS only requires that it passes.
- Not a nixpkgs contribution checker. `nixpkgs-vet` and the nixpkgs CI do that for nixpkgs itself.
- Not a replacement for review. Oracle-first testing and several module laws are attested.

### 1.3 Name and binary

- Protocol: **Nix-TOPS**. Binary: **`nixtops`**. Repo: `github.com/Hedronite/nix-tops`.
- The kit is itself a flake. An adopter adds it as an input and re-exports its runner as
  `apps.<system>.nixtops` (§4), so one command runs the gate: `nix run .#nixtops -- gate`. The
  command resolves the kit through the adopter's `flake.lock`. No command in this spec resolves an
  unlocked reference, and there is no `nix run github:...` form.

### 1.4 Two planes: the sandbox and the runner

Nix can check most things inside the build sandbox, where results are hermetic and cached. A few
things need the world outside it. Nix-TOPS splits the work that way:

| Plane | What runs there | Why |
|---|---|---|
| **Sandbox** (`checks.<system>.nixtops-*`) | format, statix, deadnix, the `nixtops-lint` analyzer, secrets scan, nix-unit, VM tests, closure-size reader | Hermetic and cacheable. A check that passed for a given input is not rerun |
| **Runner** (`nixtops gate`) | git-tracked files, lock state, pure evaluation of every output on every system with `abort-on-warn`, the `config.warnings` and diagnostics reads (§3.1), eval-cost counters, diff-based laws, the report | These need git, the evaluator's own flags, or the diff. None of them can run in a derivation |

`nix flake check` alone runs the sandbox plane. It is useful, but it is not the gate: it does not
know about untracked files, does not set `abort-on-warn`, and does not write a report.

The kit's `lib.mkChecks` returns an attrset per system (`checks.<system>.nixtops-*`) that the
adopter merges with its own checks (§4). The lint checks exist for one system only: they read
source, which does not depend on the system, so they are built once.

---

## 2. Pins

| Item | Value |
|---|---|
| Protocol version | **0.1.0** (draft) |
| Package version | 0.0.0 (unreleased) |
| Evaluator (the judge) | The CppNix pinned in the kit's `flake.lock` (2.35.2 when this spec was checked). The host supplies the daemon and the store, not the judge (§10) |
| Determinate Nix | A named CI leg (§11), checked with 3.22.3 (Nix 2.35.2). Determinate-only settings are off in the gate: `--option lazy-trees false --option eval-cores 1` (the Determinate binary defaults to `lazy-trees false` and `eval-cores 1`; the managed `/etc/nix/nix.conf` from the Determinate installer turns both on, checked on 3.22.3) |
| Nix floor (host daemon) | CppNix **2.24**, or Determinate Nix 3.x. `builtins.warn` and `abort-on-warn` landed in 2.23. `doctor` gates only on this floor (§12) |
| Lix | Lix 2.95.0 has `builtins.warn` and `abort-on-warn` **[verify]**. Not supported until M7.2 |
| Supported systems | `aarch64-darwin`, `x86_64-linux`, `aarch64-linux`. `x86_64-darwin` is unsupported: nixpkgs 26.11 dropped it (N1) |
| Baseline key | (implementation, version, system, `flake.lock` hash) |
| nixpkgs (kit's own) | 26.11 (`nixos-unstable` line); the kit's tools come from this pin, not the adopter's (§10) |
| Format | `nixfmt` 1.5.0 through `nixfmt-tree` (treefmt). `nixfmt-rfc-style` is now an alias that warns (checked) |
| Lint | `statix` 0.5.8-unstable-2026-07-17, `deadnix` 1.3.2, `nixtops-lint` (the kit's analyzer) |
| Tests | `nix-unit` 2.35.1; NixOS test driver (`testers.runNixOSTest`) from the adopter's nixpkgs |
| Secrets | `gitleaks` 8.30.1 |
| Vulnerabilities | `vulnix` 1.12.5 (tier 2) |
| Schema | `check-jsonschema` 0.38.0 |
| Optional | `nix-fast-build` 2.0.4 for parallel builds in CI; `nix-tree` 0.8.0 for closure inspection |

Protocol and package versions are separate planes. A protocol change bumps the protocol version
and the schema together.

---

## 3. Protocol model

### 3.1 Gate order

Gates run in this order. A failing gate stops the run unless `--keep-going` is set.

| # | Gate | Command(s) | Pass condition |
|---|---|---|---|
| 1 | **Source** | runner: every `.nix` file under the flake root is tracked by git; `flake.lock` is committed; evaluation runs with `--no-update-lock-file`; the tree is clean in CI | No untracked `.nix`; lock current |
| 2 | **Lint** | sandbox: `checks.<system>.nixtops-lint` runs `treefmt --ci` (nixfmt), `statix check`, `deadnix --fail`, `nixtops-lint`, `gitleaks` | All clean, or within ratchet |
| 3 | **Eval** | runner, three warning sources. (a) `nix flake check --no-build --all-systems --no-update-lock-file --option abort-on-warn true --option allow-import-from-derivation false`, then each configuration's build root (`config.system.build.toplevel.drvPath`, `activationPackage.drvPath`) under the same options; `abort-on-warn` makes every `builtins.warn` fatal. (b) `config.warnings` of every NixOS, Home Manager and nix-darwin configuration, read as data. (c) the `nix` command's own diagnostics, captured and classified by `laws/diagnostics.toml`. Then record eval-cost counters (§6.4) | No error; no warning from any source that `[[warnings.allow]]` does not match; no IFD |
| 4 | **Build and test** | `nix flake check` (builds every `checks` derivation: nix-unit, module eval tests, VM tests) and `nix build` of every `packages`, `devShells` and configuration build root for the host system. Linux runs with `sandbox = true` | Everything builds; every test passes |
| 5 | **Output coverage** | runner: maps each exported output to the checks that exercise it (§6.1) | Class floor or ratchet met |
| 6 | **Class layers** | closure-size budgets, eval-cost budgets; at tier 2, rebuild determinism, `vulnix`, cross-system builds | Per class (§7) |
| ★ | **Laws** | maps findings from gates 1–6 to law IDs, then runs the eval and diff readers (§8) | No `Refuse`; `not-evaluated` never counts as a pass |

★ The laws layer is tier 0 but executes last. It does not run statix or deadnix again: it reads
the JSON they wrote in gate 2.

Gate 3 is the most important gate in this protocol. Evaluating a NixOS configuration's
`toplevel.drvPath` makes the module system refuse every option that does not exist. A warning,
unlike an error, has three sources, and `abort-on-warn` covers one of them:

1. **`builtins.warn`** (and `lib.warn`, which delegates to it). `abort-on-warn true` turns each one into
   an error. This is what makes a renamed NixOS option fatal. Checked: exit 1 on CppNix 2.35.2 and
   Determinate Nix 3.22.3, for a flake output and for a NixOS configuration that includes Home Manager
   as a module (`docs/evidence/2026-11-17-warning-sources/logs/*/builtins-warn.*`, `docs/evidence/2026-11-17-warning-sources/logs/*/hm-nixos-module.drv.gate`).
2. **`config.warnings`.** Home Manager and nix-darwin print the list with `builtins.trace`, not
   `builtins.warn` (Home Manager `9d7ba6b` `modules/default.nix:16-21`, run in the fixtures below;
   nix-darwin `ff0e21a` `modules/system/default.nix:14`, read in source at that commit, not run **[verify]**). A renamed option in a standalone Home Manager configuration
   evaluates to exit 0 under gate 3's options on both implementations
   (`docs/evidence/2026-11-17-warning-sources/logs/*/hm-standalone.drv.gate`). The runner therefore evaluates `config.warnings` of every NixOS, Home
   Manager and nix-darwin configuration as data and requires the list to be empty or matched by the
   ledger. It does not trust `nix flake check` for this: `nix flake check --no-build` on a NixOS
   configuration whose Home Manager module has a renamed option exits 0 on both implementations
   (`docs/evidence/2026-11-17-warning-sources/logs/*/hm-nixos-module.check.gate`); only evaluating `toplevel.drvPath` is fatal. Checked on nixpkgs
   `1c872b8c` and Home Manager `9d7ba6b`.
3. **Diagnostics from the `nix` command.** Unknown flake outputs, apps without `meta`, omitted systems.
   They are log lines and the exit code stays 0, and the two implementations differ: for an unknown
   output CppNix 2.35.2 prints `unknown flake output 'fooBar'` and Determinate 3.22.3 prints `The
   following flake outputs are unchecked: fooBar.`; for an app without `meta` CppNix prints a warning
   and Determinate prints none (`docs/evidence/2026-11-17-warning-sources/logs/*/app-no-meta.check.gate`). The runner captures the command's output and
   classifies each line with a versioned table, `laws/diagnostics.toml` (one entry per diagnostic per
   implementation, each with a fixture string; written in G7). A line the table does not know fails the
   gate as an unclassified diagnostic.

**The ledger.** `nix-tops.toml` may hold `[[warnings.allow]]` entries:

| Field | Meaning |
|---|---|
| `match` | Text that must occur in the warning message (substring, not a regex) |
| `reason` | Why the warning is accepted; required |
| `until` | Date (`YYYY-MM-DD`, UTC) after which the entry no longer applies; required |

An entry is an Ack (§3.3) with an expiry. It matches warnings from sources 1 and 2 (source 3 is
classified by the table, not matched by text). An entry whose `until` is before the run date fails the
gate, whether or not its warning still appears, and matches nothing: the ledger must be cleaned, and no
expired entry passes silently. The report names the entry and its expiry.
Warnings without a match fail the gate.

**Every warning in one run.** The `abort-on-warn true` run stays the judge. Source 1 is fatal, so the
first `builtins.warn` ends it; only after such a failure the runner re-runs the same locked invocation
with `abort-on-warn false` and every other option unchanged (IFD off, never `--impure`), only to
collect each `evaluation warning:` line, and the verdict stays fail unless every collected warning
matches an unexpired ledger entry (both implementations print the lines and exit 0 on the re-run,
checked on CppNix 2.35.2 and Determinate Nix 3.22.3, `docs/evidence/2026-11-17-warning-sources/` "The collect
re-run"). The report lists every warning with its source and the matching entry, so a host with five
renamed options costs one run. The ledger is for upstream warnings with a dated reason, not a standing
escape: dropping `x86_64-darwin` (N1) removes the `rust-tops` case that motivated it (§20).

### 3.2 Tiers

`nixtops gate` runs every enabled layer at or below `--max-tier` (default **1**).

| Tier | Purpose | Contents |
|---|---|---|
| **0** | Floor | Laws; config and schema validation |
| **1** | **Merge gate** (default) | Gates 1–6 on the host system; eval (gate 3) on every declared system; re-fetch of the fixed-output derivations whose fetch inputs changed against the merge base (§8.2 N-26; needs network in the runner, not in the sandbox) |
| **2** | Deep / nightly | Builds on every declared system; `nix build --rebuild` determinism on budgeted outputs; `vulnix` on runtime closures; fixed-output re-fetch of every fetcher (`--rebuild`, §8.2 N-26) |
| **3** | Release | Signed binary cache upload check; SBOM of runtime closures **[verify tool]**; tag matches flake `version` where declared |

### 3.3 Verdicts

<!-- tops-core:begin -->
| Verdict | Meaning | Effect |
|---|---|---|
| `pass` | Evaluated and satisfied | none |
| `Refuse` | Hard rule violated | **Fails the gate** |
| `Ack` | Soft rule; needs a recorded acknowledgement (see Acks below) | Fails if unacknowledged |
| `skip` | The facet's artifact is absent (the reason names it), or config sets `enabled = false` | Printed with the reason |
| `not-run` | The layer is enabled but has no command mapped | **Fails the gate** |
| `not-evaluated` | The reader or tool could not judge, or its config is off | Reported; never a pass. **Fails the gate under `hard` and `ratchet` when the law is required** |
| `deferred` | The output cannot be built on this host and a `build-matrix` leg owns it (§11) | Printed locally. **CI fails while any output is `deferred` that no leg built** |
| `vacuous` | A diff law ran with nothing to compare: an empty diff, or no merge base | Printed; never counted as a pass for another law (see below) |
<!-- tops-core:end -->

**Required laws.** A law is *required* when it is judged in this version (✓ in §8.2) and its
artifact exists (§5.1, §8.2). `not-evaluated` on a required law fails the gate under `hard` and
`ratchet`: a disabled lint or a broken reader cannot turn a refusal into a line in a report. Under
`ratchet` the baseline stores the set of judged law IDs, and losing one from the set fails. Laws
outside the required set are reported `not-evaluated` and do not block. A broken upstream tool
therefore blocks merges until it is fixed or exempted; that is the intent.

`not-evaluated` is the verdict and `not_evaluated` is its key in the JSON report (§6.7): one name
for one thing, with the hyphen written as an underscore. The same holds for `not-run` and `not_run`.

**`deferred`** exists because a host cannot always build what the flake exports: a NixOS
configuration has no buildable output on an `aarch64-darwin` machine. The local run reports it and
goes on; it never counts as built.

**`vacuous`** is for the diff laws. With a merge base present and an empty diff, a diff law is
`vacuous` and passes: there was nothing to compare, and the report says so. With no merge base
(a shallow clone, or unrelated history) the law is `vacuous` outside CI and **fails in CI**, which
checks out with `fetch-depth: 0` (§11).

<!-- tops-core:begin -->
**Acks.** An Ack is recorded in the source (`# nixtops-ack N-NN: reason` on the line above) or in
the ledger. A ledger entry carries a reason and an expiry date, and an expired entry fails (§3.1).
The number of acks is in the baseline and does not rise under `ratchet` (§6.8).
<!-- tops-core:end -->

**Sandbox state is evidence.** On a host where `sandbox` is not `true` (the default on macOS;
Determinate Nix 3.22.3 on macOS reported `sandbox = false`, checked), a successful build does not
prove the build is network-free. The build layer still runs, and N-27's build evidence is `not-evaluated` for that
host. Linux CI with `sandbox = true` is the authoritative build.

**Family terms.** The marked blocks in this section and in §6.8 are the family core, shared
with the other TOPS protocols. Each term as the other three write it, read at the named commit:

| Nix-TOPS | Rust-TOPS 1.2.0 (`77056c9`) | Go-TOPS 0.3.0 (`8d5b416`) | Web-TOPS 0.2.0 (`7767ae0`) |
|---|---|---|---|
| `pass` | `Allow`, the Bend verdict (§3.1) | `pass` | `pass` |
| `Refuse` | `Refuse`, fails the run | `Refuse` | `Refuse` |
| `Ack` | `Ack`, recorded and does not fail | `Ack`, fails if unacknowledged | `Ack`, fails if unacknowledged |
| `skip` | none (a layer is on or off in config) | `skip` | `skip` |
| `not-run` | none | `not-run`, fails | `not-run`, fails |
| `not-evaluated` | `not-evaluated`, never a pass | `not-evaluated`, never a pass | `not-evaluated`, never a pass; `not_evaluated` counted in the report |
| fails when required | not stated | not stated | not stated |
| `deferred` | none | none | none (new here) |
| `vacuous` | none | none | `Vacuous`: an empty mutation diff, not a pass |
| missing merge base | none | none | `Failed` |

Differences kept: Rust-TOPS records an `Ack` without failing it; the other three fail an
unacknowledged one. Rust-TOPS has no `skip` or `not-run`. The rule that a required law which cannot be judged fails is adopted by each protocol in its next revision. Web-TOPS 0.2.1 (`e219e49`) widens `not-run`
(also when the run stopped early or an input is missing), `skip` (config needs a `reason`) and
`not-evaluated` (also when the tool's rule is off or below `error`); each Nix term maps to the 0.2.1
text the same way.

### 3.4 Stated rules

1. **Oracle first.** Write the oracle from the contract before the code. A test that asserts what
   a module currently produces freezes its bugs.
2. **Pure or nothing.** The gate never passes `--impure`. A change that needs `--impure` to
   evaluate is not done.
3. **Do not weaken a law**, a linter setting, an eval option or a budget so a check passes. Fix
   the code, or change the law through the protocol-change process (§13).
4. **Schema precedence.** When prose and schema disagree, fix the schema first, then the prose.
5. **No silent empties.** A flake with no checks, an eval that enumerated no outputs, or a report
   with no layers fails the gate.
6. **No dry gates in CI.** `--dry-run` is refused when `CI=true`.
7. **Never fabricate a hash.** Use `lib.fakeHash`, build, and copy the `got:` value. A placeholder
   is never committed (N-25).
8. **One gate.** `nixtops gate` is the verification command. Running `nix flake check` alone is
   not done.

---

## 4. Quick start

The adopter's `flake.nix` (also `examples/quickstart/flake.nix`, which `verify.sh` there compares with this block):

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nix-tops.url = "github:Hedronite/nix-tops";
    # Do not set nix-tops.inputs.nixpkgs.follows: the kit's tools stay on the kit's pin (N-43).
  };

  outputs =
    { self, nixpkgs, nix-tops }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];
      perSystem = f: nixpkgs.lib.genAttrs systems f;
      forAllSystems = f: perSystem (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: { default = pkgs.callPackage ./nix/package.nix { }; });

      # mkChecks returns checks.<system>.nixtops-* from ./nix-tops.toml (no IFD: builtins.fromTOML).
      # Merge it with your own checks per system.
      checks = nixpkgs.lib.recursiveUpdate (nix-tops.lib.mkChecks { inherit self systems; }) (
        perSystem (system: { package = self.packages.${system}.default; })
      );

      formatter = perSystem (system: nix-tops.formatter.${system});

      # Re-export the runner so one locked command runs the gate: nix run .#nixtops -- gate
      apps = perSystem (system: { nixtops = nix-tops.apps.${system}.nixtops; });
    };
}
```

```bash
nix run .#nixtops -- init --class dev-flake
nix run .#nixtops -- check
nix run .#nixtops -- gate
```

`.#nixtops` is the adopter's re-export of the kit's runner, so the command resolves the kit through
the adopter's own `flake.lock`. A `flake` `formatter` output is an attrset by system, so it is
re-exported per system. `checks` merges `mkChecks` with the adopter's own checks per system.
`x86_64-darwin` is not in `systems` (§2). `examples/quickstart/` holds this flake and a `stub-kit/`
that exports the interface as stubs; `nix flake check --no-build --all-systems` passes on both
CppNix 2.35.2 and Determinate Nix 3.22.3, and the adopter's own check survives the merge.

`init` writes:

- `nix-tops.toml` (§9);
- the class CI workflow (§11);
- the Nix-TOPS section of `AGENTS.md`. If `AGENTS.md` exists, `init` replaces only the text
  between `<!-- nix-tops:begin -->` and `<!-- nix-tops:end -->`.

`init` writes `enabled = false` for layers the class does not map. It never writes a baseline;
only `nixtops baseline` does (§6.8). It refuses to overwrite without `--force`. It never edits
`flake.nix`; it prints the lines to add.

---

## 5. Facets and profiles

### 5.1 Facet catalog

**Facet replaces class.** Earlier sections and some files still say "class" (`class`, `--class`,
"per class"): read it as "facet", with the rules of this section; each section is renamed by the
step that owns it (0.2.0).

A flake is judged as the facets its outputs make it. A facet is derived from the artifact it names;
`nix-tops.toml` may narrow the set (§5.4) and never add a facet the outputs do not show.

| Facet | Derived when the flake exports | Typical code | Profile |
|---|---|---|---|
| `dev-flake` | `devShells.<system>` | The flake inside a Rust, Go or web repo: its dev shell and checks | **Launch** |
| `package-set` | `packages.<system>` or `overlays` for others to use | Packages and overlays published for others | **Launch** |
| `nixos-module` | `nixosModules.*`, `homeManagerModules.*` or `darwinModules.*` | Reusable modules | **Launch** |
| `flake-lib` | `lib` | A pure Nix library, and the kit's own `lib.mkChecks` | **Launch** |
| `nixos-host` | `nixosConfigurations.*` | Real machines | Specified, unproven (no adopter in the house) |
| `home-config` | `homeConfigurations.*`, or Home Manager as a NixOS or nix-darwin module | Home Manager setups | Specified, unproven (no adopter in the house) |
| `darwin-host`, `fleet-root`, `image` | `darwinConfigurations.*`; colmena or deploy-rs outputs; `dockerTools` or image-builder outputs | | No profile |

A flake that exports `packages` and `nixosModules` has two facets, and nothing to split.
"Specified, unproven" means the floors and laws are written and no adopter has run them: the
profile ships when an adopter does, and `nixtops check` says so.

### 5.2 Floors by facet

A facet sets floors only: coverage floors, budgets, and the layer tiers the table sets (rebuild determinism, `vulnix`). It never turns a law on or off; a law applies
when the artifact it names exists (§8.2, §8.3). Floors apply to the outputs the facet is derived
from (`devShells` to `dev-flake`, `packages` to `package-set`, and so on); an output of no facet has
no floor and is reported as such. There is one floor per facet and row.

| | `dev-flake` | `package-set` | `nixos-module` | `flake-lib` | `nixos-host` (unproven) | `home-config` (unproven) |
|---|---|---|---|---|---|---|
| Eval on every declared system | required | required | `x86_64-linux`, `aarch64-linux` | required | the host's system | the config's system |
| Output coverage: built | 100% | 100% | n/a | 100% of non-function outputs | 100% (build root) | 100% (activation package) |
| Output coverage: tested | 50% | 80% | **100%** of modules eval-tested; every service module VM-tested | 80% of exported functions have a `nix-unit` case | every declared service VM-tested | n/a |
| Closure budget | per `devShell`, ratchet | per package, ratchet | n/a | n/a | per host `toplevel`, ratchet | `activationPackage`, ratchet |
| Eval-cost budget | advisory (0.x) | advisory (0.x) | advisory (0.x) | advisory (0.x) | advisory (0.x) | advisory (0.x) |
| Rebuild determinism | no | tier 2 | no | no | tier 2 (`toplevel`) | no |
| `vulnix` | no | tier 2 | no | no | tier 2 | tier 2 |

All numbers are **proposed**, to be calibrated in M4.6. Under `ratchet` (§6.8) floors and budgets
apply to new outputs, and existing outputs must not get worse. `vulnix` has one tier per facet: this
table is its definition, and a profile's `[layers]` block (§5.3) repeats it.

### 5.3 Profile file shape

```toml
# profiles/package-set.toml
facet = "package-set"
protocol = "0.1.0"

[eval]
all_systems = true
abort_on_warn = true
allow_ifd = false

[coverage]
built_floor = 100
tested_floor = 80

[budgets.closure]
mode = "ratchet"

[budgets.eval_cost]
mode = "ratchet"

[layers]
rebuild = { tier = 2 }
vulnix = { tier = 2 }
```

There is no `[laws]` list: a law applies by artifact, not by profile, and the set of laws a facet
requires (§3.3) is derived from the artifacts present.

### 5.4 Narrowing

`nix-tops.toml` may list `facets = [...]`. The list must be a subset of the derived facets;
`nixtops check` refuses a facet the outputs do not show. Narrowing drops the dropped facet's floors,
and the report lists the outputs left without a floor. It never removes a law.

### 5.5 The kit's own facets

The kit exports `lib` (`mkChecks`), `packages` (`nixtops`, `nixtops-lint`), `apps`, `formatter` and a
dev shell, so it is `flake-lib`, `package-set` and `dev-flake`, and runs `hard` (§6.8) on all three.

---

## 6. Scoring and reporting

### 6.1 Output coverage

Nix has no line coverage in the evaluator: evaluation is lazy, so a line that was never forced proves
nothing. Coverage tools exist and measure other things (below). Nix-TOPS scores what Nix can answer
exactly: **which exported outputs are exercised by a check, and by what kind of evidence.**

The runner enumerates the flake's outputs and classifies each one at three levels:

| Level | Meaning | How it is decided |
|---|---|---|
| `evaluated` | The output evaluates on its systems (gate 3) | Every output reaches this or the gate already failed |
| `built` | The output's derivation was built in gate 4 | The runner builds it |
| `tested` | A check exercises the output, and the report names the evidence kind | See the evidence kinds below |

**Evidence kinds for `tested`.** The report prints the kind beside the level.

| Output | Evidence kind | How it is read |
|---|---|---|
| package | **in-build** | The package's `.drv` runs a suite in the build: `doCheck` or `doInstallCheck` is true in `nix derivation show` |
| package | **dependent check** | The package's `.drv` is in the requisites of a `checks.*` derivation that is not the package (`testers.testVersion`, a VM test) |
| package | **`passthru.tests`** | The package exports `passthru.tests` and a check builds them |
| module | **non-default definition** | Some check's configuration defines at least one option declared in the module's file at a non-default priority. Importing a module, or declaring an option no check sets, is not `tested` |
| `lib` function | **unit test** | A nix-unit test applies the function by name |
| configuration | **VM test** | A VM test imports the configuration's modules |

In-build evidence counts. `cargo-tops` runs its suite in the build, with `doCheck` true in its derivation
(`rust-tops` at `77056c9`, CppNix 2.35.2 and Determinate Nix 3.22.3), so it scores `tested` (in-build);
the dependent-check kind alone scored it 0 of 2 on `aarch64-darwin`, because `checks.cargo-tops` is
the package itself. For modules, the options reader (`options.<name>.isDefined` and `highestPrio` on
the check's configuration) is **[verify]**: reachability from the check derivation is §16 open 3.

**Outputs that cannot be built.** Overlays, templates, apps, `lib` and module exports have no
derivation to build. They get an `instantiated` level in place of `built`:

| Output | `instantiated` means |
|---|---|
| overlay | Applied to the pinned nixpkgs, and the result evaluates |
| template | Copied to a scratch directory, and its flake evaluates |
| app | `program` resolves to a file in the built output |
| `lib` function | Applies without error to an argument the test supplies |
| module export | Evaluates as a module in a minimal configuration |

`tested` on these outputs needs a check that uses them (a nix-unit test, a configuration that imports
the module).

`tested_floor` is the percentage of outputs at `tested`, set per facet (§5.2). The report lists every
output with its level and evidence kind, so an untested output is visible, not averaged away.

**Coverage tools that exist.** None is part of `tested_floor`; each measures something else.

| Tool | Measures | Status here |
|---|---|---|
| `nixcov` | Line and expression coverage by syntax-tree instrumentation, LCOV output | Cited, not depended on (one-person project) |
| nix-unit `lib.coverage.addCoverage` | Interface coverage of a tested `lib` | Cited **[doc]** |
| `NIX_COUNT_CALLS=1` | Per-function call counts with file, line and column, in `functions[]` of the stats JSON | Call coverage for `lib`: advisory. Run on `nix eval` only (1,622 entries on CppNix 2.35.2, 1,628 on Determinate Nix 3.22.3; `docs/evidence/2026-11-16-gate3-trial/counters/*/se.json`); under nix-unit **[verify]** |

Option coverage (the share of a module's declared options that some check defines at non-default
priority) and call coverage ship advisory in 0.x, listed in the report and in no floor. §16 open 3
stays open for M4.5.

Output coverage is reachability, not behavior. A test that only runs `--version` covers the
package. That is a floor, not a proof; review still decides whether the test is meaningful.

### 6.2 No CRAP

The other TOPS protocols gate on CRAP, which combines cyclomatic complexity with per-function
line coverage. Nix has no per-function coverage, and module code is mostly attribute sets, not
branches. A complexity number for Nix would be a number nobody has validated, gating merges.
Nix-TOPS does not ship one. The density and speed concerns go into two budgets that Nix measures
exactly.

### 6.3 Closure budget

- **Measured:** NAR size of the runtime closure of each budgeted output, in bytes. Computed in the
  sandbox from `pkgs.closureInfo { rootPaths = [ drv ]; }` (`total-nar-size`), so it needs no
  network and is cached.
- **Budgeted outputs** per facet (§5.2): `devShells.*` for `dev-flake`, `packages.*` for
  `package-set`, `toplevel` for hosts, `activationPackage` for Home Manager.
- **Why it gates:** closure bytes are the same on every run for the same lock. A change that pulls
  a second Python or a full GHC into a closure is visible in one number.
- **Tolerance (proposed):** an output is over its baseline when its closure grows by more than 1%
  or 5 MiB, whichever is larger.
- **A lock update is reported and re-baselined in the same PR.** A PR that changes `flake.lock`
  reports the closure delta per output and runs `nixtops baseline` in the same PR (§6.8).
- **Names before bytes.** The report lists new store-path names and duplicate packages (the same
  name at two versions or hashes in one closure) ahead of the byte total.
- **Own derivations** point at `outputChecks` (`maxClosureSize`, `allowedRequisites`) **[verify the
  attribute names with structured attributes on the pinned Nix]**, so the build itself refuses an
  oversized closure.
- The report names the largest new paths when a budget is exceeded (the `nix-tree` view, as text).

### 6.4 Eval-cost budget

- **Measured:** `nrFunctionCalls` and `nrThunks` from `NIX_SHOW_STATS` (`NIX_SHOW_STATS_PATH`
  writes JSON; checked), **always with `--no-eval-cache`**: the eval cache changes the counters on
  Determinate Nix (a warm run had 7.6% fewer calls and 18.1% fewer thunks). For a minimal NixOS host
  on 26.11, evaluating `toplevel.drvPath` costs about 4.6 million function calls and 6.9 million thunks.
- **Why counters and not seconds:** counters do not depend on machine load. They do depend on the
  implementation, its version, the system and the lock, so the baseline is keyed by (implementation,
  version, system, `flake.lock` hash) as in §2. A change of any of the four rewrites the baseline
  through `nixtops baseline`, as a reviewed change.
- **Normalized (proposed, not measured):** the report shows each output's counters as a ratio to a
  reference output evaluated on the same lock, so the number survives a lock update. The reference
  is chosen and calibrated in M4.
- **Advisory in every profile for 0.x:** whatever §5.2 and §6.8 say for other budgets, eval cost
  does not fail a run in 0.x.
- **Determinism:** on one implementation the counters are identical across runs with the same
  inputs (three runs each on CppNix 2.35.2 and Determinate Nix 3.22.3, 2026-11-17). They differ
  between the two implementations (53 calls on one devShell), with the eval cache on Determinate
  Nix, and by system (40% fewer calls for `x86_64-linux` than `aarch64-darwin` on the same devShell)
  (`docs/evidence/2026-11-16-gate3-trial/`). §16 open 2 is closed on this.

### 6.5 Mutation: not in 0.x

There is no mutation-testing tool for Nix; the prior art is a pair of per-repo shell scripts
**[doc]**. Nix-TOPS 0.x has no mutation layer, and it does not appear in config or the report.

The design on record for the M6 spike is **derivation triage**. A mutant of a Nix file changes
derivation hashes or it does not, so most mutants are judged without running a test:

1. No output's `drvPath` changes: the mutated code is dead, or the mutant is equivalent. Cost: one evaluation.
2. Some output changes and no check's `drvPath` changes: no check can see the change, so the mutant survives. No build ran.
3. A check's `drvPath` changes: build only that check. It fails (the mutant is killed) or passes (it survives).

Two of the three outcomes cost an evaluation. The spike runs on one module of a flake with the
`nixos-module` facet and reports counts: dead, survived, killed, and builds spent. Equivalent-mutant
noise is unknown. This is a plan for a spike, not a commitment, and not a layer in 0.x.

### 6.6 Law verdicts

`Refuse` fails, `Ack` needs an acknowledgement, and `not-evaluated` is never a pass: on a required
law it fails under `hard` and `ratchet` (§3.3). The report names each law's reader.

### 6.7 Report

`nixtops gate` always writes `.nix-tops/report.json` and prints text. The schema is
`schema/gate-report.schema.json`. Every layer is present; `skip` carries a reason; `not-run`
fails; `not_evaluated`, `deferred` and `vacuous` are each counted on their own, and a required law
that is `not_evaluated` also counts as failed. An output re-evaluated with IFD on (N-07) is marked as
such. Eval cost is the uncached, normalized figure of §6.4.

### 6.8 Enforcement modes

The top-level key `enforce` sets how floors and budgets apply. Adopters default to `ratchet`.
Nix-TOPS runs `hard` on itself.

<!-- tops-core:begin -->
| Mode | What fails |
|---|---|
| `hard` | Any finding; any output below the coverage floor; any budget overrun; a required law `not-evaluated`; in CI, any `deferred` output that no leg built |
| `ratchet` | A new output below the floor or over budget; an existing output whose closure bytes rise above baseline plus tolerance (§6.3); a per-linter, per-file finding count that rises; an ack count that rises; a law lost from the judged set; a required law `not-evaluated` |
| `advisory` | Nothing; needs a dated exemption, and the report prints the run as not enforced |
<!-- tops-core:end -->

<!-- tops-core:begin -->
Baseline rules:

- `.nix-tops/baseline.json` holds finding counts per linter per file, closure bytes per output, eval
  counters per output (keyed by implementation, version, system and lock hash, §2), coverage level
  per output, the ack count, the judged-law ID set, and the tool configuration it was taken under (a
  hash of the statix, deadnix, formatter and secrets-scan configs and the enabled-law set).
- Only `nixtops baseline` writes it, after a run in which every layer that feeds it passed. It is
  tracked and clean against `HEAD`. Under `ratchet` a missing baseline fails, and deleting it is
  not a reset.
- A narrower tool configuration is refused: a run with fewer lints or laws enabled than the baseline
  was taken under fails. A wider one is allowed and re-baselines.
- An output is identified by its flake attribute path (`packages.x86_64-linux.foo`). A renamed
  output is new. A new file starts at 0 findings; a deleted file's allowance does not move.
- A PR that changes `flake.lock` runs `nixtops baseline` in the same PR (§6.3).
- Legacy findings stay in the baseline. They are not wrapped in `# nixtops-ack` to zero a count.
<!-- tops-core:end -->

---

## 7. Facet layers

| Layer | Tool | What fails | Tier |
|---|---|---|---|
| nix-unit | `nix-unit` over `tests/` or `checks.*.nix-unit` | Any failing case; an exported `lib` function with no case (`flake-lib`) | 1 |
| Module eval tests | `lib.nixosSystem` / `lib.evalModules` in a check, asserting on `config` | Any failure | 1 |
| VM tests | `testers.runNixOSTest`; needs KVM on Linux CI **[verify runner KVM]** | Any failure; `sleep` in a test script (N-40) | 1 |
| Disabled-module identity | runner evaluates a minimal host with and without the module imported, `enable = false` | The two `toplevel.drvPath` differ (N-20) | 1 |
| Closure budget | `closureInfo` in a check | Over budget or baseline | 1 |
| Eval-cost budget | runner, `NIX_SHOW_STATS` with `--no-eval-cache` | Advisory in every profile for 0.x: reported against the baseline, never fails (§6.4) | 1 |
| Cross-system builds | `nix build` per declared system (remote builders or CI matrix) | Any failure | 2 |
| Rebuild determinism | `nix build --rebuild` on budgeted outputs | Output differs | 2 |
| Fixed-output re-fetch | runner: `nix build --rebuild` on each fixed-output derivation whose fetch inputs changed against the merge base (network in the runner, not the sandbox) | Hash mismatch (N-26) | 1 |
| Fixed-output re-fetch, all | `nix build --rebuild` on every fixed-output derivation in the flake's outputs | Hash mismatch (N-26) | 2 |
| Vulnerabilities | `vulnix` on runtime closures | Reported as `Ack` per CVE; unacknowledged fails (N-38) | 2 |
| Custom | `[[layers.custom]]` with `id`, `command`, `tier`, `report` | Same rules: missing tool fails, empty report fails, unmapped is `not-run` | declared |

**An enabled layer with no command is `not-run` and fails the gate**, and a mapped tool that is
missing fails the gate. Tools come from the kit's flake, so a missing tool is a configuration
error, not an accident of the environment.

---

## 8. The Laws layer

### 8.1 Model

Laws are numbered rules, `N-NN`. Some are delegated to tools the ecosystem already has: the
evaluator itself, statix, deadnix, nixfmt, gitleaks, vulnix. The rest are rules in
`nixtops-lint`, the kit's syntax analyzer, or readers in the runner. There is no formal (Bend or
Lean) layer.

Readers:

- **`ev`** (evaluation): judged by evaluating the flake with the CppNix pinned in the kit's lock (§2) and the gate's options.
  This reader is new in Nix-TOPS. It is the strongest one, because the module system and the
  evaluator do the judging.
- **`tl`** (tool): mapped from that tool's JSON output. A `tl` reader reads the tool's effective
  configuration first (`statix.toml`, treefmt config). If the lint is disabled, the law is
  `not-evaluated`, not `pass`.
- **`an`** (analyzer): a `nixtops-lint` rule over the syntax tree, or a runner reader over git,
  `flake.lock` or the diff.
- **`at`** (attested): judged by review; `not-evaluated` until attested.
- **`tbd`**: not yet implemented; `not-evaluated`.

**Reader preference.** A law is judged from the most structured artifact Nix already produced,
in this order: the lock (`flake.lock` JSON), the derivation (`nix derivation show` JSON), the options
tree (`options` and `config`), then syntax. A law gets an `an` syntax reader only when no evaluated
artifact can answer it. Each row of §8.2 lists its readers in this order, strongest first, as
`kind` or `kind: artifact`. Syntax readers stay for N-01 to N-03 and N-10: a derivation or options
reader sees only what an output reaches, and a broken file no output imports is invisible to it.
The runner readers over git and over the diff (`an: git`, `an: diff`) rank with the lock: structured
data the runner already holds, with no Nix parsing. The `meta` reader (`ev: meta`) reads evaluated
attributes and ranks with the options tree.

Every delegated rule is proven to fire. `fixtures/laws/` breaks each enabled law on purpose, with
a `# expect: N-NN` marker on the line. A test fails if a marked law stays quiet or an unmarked one
fires. This matters: statix does not flag a file-scope `with` or `rec` at all (checked), so those
rules are `nixtops-lint` rules, not statix mappings.

### 8.2 Law catalog (draft: 49 laws, N-01…N-49)

Severity: **R** = Refuse, **A** = Ack. ✓ = judged in v0.1. The **Sketch** column names the
mistake in the source sketch the law comes from (§1–§10), where there is one. The **Applies to**
column names the artifact that triggers the law; a law whose artifact is absent is `skip`, with the
artifact named (§3.3). Artifacts: `flake` (always), `source` (any `.nix` file), `lock` (`flake.lock`),
`output` (any evaluated output), `module` (an exported module), `configuration` (a NixOS, Home
Manager or nix-darwin configuration), `package` (an exported package), `derivation` (an own
`mkDerivation` or builder call), `fod` (an own fixed-output derivation in an output's closure),
`vm-test` (a NixOS test in `checks`), `budgeted` (an output with a budget), `nixpkgs-config` (an
`import nixpkgs` config or `nixpkgs.config`), `nix-tops-input` (the `nix-tops` input in the lock).

**Purity and flakes**

| ID | Law | Sev | Reader | Sketch | Applies to | v0.1 |
|---|---|---|---|---|---|---|
| N-01 | No search-path lookups (`<nixpkgs>`, `NIX_PATH`) in flake-evaluated code. Pure eval refuses them (checked); `nixtops-lint` also catches them in files the flake does not evaluate. A `flake-compat` shim is the one exemption | R | ev, an: syntax | §2 | source | ✓ |
| N-02 | No impure builtins: `builtins.getEnv`, `currentSystem`, `currentTime`, `getFlake` on an unlocked ref, `fetchTarball`/`fetchurl`/`fetchGit` without a hash. In pure eval `getEnv` returns `""` instead of failing (checked), so only the analyzer catches it | R | ev, an: syntax | §1 | source | ✓ |
| N-03 | No host paths (`/home/…`, `/Users/…`, `/root/…`) as path literals or in strings. Runtime paths (`/run`, `/var`, `/etc`, `/dev`, `/proc`, `/sys`, `/tmp`) are allowed in strings | R | an: syntax | §1 | source | ✓ |
| N-04 | No untracked, non-ignored `.nix` file under the flake root, and in CI the working tree equals `HEAD` (a file that is staged and not committed counts as untracked). A flake reads tracked files only: an untracked file that is imported fails with "path does not exist" on the machine that has it, and a staged but uncommitted file passes there and fails in CI. Ignored directories such as `.direnv` are exempt | R | an: git | §1 | flake | ✓ |
| N-05 | `flake.lock` is committed and current; evaluation with `--no-update-lock-file` succeeds | R | ev | | lock | ✓ |
| N-06 | One nixpkgs per lock: other inputs `follows` the main nixpkgs. Exempt: the `nix-tops` input (N-43), and inputs with an ack (for example a second nixpkgs for a platform the main pin dropped) | A | an: lock | | lock | |
| N-07 | No import-from-derivation by default: the gate evaluates with `allow-import-from-derivation = false`. IFD is ackable per output: the inventory records which outputs fail only with IFD off, an acked output is re-evaluated with `allow-import-from-derivation = true` and the report says so, and an unacked IFD output fails | R | ev | | output | ✓ |
| N-08 | Every output evaluates on every system it declares (`--all-systems`). No hardcoded `"x86_64-linux"` inside a per-system function | R | ev, an: syntax | §2 | output | ✓ |
| N-09 | `pkgs` is instantiated once per system from the flake input (`legacyPackages.${system}`, or one `import nixpkgs { … }`). Modules take `pkgs` as an argument and never `import nixpkgs` | A | an: syntax | §2 | source | |
| N-44 | Every input has an explicit URL; no registry-indirect inputs (`"type": "indirect"` in `flake.lock`) | R | an: lock | | lock | |

**Language**

| ID | Law | Sev | Reader | Sketch | Applies to | v0.1 |
|---|---|---|---|---|---|---|
| N-10 | No file-scope `with` (a module or file body wrapped in `with pkgs;` or `with lib;`). `with pkgs;` around a list is allowed | R | an: syntax | §3 | source | ✓ |
| N-11 | No `rec` attrset passed to `mkDerivation` or a builder that accepts `finalAttrs:` in the pinned nixpkgs **[verify which builders]**; use `finalAttrs`. `rec` elsewhere is an Ack | R | an: syntax | §3 | derivation | ✓ |
| N-12 | No dead bindings, lambda arguments or `let` entries | R | tl: `deadnix --fail` | | source | ✓ |
| N-13 | statix is clean | R | tl: `statix check` | | source | ✓ |
| N-14 | Source is formatted with nixfmt (RFC 166 style) | R | tl: `treefmt --ci` with `nixfmt` | | source | ✓ |
| N-15 | Every `lib.<name>` and `builtins.<name>` reference resolves in the locked inputs and exists at the Nix floor the config declares (`[toolchain].nix_floor`), not only in the Nix the gate runs; a builtin newer than the floor passes the gate and breaks for users at the floor. The analyzer collects each selection path and checks it with `lib.hasAttrByPath` against the pinned `lib` and `builtins`; an extended `lib` (`lib.hm`, flake-parts, `lib.extend`) is resolved against the extended set | R | ev, an: syntax | §4 | source | ✓ |
| N-16 | No `builtins.trace`, `lib.traceVal` or other `lib.debug` tracing committed outside tests | A | an: syntax | | source | |
| N-17 | Evaluation emits **no unmatched warnings** from any of three sources: `builtins.warn` (renamed NixOS options, `lib.warn`, deprecated aliases; `abort-on-warn = true` makes each an error, checked), `config.warnings` read as data (Home Manager prints it with `builtins.trace`, so `abort-on-warn` does not see it, checked; nix-darwin does the same in source at `ff0e21a`, **[verify]**), and `nix` command diagnostics classified by `laws/diagnostics.toml`. A warning is accepted only by a dated `[[warnings.allow]]` entry | R | ev | §5 | output | ✓ |

**Modules and options**

| ID | Law | Sev | Reader | Sketch | Applies to | v0.1 |
|---|---|---|---|---|---|---|
| N-18 | Every configuration evaluates to its build root (`config.system.build.toplevel.drvPath`, `activationPackage.drvPath`). The module system refuses options that do not exist, such as `virtualHosts.<name>.enableSSL` (checked), and missing package attributes | R | ev | §5 | configuration | ✓ |
| N-19 | Every declared option has `type` and `description`. `types.anything`, `types.attrs` and `types.unspecified` need an ack | A | ev: options tree | | module | |
| N-20 | A module that is imported but not enabled changes nothing: the `toplevel.drvPath` of a minimal host with the module imported and `enable = false` equals the one without it | R | ev: derivation | | module | ✓ |
| N-21 | `lib.mkForce`, and `lib.mkOverride` with a priority below 100, carry an ack naming the definition they override | A | ev: options tree **[verify]**, an: syntax | | module, configuration | |
| N-22 | `system.stateVersion` and `home.stateVersion` do not change in a diff without an ack. They record the state format the machine was installed with. They are not the release version | R | an: diff | | configuration | ✓ |
| N-23 | Unfree packages are allowed by `allowUnfreePredicate` over a named list, not by a blanket `allowUnfree = true` | A | ev: options tree **[verify]**, an: syntax | | nixpkgs-config | |
| N-24 | Each `permittedInsecurePackages` entry carries an ack with an expiry date | A | ev: options tree **[verify]**, an: syntax | | nixpkgs-config | |

**Derivations and packaging**

| ID | Law | Sev | Reader | Sketch | Applies to | v0.1 |
|---|---|---|---|---|---|---|
| N-25 | Fixed-output hashes are SRI: `sha256-` with 44 base64 characters, or `sha512-` with 88. `lib.fakeHash` (the all-zero `sha256-` value), `lib.fakeSha256`, `""` and other placeholders are refused, although `lib.fakeHash` has the shape of a valid sha256 hash. A valid `sha512-` hash is accepted | R | ev: derivation, an: syntax | §6 | fod | ✓ |
| N-26 | A fixed-output derivation whose fetch inputs (`rev`, `tag`, `url`, or a `version` the source depends on) changed between the merge base and the head must also change its hash, and `vendorHash`/`cargoHash`/`npmDepsHash` when the dependency set moved. A fetcher's default store name is `source`, so a stale hash still matches the old path already in the store, and the build silently uses the old source. The exact reader compares fetch inputs, not `drvPath` (a lock update changes every fetcher's derivation), and is primary; the diff heuristic is the fallback. Tier 1 re-fetches the changed fixed-output derivations; tier 2 re-fetches all | R | ev: derivation (base against head); an: diff; tl at tier 2 | §6 | fod | ✓ |
| N-27 | No network in build phases: a raw `mkDerivation` does not run `go build`/`go mod download`, `cargo build`/`cargo fetch`, `npm install`/`npm ci`, `pip install`, `curl`, `wget` or `git clone`. Use the language builders (`buildGoModule`, `rustPlatform.buildRustPackage`, `buildNpmPackage`, `buildPythonApplication`) | R | an; build with `sandbox = true` | §7 | derivation | ✓ |
| N-28 | `.override` takes package-function arguments; `.overrideAttrs` takes derivation attributes. `.override` with `src`, `version`, `patches`, a `*Phase`, `buildInputs` or `nativeBuildInputs` is refused: it errors, or is silently ignored when the function takes `...` | R | an: syntax | §8 | source | ✓ |
| N-29 | Store paths keep their string context: no `builtins.unsafeDiscardStringContext`, `builtins.storePath`, `/nix/store/<hash>-…` literals, or string surgery on store paths. Use interpolation, `lib.getExe` or `lib.getExe'` | R | an: syntax | §9 | source | ✓ |
| N-30 | Exported packages set `meta.description` and `meta.license`, and `meta.mainProgram` when they ship one primary binary (`lib.getExe` warns without it, which N-17 then refuses). `meta.platforms` is advised, not required | A | ev: meta | | package | |
| N-31 | A package's `src` is filtered (`lib.fileset.toSource`, at least `lib.cleanSource`), not a bare `./.` or `${./.}`, so unrelated edits do not rebuild it and `.git` or `result` never enter the store | A | ev: derivation **[verify]**, an: syntax | | package | |
| N-32 | Scripts use `writeShellApplication` (shellcheck, `runtimeInputs`, strict mode), not `writeShellScriptBin` with bare command names | A | an: syntax | §9 | source | |
| N-45 | Fetcher refs are immutable: a tag or a 40-hex commit, never a branch; no `leaveDotGit = true` | R | ev: derivation, an: syntax | | fod | |
| N-46 | `doCheck = false`, `dontCheck`, an emptied check phase, or a removed check carries an ack | A | ev: derivation, an: diff | | derivation | |
| N-47 | Prebuilt binaries declare `meta.sourceProvenance` | A | ev: meta | | package | |

**Secrets and security**

| ID | Law | Sev | Reader | Sketch | Applies to | v0.1 |
|---|---|---|---|---|---|---|
| N-33 | No secrets in source, fixtures or strings bound for the store | R | tl: `gitleaks` | §10 | flake | ✓ |
| N-34 | Options that take a secret file (`passwordFile`, `environmentFile`, `keyFile`, `tokenFile`, `credentialsFile`, `hashedPasswordFile`, and the like) get a runtime path string: `/run/secrets/…`, `config.sops.secrets.<name>.path`, `config.age.secrets.<name>.path`. Never a Nix path literal (`./secret.env` copies the file into the store) and never `pkgs.writeText` | R | ev: options tree (value is a string outside `/nix/store`), an: syntax | §10 | module, configuration | ✓ |
| N-35 | No plaintext `password` or `initialPassword` for users; use `hashedPasswordFile`. A `hashedPassword` literal is an Ack: the hash is world-readable in the store | R | ev: options tree | §10 | configuration | ✓ |
| N-36 | No sandbox escapes: `__noChroot`, `__impure`, `nix.settings.sandbox = false`, `--option sandbox false` | R | ev: derivation **[verify]**, an: syntax | | source | ✓ |
| N-37 | `networking.firewall.enable = false`, and port ranges wider than the services need, carry an ack | A | ev: options tree | | module, configuration | |
| N-38 | Known vulnerabilities in runtime closures are acknowledged or fixed | A | tl: `vulnix`, tier 2 | | package, configuration | |
| N-48 | `nixConfig` in `flake.nix` (substituters, keys) carries an ack; the gate never accepts flake config (`--no-accept-flake-config`) | A | an: syntax | | flake | |

**Tests, budgets and tools**

| ID | Law | Sev | Reader | Sketch | Applies to | v0.1 |
|---|---|---|---|---|---|---|
| N-39 | Oracle-first: tests derive from the contract, not from what the module produces today | A | at | | vm-test, module | |
| N-40 | VM test scripts wait on conditions (`wait_for_unit`, `wait_for_open_port`, `wait_until_succeeds`), never `sleep`, `time.sleep` or `machine.sleep` | R | an: syntax | | vm-test | ✓ |
| N-41 | Each budgeted output's closure stays within its budget or baseline | R | ev (`closureInfo`) | | budgeted | ✓ |
| N-42 | Each budgeted output's eval cost stays within its baseline and tolerance | A | ev (`NIX_SHOW_STATS`) | | budgeted | |
| N-43 | The kit's tools come from the kit's own pin: the `nix-tops` input does not `follows` the adopter's nixpkgs | R | an: lock | | nix-tops-input | ✓ |
| N-49 | No two major versions of the same package in one budgeted closure without an ack | A | ev (`closureInfo`) | | budgeted | |

**v0.1 target:** 29 of 49 laws judged. Fifteen of them use the `ev` reader, alone or with an
analyzer; for those the evaluator does most of the work. A law whose artifact is absent is `skip`, with
the artifact named (§3.3). N-44 to N-49 are not judged in v0.1.

### 8.3 Law metadata

Each law lives in `laws/N-NN.toml`. `artifacts` is the "Applies to" cell of §8.2, as a list; a law
applies to a flake only when one of those artifacts exists, and is `skip` otherwise. The example is `laws/N-34.toml`; every law file validates against
`schema/law.schema.json`, which requires `id`, `title`, `law`, `severity`, `artifacts`, `reader`, `judged`,
`rationale` and `fix`.

```toml
id = "N-34"
title = "Secret files are runtime paths, never store paths"
law = "Options that take a secret file (`passwordFile`, `environmentFile`, `keyFile`, `tokenFile`, `credentialsFile`, `hashedPasswordFile`, and the like) get a runtime path string: `/run/secrets/…`, `config.sops.secrets.<name>.path`, `config.age.secrets.<name>.path`. Never a Nix path literal (`./secret.env` copies the file into the store) and never `pkgs.writeText`"
severity = "Refuse"
artifacts = ["module", "configuration"]
reader = "ev"
reader_also = ["an"]
reader_detail = "ev: options tree (value is a string outside `/nix/store`), an: syntax"
judged = true
sketch = "§10"
option_names = ["passwordFile", "environmentFile", "keyFile", "tokenFile", "credentialsFile", "hashedPasswordFile"]
rationale = "A Nix path literal or writeText copies the secret into /nix/store, which every local user can read and which binary caches may copy."
fix = "Use config.sops.secrets.<name>.path, config.age.secrets.<name>.path, or a /run/secrets string."
```

---

## 9. Configuration: `nix-tops.toml`

The config is TOML, not YAML as in the other TOPS protocols, because Nix reads TOML natively
(`builtins.fromTOML`). The sandbox checks and the runner read the same file, without IFD and
without a YAML parser in the evaluator. It is not a flake output: `nix flake check` warns about
unknown outputs, and the gate refuses warnings.

```toml
protocol = "0.1.0"
version = "0.1.0"
flake = "github:Hedronite/example"
class = "dev-flake"
enforce = "ratchet"                  # hard | ratchet | advisory (§6.8)
baseline = ".nix-tops/baseline.json"

[toolchain]
nix_floor = "2.24"
systems = ["x86_64-linux", "aarch64-linux", "aarch64-darwin"]

[eval]
all_systems = true
abort_on_warn = true
allow_ifd = false

[lint]
statix = { config = "statix.toml" }
deadnix = { fail = true }
formatter = "nixfmt-tree"
secrets = { tool = "gitleaks" }

[coverage]
built_floor = 100
tested_floor = 50

[budgets.closure]
mode = "ratchet"
outputs = ["devShells.*.default"]

[budgets.eval_cost]
mode = "advisory"
tolerance_pct = 2

[layers]
vm_tests = { enabled = false }
rebuild = { enabled = false }
vulnix = { enabled = false }
custom = []

[laws]
enable = "default"
ack_ledger = ".nix-tops/acks.toml"

[report]
path = ".nix-tops/report.json"
```

---

## 10. Tool lock: the kit's `flake.lock`

The other TOPS protocols keep a separate tool lock (`rust-tops.lock`, `go-tops.lock`,
`web-tops.lock`). Nix-TOPS does not need one. The tools come from the kit's flake, and the
adopter's `flake.lock` pins the kit by revision and NAR hash. That is the tool lock.

One rule keeps it true: the adopter must not set `nix-tops.inputs.nixpkgs.follows`. With
`follows`, the adopter's nixpkgs would pick the versions of statix, deadnix and nixfmt, and the
same kit revision would judge differently in different repos (N-43). Updating the kit is
`nix flake update nix-tops`, a reviewed change.

The same lock pins the evaluator that judges: the CppNix in the kit's lock (§2), not the host's
`nix`. The host supplies the daemon and the store. Two implementations are different judges on the
same input (the `flake check` model, which warnings print, `--build-all`), so a baseline records
which one produced it (§2).

---

## 11. CI floor

The drop-in workflow `.github/workflows/nix-tops.yml`:

| Job | Content |
|---|---|
| `gate` | Install Nix (action pinned by commit SHA); `sandbox = true`; `nix run .#nixtops -- gate --max-tier 1` on `x86_64-linux`. It fails if fewer layers ran than are enabled in range. Gate 3 uses `--all-systems`, so evaluation for every declared system runs here and needs no other runner |
| `determinate` | The same gate, evaluating with Determinate Nix (the named leg of §2), `lazy-trees false` and `eval-cores 1` (§2) |
| `build-matrix` | Tier 2, and where `deferred` outputs build: `ubuntu-24.04` (x86_64), `ubuntu-24.04-arm` (aarch64) and an aarch64-darwin runner, per declared systems. VM tests run on x86_64 only |
| `nightly` | `nixtops gate --max-tier 2` on a schedule |
| `law-diff` | Fails a PR that weakens or removes a law, a lint setting, an eval option or a budget without the `protocol-change` label. It runs the base branch's script, never the PR's |

CI rules: `fetch-depth: 0` (diff laws need the merge base); actions pinned by commit SHA; minimal
`permissions:`; `--dry-run` refused; empty reports fail. Every `nix` command passes
`--no-accept-flake-config`, so a flake cannot change the gate's settings. Workflows never use
`pull_request_target`. No command resolves an unlocked reference. The kit publishes a binary cache
for its tools (owner: open, Q5); an adopter may add its own, and any token comes from the CI secret
store, never from the repo (N-33). The gate is one job running one command. The workflow does not
repeat the tools as separate steps, because steps that bypass `nixtops` judge nothing.

---

## 12. The `nixtops` CLI

| Command | Behavior |
|---|---|
| `init --class <c>` | Writes `nix-tops.toml`, the class workflow and the `AGENTS.md` section (§4). Prints the `flake.nix` lines; never edits `flake.nix`. Refuses to overwrite without `--force` |
| `check` | Validates `nix-tops.toml` against the JSON Schema, plus semantic checks: protocol id, the class has a profile, declared systems are valid, the host daemon meets the floor, `advisory` has a dated exemption, the `nix-tops` input does not follow nixpkgs, and under `ratchet` the baseline exists and is clean |
| `gate` | Runs enabled layers at or below `--max-tier` (default 1). `--base`, `--keep-going`, `--json`, `--only <layer>`, `--system <s>`, `--dry-run` (refused in CI) |
| `baseline` | Writes `.nix-tops/baseline.json` after a passing run; the only writer |
| `laws [list \| run \| explain N-NN]` | Catalog with readers; laws alone; one law explained |
| `outputs` | Lists every output with its coverage level (§6.1) |
| `report [--format text \| json]` | Renders the last report |
| `doctor` | Reports the host: Nix version and implementation, daemon, `sandbox`, `experimental-features`, KVM, remote builders, substituters, and Determinate-only settings. It gates only on the daemon floor (§2); the judge is the kit's pinned CppNix |
| `version` | Protocol, package, and tool versions from the kit's pin |

Exit codes: 0 pass; 1 gate failed; 2 config invalid; 3 environment error (Nix below floor, no
sandbox where required); 4 refused by policy.

---

## 13. Policies and `AGENTS.md`

The template is [`config/AGENTS.template.md`](config/AGENTS.template.md). It turns the source
sketch's §12 drop-in rules into the Go-TOPS template shape: each rule names the check that
enforces it, or `[review]`.

Non-negotiable:

1. Write the oracle from the contract before the code.
2. Never pass `--impure` to make something evaluate.
3. Never weaken a law, lint setting, eval option or budget to make a change pass. Change it through
   a `protocol-change` PR.
4. Never fabricate a hash. Never commit `lib.fakeHash`.
5. Every `# nixtops-ack` names its law and a reason.
6. Agent-authored code meets the same gate as human code.
7. Do not edit generated files (`hardware-configuration.nix`, `flake.lock` by hand); regenerate them.
8. Prefer deleting code to adding configuration.
9. `nixtops gate` is the definition of done.

---

## 14. Deltas

| Area | Go-TOPS 0.3.0 | Web-TOPS 0.2.0 | Nix-TOPS 0.1.0 |
|---|---|---|---|
| Scope | Go | TypeScript, HTML, CSS, Tailwind, Astro | Nix language, flakes, packages, modules, configurations |
| Strongest check | `go vet` + analyzers | type-aware lint | Pure evaluation with `abort-on-warn` and IFD off |
| Coverage | Statement coverage | Line coverage | **Output coverage** (§6.1); Nix has no line coverage |
| CRAP | One cap per class | One cap per class | **None**; closure and eval-cost budgets instead (§6.2–§6.4) |
| Mutation | Pluggable engine | StrykerJS | **None in 0.1**; module mutation proposed for tier 2 (§6.5) |
| Performance gate | `AllocsPerRun`, ns/op at tier 2 | Bytes per page at tier 1 | Closure bytes at tier 1; eval counters at tier 1 |
| Reader kinds | `an`, `tl`, `at`, `tbd` | same | adds **`ev`**: the evaluator judges |
| Execution | One process | One process | Two planes: sandbox checks and an outside runner (§1.4) |
| Config | `go-tops.yaml` | `web-tops.yaml` | `nix-tops.toml`, read natively by Nix |
| Tool lock | `go-tops.lock` | `web-tops.lock` | The kit's `flake.lock`, pinned by the adopter's lock (§10) |
| Unmapped layer | `not-run` fails | `not-run` fails | `not-run` fails |
| Enforcement | `hard` / `ratchet` / `advisory` | same | same |
| Formal layer | removed | none | none |

---

## 15. What you get / what you do not get

**You get:** pure, warning-free evaluation on every declared system; option and attribute names
checked by the module system itself; laws that block the common agent Nix mistakes (impure
lookups, untracked files, invented `lib` names, fake and stale hashes, network in builds, context
loss, secrets in the store); output coverage; closure and eval-cost budgets; a machine-readable
report; an agent contract.

**You do not get (v0.1):**

- Line or branch coverage of Nix code. There is no such tool.
- Mutation testing.
- Proof that a build is network-free on macOS, where the sandbox is relaxed by default.
- Profiles for `flake-lib`, `darwin-host`, `fleet-root` and `image` at launch.
- Lix support.
- Judgement of whether a test is meaningful. Output coverage shows that a check touches an
  output, not that it checks the right thing.

---

## 16. Decisions

**Resolved for 0.1.0:**

1. **Name:** Nix-TOPS / `nixtops` / `Hedronite/nix-tops`.
2. **Coverage:** output coverage (§6.1), at three levels, `tested` floored per class.
3. **CRAP:** none. Budgets instead.
4. **Mutation:** none in 0.1.
5. **Eval options:** the gate always sets `abort-on-warn = true` and `allow-import-from-derivation = false`,
   and never `--impure`. `abort-on-warn` covers `builtins.warn` only; warnings in `config.warnings`
   and diagnostics printed by `nix` itself are read separately (§3.1), and a warning is accepted only by a
   dated `[[warnings.allow]]` entry. IFD stays off by default and is ackable per output (N-07).
6. **Config format:** TOML, for `builtins.fromTOML`.
7. **Tool lock:** the kit's `flake.lock`; the `nix-tops` input never follows the adopter's nixpkgs.
8. **Formatter:** `nixfmt` through `nixfmt-tree`; `nixfmt-rfc-style` is not used.
9. **Enforcement:** `ratchet` for adopters, `hard` for the kit.
10. **Formal layer:** none.
11. **Analyzer and runner language (0.2.0, N2):** one Rust binary from M1, `nixtops`, with
    `nixtops-lint` as its analyzer on `rnix`, gated by Rust-TOPS; the kit's flake by Nix-TOPS.
12. **`nix flake check --build-all` (0.2.0):** absent on CppNix 2.35.2 (`error: unrecognised flag
    '--build-all'`), accepted on Determinate Nix 3.22.3 (both run 2026-11-17). Gate 4 does not use
    it: it builds outputs one by one.
13. **Facets (0.2.0, R4, X2):** facets derived from the outputs replace the single class; config may
    narrow them and never add one; a facet sets floors only; `flake-lib` launches; `nixos-host` and
    `home-config` are specified, unproven. Floors are proposed and calibrated in M4.6. After launch the
    order is `fleet-root`, `darwin-host`, `image`.
14. **Eval-cost determinism (0.2.0):** `NIX_SHOW_STATS` counters are deterministic from run to run
    for the same implementation, version, system and lock. They differ between implementations
    (53 calls on one devShell), with the eval cache on Determinate Nix, and by system
    (`docs/evidence/2026-11-16-gate3-trial/`, run 2026-11-17). Hence the baseline key and
    `--no-eval-cache` (§6.4); `eval_cost` is advisory in 0.x.

**Open:**

1. *Resolved: item 11.*
2. *Resolved: item 14.*
3. **Module coverage reader.** Confirm that option declaration positions in a VM test's node
   configurations are reachable from the check derivation's passthru **[verify]** (§6.1). Option coverage and call
   coverage ship advisory in 0.x; M4.5 answers this with a working reader or closes it as infeasible, with
   evidence.
4. *Resolved: item 12.*
5. **Home Manager pin.** `home-config` evaluates against the adopter's Home Manager input; decide
   whether the kit tests its HM laws against a pinned Home Manager of its own.
6. *Resolved: item 13.*

---

## 17. Repository layout (proposed)

The kit is a flake. It gates itself with its own protocol, and its Rust analyzer with Rust-TOPS.

```
nix-tops/
├── flake.nix · flake.lock          # nixtops, nixtops-lint, lib.mkChecks, lib.formatter
├── nix/
│   ├── checks.nix                  # sandbox plane: lint, nix-unit, closure reader
│   ├── runner.nix                  # outside plane: nixtops gate
│   └── profiles.nix                # profiles/*.toml → check sets
├── crates/nixtops-lint/            # rnix analyzer: N-01, N-02, N-03, N-10, N-11, N-15, N-25, …
├── laws/                           # N-NN.toml
├── profiles/                       # one TOML per launch class
├── schema/                         # nix-tops.schema.json, gate-report.schema.json
├── config/                         # AGENTS.template.md, workflow, statix.toml, treefmt config
├── fixtures/laws/                  # one deliberate violation per law, with expect markers
├── examples/                       # one flake per launch class
├── NIX_TOPS.md
├── AGENTS.md · CLAUDE.md · CHANGELOG.md · README.md
├── nix-tops.toml                   # Nix-TOPS gates itself, hard
└── LICENSE · LICENSE-MIT · LICENSE-APACHE
```

Self-hosting: the kit runs `hard` and passes its own tier-1 gate before any release.

---

## 18. JSON Schema sketch

`nixtops check` converts `nix-tops.toml` to JSON and validates it.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://hedronite.com/schemas/nix-tops/0.1.0/nix-tops.schema.json",
  "title": "Nix-TOPS configuration",
  "type": "object",
  "required": ["protocol", "version", "flake", "class"],
  "additionalProperties": false,
  "properties": {
    "protocol": { "const": "0.1.0" },
    "version":  { "type": "string", "minLength": 1 },
    "flake":    { "type": "string", "minLength": 1 },
    "class": {
      "enum": ["dev-flake", "package-set", "nixos-module", "nixos-host", "home-config",
               "flake-lib", "darwin-host", "fleet-root", "image"]
    },
    "enforce":  { "enum": ["hard", "ratchet", "advisory"], "default": "ratchet" },
    "baseline": { "type": "string", "minLength": 1 },
    "toolchain": {
      "type": "object",
      "properties": {
        "nix_floor": { "type": "string", "pattern": "^2\\.[0-9]+$" },
        "systems": {
          "type": "array", "minItems": 1, "uniqueItems": true,
          "items": { "enum": ["x86_64-linux", "aarch64-linux", "aarch64-darwin", "x86_64-darwin"] }
        }
      }
    },
    "eval": {
      "type": "object",
      "properties": {
        "all_systems":   { "type": "boolean" },
        "abort_on_warn": { "const": true },
        "allow_ifd":     { "const": false }
      }
    },
    "coverage": {
      "type": "object",
      "properties": {
        "built_floor":  { "type": "number", "minimum": 0, "maximum": 100 },
        "tested_floor": { "type": "number", "minimum": 0, "maximum": 100 }
      }
    }
  }
}
```

`abort_on_warn` and `allow_ifd` are constants: they exist in the file so a reader sees them, and a
config that tries to change them is invalid. `abort_on_warn` is the first of three warning sources
(§3.1); it does not make every warning fatal. The `[[warnings.allow]]` table (`match`, `reason`,
`until`) is part of the 0.2.0 schema and lands in the schema files with G8; this sketch is abbreviated
and does not list it. (Abbreviated; the full schema covers every key in §9.)

---

## 19. Roadmap

| Milestone | Contents | Exit criterion |
|---|---|---|
| **M0** | This spec, schemas, five profiles, `AGENTS.template.md` | Reviewed; budgets marked for calibration |
| **M1** | Runner: `init`, `check`, `gate` for gates 1 and 3 (source, eval) and the report | The kit's own flake gates green; a flake with a renamed option fails |
| **M2** | Sandbox plane: `lib.mkChecks` with nixfmt, statix, deadnix, gitleaks; gate 4; baseline and ratchet | A deliberate deadnix finding fails under `hard` and passes as baseline under `ratchet` |
| **M3** | Laws v0.1: `nixtops-lint` and readers, 29 judged; `fixtures/laws` | Report shows "29 of 49 judged" |
| **M4** | Output coverage; closure and eval-cost budgets; VM-test layer | A `nixos-module` example fails when its VM test is removed |
| **M5** | Tier 2: rebuild determinism, fixed-output re-fetch, `vulnix`, build matrix; module-mutation spike | Nightly green on the examples |
| **M6** | Protocol 1.0 | Calibrated floors and budgets on three Hedronite flakes; frozen law IDs |

---

## 20. First adopter: the Rust-TOPS flake

The Rust-TOPS repo has a `flake.nix` with packages, an overlay, apps, a dev shell and checks. It
is a `dev-flake`, it is small, and it is already Hedronite's. Read on 2026-11-14, its flake would
meet Nix-TOPS like this:

| Finding | Law | Note |
|---|---|---|
| A second nixpkgs input (`nixpkgs-26.05-darwin`) for `x86_64-darwin`, which nixpkgs 26.11 dropped | N-06, Ack | A legitimate exemption. It is exactly what the ack is for |
| `export RUST_TOPS_ROOT=${./.}` in two checks copies the whole tree into the store | N-31, Ack | Every edit, including to the README, rebuilds both checks. `lib.fileset.toSource` over the files the script reads fixes it |
| "No GitHub job runs `nix flake check`" | gates 3–4 | The flake's checks are not part of CI today. The Nix-TOPS `gate` job would add them |
| `pkgsFor` imports nixpkgs once per system with the overlay | N-09, pass | The pattern the law asks for |

The flake already uses `lib.getExe` for its app and splits Darwin-only and Linux-only outputs
carefully. Adopting at `ratchet` costs one ack and one `fileset` change.

---

## Appendix A: Gate order at a glance

```
tier 1 (merge gate)
  1 source    → tracked .nix · flake.lock current · clean tree in CI
  2 lint      → nixfmt · statix · deadnix · nixtops-lint · gitleaks   (sandbox)
  3 eval      → every output · every system · abort-on-warn · no IFD · build roots
  4 build     → nix flake check · packages · devShells · build roots · VM tests
  5 coverage  → every output: evaluated · built · tested
  6 class     → closure bytes · eval counters
tier 0 (executes last)
  ★ laws      → N-NN readers · Refuse fails · not-evaluated ≠ pass
tier 2 (nightly)  cross-system builds · --rebuild determinism · FOD re-fetch · vulnix
tier 3 (release)  signed cache · closure SBOM · tag = version
```

## Appendix B: Corrections to the source sketch

Checked on 2026-11-14 against nixpkgs 26.11 (`nixpkgs-weekly`, 2026-11-01) with Nix 2.35.2.

1. **`lib.mergeAttrsList` exists.** The sketch lists it among invented functions (§4). It is in
   nixpkgs `lib`. `lib.isEmpty` and `builtins.stringSplit` do not exist, as the sketch says.
2. **`nixfmt-rfc-style` warns.** It has been an alias of `nixfmt` since 2025-07-14, defined with
   `warnAlias`. The sketch's §11 check would fail under `abort-on-warn`. Use `pkgs.nixfmt`.
   `nixfmt-classic` has been removed.
3. **`nixfmt --check <directory>` is deprecated.** nixfmt 1.5.0 prints that directory arguments
   will be unsupported soon and points to `nixfmt-tree` or treefmt-nix. The sketch's
   `nixfmt --check ${self}` relies on it.
4. **statix does not catch `with` or `rec`.** A fixture with a file-scope `with pkgs;` and a `rec`
   attrset passed `statix check` with exit 0. The sketch's §3 rules need their own analyzer
   (N-10, N-11). On another fixture, statix reported W04 (`manual_inherit_from`) but not W01
   (`bool_comparison`) for `if a == true`. Calibrate before mapping W01 to anything.
5. **The fabricated hash in §6 is the safe failure.** `sha256-` followed by 64 hex characters is
   not valid SRI, so Nix rejects it immediately. The dangerous case is a valid, stale hash after a
   version bump: the fetcher's store name is `source`, so the old path still matches and the build
   uses the old source with no error (N-26).
6. **`getEnv` does not fail in pure evaluation.** It returns `""`. The sketch's §1 example
   evaluates without error and sets `API_URL` to an empty string. Only an analyzer catches it
   (N-02). `<nixpkgs>` and `fetchTarball` without a hash do fail in pure evaluation.
7. **The §7 network failure depends on the host.** On macOS the sandbox is off by default
   (Determinate Nix on macOS reported `sandbox = false`, checked), so a `buildPhase` that runs `go build` against the network can
   succeed on a Mac and fail on Linux CI.
8. **Path literals copy secrets too.** §10 covers `pkgs.writeText`. A path literal such as
   `environmentFile = ./my-app.env` also copies the file into the store (N-34).
9. **`services.my-app` in §10 is an illustration**, not a nixpkgs module; `sops.secrets.<name>.path`
   assumes sops-nix is imported.

## Appendix C: Sources

- Nix manual: `nix flake check`, `abort-on-warn`, `allow-import-from-derivation`, `NIX_SHOW_STATS`
  (<https://nix.dev/manual/nix/latest/>)
- nixpkgs `lib/trivial.nix` (`warn` delegates to `builtins.warn`) and
  `pkgs/top-level/aliases.nix` (`nixfmt-rfc-style`), on the 26.11 pin
- statix (<https://github.com/oppiliappan/statix>), deadnix (<https://github.com/astro/deadnix>),
  nixfmt (<https://github.com/NixOS/nixfmt>), nix-unit (<https://github.com/nix-community/nix-unit>),
  vulnix (<https://github.com/nix-community/vulnix>), gitleaks (<https://github.com/gitleaks/gitleaks>)
- NixOS manual: writing NixOS tests (`testers.runNixOSTest`)
- Rust-TOPS (<https://github.com/Hedronite/rust-tops>), Go-TOPS
  (<https://github.com/Hedronite/go-tops>), Web-TOPS (<https://github.com/Hedronite/web-tops>), as
  read on 2026-11-14

---

## Credits and license

Built by Hedronite. Structure adapted from Rust-TOPS, Go-TOPS and Web-TOPS. The laws draw on
*Common Mistakes AI Coding Agents Make in Nix* (Hedronite sketch). Licensed under MIT OR
Apache-2.0, as in `LICENSE`, `LICENSE-MIT` and `LICENSE-APACHE`.
