#!/bin/sh
# Usage: setup.sh <empty-scratch-dir>. Builds the fixture as its own git repository.
set -eu
d=$1
here=$(cd "$(dirname "$0")" && pwd)
cp "$here/flake.nix" "$here/nix-tops.toml" "$d/"
cd "$d"
git init -q .
git add flake.nix nix-tops.toml
git -c user.name=fixture -c user.email=fixture@example.invalid commit -q -m base
