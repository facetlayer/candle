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
| `Candle/RunFilter.lean` | `LatestRunFilter` (`rust/src/log_filters/latest_run_filter.rs`): every row carries its run id; a row is shown iff its run is its command's latest | `batch_eq_spec`: seeded from the database, filtering **any** batch in **any** order returns exactly each command's latest run. `stream_never_superseded`: while streaming, a row from a superseded run is never shown. No ordering assumption anywhere. |
| `Candle/Protocol.lean` | `start` / `restart` / `kill` racing the previous instance's monitor over the `processes` row and the log table | Current code (`startNow`, `restartNow`): `now_rows_still_reorder` shows the old instance's rows can still land after the new launch marker, and `now_latest_run_clean` shows that, tagged with runs, they never appear in the latest run. Historical (`startProg`, `restartProg`): the order-based design kept `start` clean (`start_boundary_clean`) but not `restart` (`restart_boundary_violated`). |
| `Candle/LogFilter.lean` | The original `LatestExecutionLogFilter`, which told runs apart by row order | Kept as the record of the bug the models found: `batch_shows_stale_exit` and `replayed_old_result_marks_new_launch`. |

## History

The first models found two bugs in the order-based design:

- `restart`, and `kill` followed by `start`, skipped the wait for the previous
  instance's monitor. That instance's last output and exit could land after the
  new launch marker.
- `LatestExecutionLogFilter` showed such a stale exit whenever it pre-scanned and
  filtered one batch, as `logs` did.

An interim fix patched both; it was proved correct in `LogFilterFix.lean`, since
removed (see git history). The current design removes the root cause instead:
every row records its run, so row order no longer matters. The previous-instance
wait, the stale-exit special case, and the proofs' ordering assumptions are all
gone.

Regression tests: `a_previous_runs_late_rows_stay_out_of_the_latest_run` in
`rust/src/logs/process_logs.rs`, and "previous instance rows stay out of the new
run" in `test/cli/restart.test.ts`.

## Adding a model

Keep models small and name each definition after the Rust function it mirrors.
State modelling assumptions in the module docstring. Prefer `decide` for
finite-state checks, and write general proofs for properties over all inputs.
