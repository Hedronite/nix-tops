# Agent standing orders (Nix-TOPS 0.2.0, draft)

Goal: pure, correct, dense, fast Nix.
Production: the fewest lines that keep the named invariants and measured budgets.
Tests: as many as the contract needs. Never write tests by reading the implementation.

The protocol text is [`NIX_TOPS.md`](NIX_TOPS.md). This file is the harness for work on this repo.
[`config/AGENTS.template.md`](config/AGENTS.template.md) is the section `nixtops init` will write into an adopting repo. It is not the harness for this repo.

## State

Milestone M0: the spec. There is no `flake.nix`, no runner and no analyzer yet. `nixtops` does not exist. The one workflow, `spec`, runs `scripts/spec_check.py` and `scripts/rules_gen.py` on the spec itself. Do not document, run or claim a command that has not landed.

Once code lands, the kit is a flake that gates itself `hard` with its own protocol (§17), and its Rust analyzer is gated by Rust-TOPS.

Points from 0.2.0 that code must follow:

1. The gate always evaluates with `abort-on-warn = true` and `allow-import-from-derivation = false`, and never with `--impure` (§3.1, §16). Warnings come from three sources, and only `builtins.warn` is fatal by `abort-on-warn` alone (§3.1).
2. Coverage is output coverage, at three levels: evaluated, built, tested (§6.1). There is no CRAP and no mutation layer in 0.x.
3. Closure bytes and eval counters are the budgets (§6.3, §6.4). Eval cost is advisory in 0.x.
4. Config is `nix-tops.toml`, read with `builtins.fromTOML`. It is not a flake output (§9).
5. The kit's `flake.lock` is the tool lock. The `nix-tops` input never follows the adopter's nixpkgs (§10, N-43).
6. Claims about Nix behavior are checked on a named pin, or marked **[verify]**.

## Do this

1. Read the section of `NIX_TOPS.md` you are changing, and the schema. Fix the schema first, then the prose (the Schema precedence rule in Stated rules below).
2. A change to `laws/`, `schema/`, `profiles/`, a floor, a budget, an eval option, or normative text in `NIX_TOPS.md` is a protocol change. It carries the `protocol-change` label and a `CHANGELOG.md` entry (as a fragment in `changelog.d/` until the release folds them), and bumps the protocol version together with the schema.
3. Check Nix behavior before you write it down: evaluate it against the pinned nixpkgs and record the pin. Appendix B exists because the source sketch was not checked.
4. Before a PR is ready, run `python3 scripts/spec_check.py all`, `python3 -m unittest discover -s scripts/tests` and `python3 scripts/rules_gen.py check`. The schema check needs `check-jsonschema` 0.38.0, which needs Python 3.10 or later; an older Python cannot run the schema check.
5. The rules below are generated from `NIX_TOPS.md` §3.4. Edit §3.4, then run `python3 scripts/rules_gen.py write`.
6. Once code lands: write the oracle from the contract first, then the smallest shape that satisfies it, then run the gate.

## Git

- Work on a branch and open a PR ready for review when the slice is complete, never a draft. Never push `main`, force-push, tag or publish a release.
- Fixes are new commits on the PR branch.
- No secrets, tokens, local paths or internal notes in the repo.

## Stated rules

<!-- rules:begin -->
<!-- generated from NIX_TOPS.md section 3.4 by scripts/rules_gen.py; edit the source, not this copy -->

- **Oracle first.** Write the oracle from the contract before the code. A test that asserts what a module currently produces freezes its bugs. **[review, N-39]**
- **Pure or nothing.** The gate never passes `--impure`. A change that needs `--impure` to evaluate is not done. **[gate 3]**
- **Do not weaken a law**, a linter setting, an eval option or a budget so a check passes. Fix the code, or change the law through a `protocol-change` PR (§13). **[`law-diff`, §11]**
- **Never fabricate a hash.** Use `lib.fakeHash`, build, and copy the `got:` value. A placeholder is never committed. **[N-25]**
- **Every ack names its law and a reason** (`# nixtops-ack N-NN: reason`). Never edit `.nix-tops/baseline.json` by hand, and never wrap old findings in acks to zero a count. **[§3.3, §6.8]**
- **No silent empties.** A flake with no checks, an eval that enumerated no outputs, or a report with no layers fails the gate. `not-evaluated` and an unmapped layer are never a pass. **[§3.3]**
- **No dry gates in CI.** `--dry-run` is refused when `CI=true`. **[§12]**
- **One gate.** `nixtops gate` is the verification command and the definition of done. Running `nix flake check` alone is not done. **[§11]**
- **Do not edit generated files** (`hardware-configuration.nix`, `flake.lock` by hand, any file a generator writes). Regenerate them, or fix the generator. **[review]**
- **Agent-authored code meets the same gate as human code.** Prefer deleting code to adding configuration, and add a dependency only if it deletes code or buys measured safety or performance. **[review]**
- **Schema precedence.** When prose and schema disagree, fix the schema first, then the prose. **[review]**
<!-- rules:end -->

## Forbidden in kit code

- `import <nixpkgs>`, `builtins.getEnv`, or import-from-derivation (N-01, N-02, N-07).
