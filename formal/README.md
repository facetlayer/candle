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
| `Candle/Protocol.lean` | `start` / `restart` / `kill` racing the previous instance's monitor over the `processes` row and the log table | `rows_can_reorder`: the old instance's rows can land after the new launch marker. `latest_run_clean`: tagged with runs, they never appear in the latest run, in any interleaving. |

Regression tests: `a_previous_runs_late_rows_stay_out_of_the_latest_run` in
`rust/src/logs/process_logs.rs`, and "previous instance rows stay out of the new
run" in `test/cli/restart.test.ts`.

## Adding a model

Keep models small and name each definition after the Rust function it mirrors.
State modelling assumptions in the module docstring. Prefer `decide` for
finite-state checks, and write general proofs for properties over all inputs.
