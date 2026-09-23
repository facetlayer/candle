/-!
# Model of `LatestExecutionLogFilter`

Mirrors `rust/src/log_filters/latest_execution_log_filter.rs`.

Scope of the model:
* One command. The Rust filter keys its state by `command_name` and entries
  for different commands never interact, so each command is filtered
  independently.
* No `recent_window_ms` time window (`logs` and `wait-for-log` pass `None`).
* Rows are identified by their autoincrement `id`; `content` is irrelevant.
-/

namespace Candle.LogFilter

inductive LogType where
  | stdout | stderr | startInitiated | startFailed | started | exited
  deriving DecidableEq, Repr

structure Log where
  id : Nat
  ty : LogType
  deriving DecidableEq, Repr

/-- `process_started` / `process_start_failed`: the monitor's start result. -/
def isStartResult : LogType → Bool
  | .started | .startFailed => true
  | _ => false

/-- Rows are read in chronological order: ids strictly increasing. -/
abbrev Chrono (ls : List Log) : Prop := ls.Pairwise (fun a b => a.id < b.id)

inductive Behavior where
  | showLogsFromPreviousLaunch
  | onlyShowAfterRecentLaunch
  deriving DecidableEq, Repr

/-! ## The filter as implemented -/

/-- `LaunchStatus { start_log_id, reported_start_result }` -/
structure Status where
  startId : Nat
  reported : Bool
  deriving DecidableEq, Repr

/-- One iteration of the loop in `check_latest_launch_status`. -/
def checkStep (s : Option Status) (l : Log) : Option Status :=
  if l.ty = .startInitiated then some ⟨l.id, false⟩
  else if isStartResult l.ty then s.map (fun st => { st with reported := true })
  else s

/-- `check_latest_launch_status` (the map is cleared first, hence `none`). -/
def check (ls : List Log) : Option Status := ls.foldl checkStep none

/-- One iteration of the loop in `filter`: the new state and whether the row is
included. -/
def filterStep (b : Behavior) (s : Option Status) (l : Log) : Option Status × Bool :=
  -- "A launch event moves the boundary forward — but only forward."
  let s1 :=
    if l.ty = .startInitiated then
      match s with
      | none => some ⟨l.id, false⟩
      | some st => if l.id > st.startId then some ⟨l.id, false⟩ else s
    else s
  let incl :=
    match s1 with
    | some st =>
      if l.ty = .exited ∧ st.reported = false then false
      else decide (l.id ≥ st.startId)
    | none => b == .showLogsFromPreviousLaunch
  let s2 := if isStartResult l.ty then s1.map (fun st => { st with reported := true }) else s1
  (s2, incl)

/-- `filter`: returns the final state and the included rows. -/
def filter (b : Behavior) : Option Status → List Log → Option Status × List Log
  | s, [] => (s, [])
  | s, l :: ls =>
    let (s', inc) := filterStep b s l
    let (s'', out) := filter b s' ls
    (s'', if inc then l :: out else out)

/-- Batch mode, as used by `logs`, `wait-for-log`'s initial scan and `watch`'s
first print: `check_latest_launch_status(batch)` then `filter(batch)`. -/
def batch (b : Behavior) (ls : List Log) : List Log := (filter b (check ls) ls).2

/-- Streaming mode: `check(initial)`, then each later chunk is passed to
`filter` with the state carried over. (Here the initial batch itself is not
re-filtered; `streamAfter` returns only the output for `rest`.) -/
def streamAfter (b : Behavior) (initial rest : List Log) : List Log :=
  (filter b (check initial) rest).2

/-! ## The property the filter is meant to guarantee

From the code comment: "A monitor only writes process_exited after it has
written process_started, so an exit seen before this launch's start result
belongs to the instance that was just killed". So an `exited` row that follows
a `process_start_initiated` with no start result in between is *stale*: it
belongs to the previous instance and must not be shown as part of this run. -/

def StaleExit (ls : List Log) (r : Log) : Prop :=
  r.ty = .exited ∧
  ∃ s ∈ ls, s.ty = .startInitiated ∧ s.id < r.id ∧
    ∀ x ∈ ls, isStartResult x.ty = true → ¬ (s.id < x.id ∧ x.id < r.id)

instance (ls : List Log) (r : Log) : Decidable (StaleExit ls r) := by
  unfold StaleExit; infer_instance

/-! ## The bug: batch mode shows a stale exit

The real row order produced by `candle restart` (see `Protocol.lean`):
new launch marker, then the old monitor's `process_exited`, then the new
monitor's `process_started`. -/

def restartTrace : List Log :=
  [⟨1, .startInitiated⟩, ⟨2, .started⟩, ⟨3, .stdout⟩,   -- previous run
   ⟨4, .startInitiated⟩,                                 -- new launch boundary
   ⟨5, .exited⟩,                                         -- OLD monitor: "Process was stopped"
   ⟨6, .started⟩, ⟨7, .stdout⟩]                          -- new run

example : Chrono restartTrace := by decide
example : StaleExit restartTrace ⟨5, .exited⟩ := by decide

/-- Batch mode (`candle logs`, `wait-for-log`) includes the stale exit. -/
theorem batch_shows_stale_exit :
    ⟨5, .exited⟩ ∈ batch .onlyShowAfterRecentLaunch restartTrace ∧
    ⟨5, .exited⟩ ∈ batch .showLogsFromPreviousLaunch restartTrace := by decide

/-- Streaming mode hides it: e.g. `watch` saw rows 1–4, then polls 5–7. -/
theorem stream_hides_stale_exit :
    streamAfter .onlyShowAfterRecentLaunch (restartTrace.take 4) (restartTrace.drop 4)
      = [⟨6, .started⟩, ⟨7, .stdout⟩] := by decide

/-- A second, independent way to defeat the check even in streaming mode: the
previous launch's `process_started`, replayed in the same batch, marks the *new*
launch as having reported its start result. -/
theorem replayed_old_result_marks_new_launch :
    let ls : List Log := [⟨1, .startInitiated⟩, ⟨2, .started⟩, ⟨3, .startInitiated⟩, ⟨4, .exited⟩]
    StaleExit ls ⟨4, .exited⟩ ∧ ⟨4, .exited⟩ ∈ batch .onlyShowAfterRecentLaunch ls := by decide

end Candle.LogFilter
