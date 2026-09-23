# Formal models of Candle (Lean 4)

Machine-checked models of Candle's core coordination logic. They don't extract
code: each model mirrors a piece of `rust/src`, and a theorem either proves a
property of it or exhibits a concrete counterexample.

## Build

```sh
curl -sSfL https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh | sh -s -- -y
cd formal && lake build
```

A successful build means every theorem checked. No Mathlib is needed, and the
models use no `sorry`.

## Contents

| File | Models | Main results |
|---|---|---|
| `Candle/LogFilter.lean` | `LatestExecutionLogFilter` (`rust/src/log_filters/latest_execution_log_filter.rs`) | `batch_shows_stale_exit`: in batch mode (`check_latest_launch_status` + `filter` on the same rows, as `logs`, `wait-for-log` and `watch`'s first print do), a previous instance's `process_exited` is shown as part of the new run. `replayed_old_result_marks_new_launch`: an older launch's `process_started` in the same batch has the same effect. |
| `Candle/LogFilterFix.lean` | A corrected filter that records the *id* of the launch's first start result instead of a `bool` | `batchFix_eq_spec`, `streamFix_eq_spec`: the fix meets the spec for **all** chronological inputs, in batch and streaming mode. `stream_orig_eq_fix`: the current code is already correct in streaming mode, so the defect is confined to batch use. |
| `Candle/Protocol.lean` | `start` / `restart` / `kill` racing the previous instance's monitor over the `processes` row and log table | `start_boundary_clean`: `start` keeps every stale row before the new launch boundary in every interleaving. `restart_boundary_violated`: `restart` (and `kill` then `start`) does not, because `previous_instance_pids` skips rows `kill` already marked. `fixed_restart_boundary_clean`: waiting on marked rows too fixes it. |

### Status

Both defects are now fixed in `rust/src`. `LogFilter.lean`, and `restartProg` with plain
`readPrevPids` in `Protocol.lean`, model the code **before** the fix; they are kept as the
record of what the proofs found. The current Rust code corresponds to `LogFilterFix.lean`
(`LaunchStatus::start_result_id`) and to `fixStart` in `Protocol.lean` (`previous_instance_pids`
now includes rows already marked killed). Regression tests: the `latest_execution_log_filter`
unit tests and "previous instance rows stay out of the new run" in `test/cli/restart.test.ts`.

## Spec used for the log filter

An `exited` row is *stale* if it follows a `process_start_initiated` with no
start result (`process_started` / `process_start_failed`) in between. A monitor
writes `process_exited` only after its start result, so such a row belongs to
the previous instance (see the comment in `filter`). Batch output must be
exactly the rows at or after the latest launch marker, minus stale exits.

## Adding a model

Keep models small and name each definition after the Rust function it mirrors.
State modelling assumptions in the module docstring. Prefer `decide` for
finite-state checks and write general proofs for properties over all inputs.
