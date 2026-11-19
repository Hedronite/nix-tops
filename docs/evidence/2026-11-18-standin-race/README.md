# The stand-in tool race (`ETXTBSY`)

## What failed

The test suite writes small shell scripts that stand in for `nix` and `nix-eval-jobs`, then runs them. On Linux, `exec` of a file fails with "text file busy" (`ETXTBSY`) while any process still holds that file open for writing. A test thread that writes a script and a second test thread that forks at the same moment can leave the forked child holding the first thread's write handle until it execs, so the first thread's `exec` fails. It showed up as `cannot run <path>/judge: Text file busy (os error 26)` in `outputs_walk.rs` and in `doctor.rs` on the Determinate CI leg. It cannot happen on macOS, which does not refuse the exec.

## The fix

One place, where the library starts a tool: `crates/nixtops/src/exec.rs`. `exec::output` runs a command and, when the program is busy, tries again (40 calls in all, 25 ms apart); any other error is returned at once. `probe::run` (the host's `nix`) and the inventory's `run_program` (the pinned judge and `nix-eval-jobs`) both use it, so every test that starts a stand-in is covered without a change to any test file.

## The proof

A Linux aarch64 container (`rust:1.98-bookworm`, 1.99.1 toolchain installed in it), 2 CPUs, sources copied in as tarballs. Each test binary was built once and then run in a loop with `RUST_TEST_THREADS=48`, counting runs that fail and, of those, runs whose output says `Text file busy`.

| Tree | Binaries | Runs | Failing runs | `Text file busy` |
|---|---|---|---|---|
| `main` before the fix | `doctor`, `outputs_walk` (150 each) | 300 | 28 | 15 |
| fix | `doctor`, `outputs_walk` (150 each) | 300 | 0 | 0 |
| `main` before the fix | all 12 test binaries (25 each) | 300 | 2 | 1 |
| fix | all 12 test binaries (25 each) | 300 | 0 | 0 |

The failing runs that do not say `Text file busy` were not examined separately; they stop with the fix. With 15 test threads (the default for four CPUs), 180 runs on `main` did not fail, so the loop above is a stress setting, not the CI setting.

The retry itself is checked everywhere by unit tests of `exec`: a busy program runs once it is released, a program that stays busy is given up after 40 calls and reported as busy, and any other error returns after one call.
