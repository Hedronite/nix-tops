#!/usr/bin/env bash
# Evaluate every fixture under gate 3's options. Eval only: no builds, never --impure.
# usage: run.sh <cppnix|determinate>   (run from this directory)
set -u
impl=${1:?cppnix|determinate}
export PATH=/nix/var/nix/profiles/default/bin:$PATH
here=$(cd "$(dirname "$0")" && pwd)
out=$here/logs/$impl
mkdir -p "$out"
NIX=(nix --extra-experimental-features 'nix-command flakes')
# gate 3, NIX_TOPS.md section 3.1 (line 103), options verbatim
GATE=(--no-update-lock-file --option abort-on-warn true --option allow-import-from-derivation false)
# control: the same, with abort-on-warn off, to show what is printed
CTRL=(--no-update-lock-file --option abort-on-warn false --option allow-import-from-derivation false)
{ "${NIX[@]}" --version; uname -sm; date -u +%Y-%m-%dT%H:%M:%SZ; } > "$out/VERSION.txt" 2>&1

run() { # name label cmd...
  local n=$1 l=$2; shift 2
  "$@" > "$out/$n.$l.log" 2>&1; echo $? > "$out/$n.$l.exit"
}

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
for f in "$here"/fixtures/*/; do
  n=$(basename "$f"); cp -R "$f" "$tmp/$n"; d="path:$tmp/$n"
  run "$n" check.gate    "${NIX[@]}" flake check --no-build --all-systems "${GATE[@]}" "$d"
  run "$n" check.control "${NIX[@]}" flake check --no-build --all-systems "${CTRL[@]}" "$d"
  case $n in
    builtins-warn)
      run "$n" drv.gate    "${NIX[@]}" eval --raw "$d#packages.x86_64-linux.default.drvPath" "${GATE[@]}"
      run "$n" drv.control "${NIX[@]}" eval --raw "$d#packages.x86_64-linux.default.drvPath" "${CTRL[@]}" ;;
    hm-standalone)
      a=homeConfigurations.fixture
      run "$n" drv.gate      "${NIX[@]}" eval --raw "$d#$a.activationPackage.drvPath" "${GATE[@]}"
      run "$n" drv.control   "${NIX[@]}" eval --raw "$d#$a.activationPackage.drvPath" "${CTRL[@]}"
      run "$n" warnings.data "${NIX[@]}" eval --json "$d#$a.config.warnings" "${GATE[@]}" ;;
    hm-nixos-module)
      a=nixosConfigurations.fixture
      run "$n" drv.gate      "${NIX[@]}" eval --raw "$d#$a.config.system.build.toplevel.drvPath" "${GATE[@]}"
      run "$n" drv.control   "${NIX[@]}" eval --raw "$d#$a.config.system.build.toplevel.drvPath" "${CTRL[@]}"
      run "$n" warnings.data "${NIX[@]}" eval --json "$d#$a.config.warnings" "${GATE[@]}" ;;
  esac
done
for e in "$out"/*.exit; do printf '%s %s\n' "$(basename "$e" .exit)" "$(cat "$e")"; done > "$out/EXIT-CODES.txt"
cat "$out/EXIT-CODES.txt"
