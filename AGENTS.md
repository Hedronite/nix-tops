# Agent standing orders (Nix-TOPS 0.1.0, draft)

Goal: pure, correct, dense, fast Nix.
Production: the fewest lines that keep the named invariants and measured budgets.
Tests: as many as the contract needs. Never write tests by reading the implementation.

The protocol text is [`NIX_TOPS.md`](NIX_TOPS.md). This file is the harness for work on this repo.
[`config/AGENTS.template.md`](config/AGENTS.template.md) is the section `nixtops init` will write into an adopting repo. It is not the harness for this repo.

## State

Milestone M0: the spec. There is no `flake.nix`, no runner and no analyzer yet. `nixtops` does not exist. The one workflow, `spec`, runs `scripts/spec_check.py` on the spec itself. Do not document, run or claim a command that has not landed.

Once code lands, the kit is a flake that gates itself `hard` with its own protocol (§17), and its Rust analyzer is gated by Rust-TOPS.

Points from 0.1.0 that code must follow:

1. The gate always evaluates with `abort-on-warn = true` and `allow-import-from-derivation = false`, and never with `--impure` (§3.1, §16).
2. Coverage is output coverage, at three levels: evaluated, built, tested (§6.1). There is no CRAP and no mutation layer in 0.1.
3. Closure bytes and eval counters are the budgets (§6.3, §6.4).
4. Config is `nix-tops.toml`, read with `builtins.fromTOML`. It is not a flake output (§9).
5. The kit's `flake.lock` is the tool lock. The `nix-tops` input never follows the adopter's nixpkgs (§10, N-43).
6. Claims about Nix behavior are checked on a named pin, or marked **[verify]**.

## Do this

1. Read the section of `NIX_TOPS.md` you are changing, and the schema once it exists. Fix the schema first, then the prose (§3.4). Before a PR is ready, run `python3 scripts/spec_check.py all` (needs `check-jsonschema` 0.38.0).
2. A change to `laws/`, `schema/`, `profiles/`, a floor, a budget, an eval option, or normative text in `NIX_TOPS.md` is a protocol change. It carries the `protocol-change` label and a `CHANGELOG.md` entry, and bumps the protocol version together with the schema.
3. Check Nix behavior before you write it down: evaluate it against the pinned nixpkgs and record the pin. Appendix B exists because the source sketch was not checked.
4. Once code lands: write the oracle from the contract first, then the smallest shape that satisfies it, then run the gate.

## Git

- Work on a branch and open a PR ready for review when the slice is complete, never a draft. Never push `main`, force-push, tag or publish a release.
- Fixes are new commits on the PR branch.
- No secrets, tokens, local paths or internal notes in the repo.

## Forbidden

- Writing tests from the current function body or module output.
- Weakening a law, lint setting, eval option or budget so a check passes.
- Counting `not-evaluated`, an unmapped layer, or an empty report as a pass.
- `--impure`, `import <nixpkgs>`, `builtins.getEnv` or IFD in kit code.
- Committing `lib.fakeHash` or any hash Nix did not print.
- An ack without the law ID and a reason.
- Editing generated files. Fix the generator.
- Adding a dependency that does not delete code or buy measured safety or performance.
