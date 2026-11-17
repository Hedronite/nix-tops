# Quickstart: the adopter flake and a stub kit

`flake.nix` is the adopter flake of NIX_TOPS.md section 4, word for word. `stub-kit/` is a stand-in for
the kit that exports the documented interface with no behaviour: `lib.mkChecks` (per-system checks,
lint on the first system only), `apps.<system>.nixtops` and `formatter.<system>`.

`verify.sh` evaluates the adopter against the stub (eval only, never `--impure`, `abort-on-warn` on,
IFD off): `nix flake check --no-build --all-systems --no-update-lock-file`, then lists the merged
checks per system, then renames the adopter's own check in a copy and confirms the rename shows.

**Stub-only step.** The kit is not published yet, so `verify.sh` passes
`--override-input nix-tops path:./stub-kit`. `flake.lock` pins nixpkgs and has no `nix-tops` node
(a lock for the stub would hold a host path). The override therefore prints one
`warning: not writing modified lock file ...` line. An adopter locks the real kit and never runs this
override or sees the line.

Checked 2026-11-17 on CppNix 2.35.2 and Determinate Nix 3.22.3 (aarch64-darwin), nixpkgs
`d5cc6833999746cffdfe0598f7bc801a57b887a9` (the lock).
