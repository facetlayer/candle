/-!
# The start / kill / monitor protocol

The CLI and the detached monitor of the *previous* instance share two tables:
the `processes` row and the append-only `process_output` log. `start` writes a
`process_start_initiated` row that every log reader treats as the boundary of
the new run, so all of the previous instance's rows must land *before* it.
`start_one_service` tries to ensure that:

    // Wait for the old shell and its monitor to be gone first so every stale
    // row lands before the boundary.
    wait_for_pids_to_exit(&previous_pids, PREVIOUS_INSTANCE_DRAIN_TIMEOUT);

where `previous_pids` comes from rows with `killed_at IS NULL`.

This file models each CLI entry point as a straight-line program over the
shared state and explores **every interleaving** with the old monitor.

`startProg` / `restartProg` model the code as it was when the bug was found;
`startNow` / `restartNow` at the end model the current code, which tags rows
with their run instead of relying on their order.

Modelling choices (all conservative for the property checked):
* The old service's shell dies as soon as it is signalled; `kill` then returns
  (`kill_process_tree_and_wait` waits only for the shell's process tree, which
  does not include the monitor, its parent).
* After its shell dies the old monitor may write late output (drained from the
  pipes, `drain_after_exit`), then writes `process_exited` and deletes its row.
* `wait_for_pids_to_exit` blocks until the listed pids are gone. Its 2 s timeout
  is not modelled: we assume the monitor finishes in time, which only makes the
  `start` result stronger than reality.
-/

namespace Candle.Protocol

inductive Ev where
  | oldOutput   -- late stdout/stderr from the previous instance
  | oldExited   -- previous monitor's `process_exited`
  | newStart    -- the new `process_start_initiated` boundary
  deriving DecidableEq, Repr

inductive Mon where
  | running | shellDead | drained | done
  deriving DecidableEq, Repr

/-- Straight-line CLI steps, named after the Rust code they model. -/
inductive Instr where
  /-- `find_processes_by_command_name_and_project_dir` in `kill_by_command_name`. -/
  | killReadRows
  /-- `kill_one_running_process` on the row read above: mark `killed_at` if not
  already marked, signal; `ProcessNotFound` deletes the row. -/
  | killEntry
  /-- `previous_instance_pids` in `start_one_service`: rows with no `killed_at`. -/
  | readPrevPids
  /-- `wait_for_pids_to_exit(&previous_pids, ..)`. -/
  | waitPrevPids
  /-- `save_process_log(.., ProcessStartInitiated, ..)`. -/
  | writeStart
  deriving DecidableEq, Repr

structure Row where
  killed : Bool
  deriving DecidableEq, Repr

structure St where
  pc : Nat
  row : Option Row            -- the previous instance's `processes` row
  snap : Option Row           -- rows read by the current `kill`
  waitForMonitor : Bool       -- is the old monitor pid in `previous_pids`?
  shellAlive : Bool
  mon : Mon
  log : List Ev
  deriving DecidableEq, Repr

/-- The previous instance is up and running; nothing has been killed. -/
def init : St :=
  { pc := 0, row := some ⟨false⟩, snap := none, waitForMonitor := false,
    shellAlive := true, mon := .running, log := [] }

/-- One CLI step, or `none` if finished or blocked. -/
def cliStep (prog : List Instr) (st : St) : Option St :=
  match prog[st.pc]? with
  | none => none
  | some i =>
    let st := { st with pc := st.pc + 1 }
    match i with
    | .killReadRows => some { st with snap := st.row }
    | .killEntry =>
      match st.snap with
      | none => some st
      | some e =>
        -- mark before signalling (an UPDATE on a deleted row is a no-op)
        let row := if e.killed then st.row else st.row.map (fun _ => ⟨true⟩)
        if st.shellAlive then
          -- Terminated: the shell is gone, `killed_at` stays set, the row stays
          some { st with row := row, shellAlive := false,
                         mon := if st.mon = .running then .shellDead else st.mon }
        else
          -- ProcessNotFound: delete the row
          some { st with row := none }
    | .readPrevPids =>
      some { st with waitForMonitor := match st.row with
                                       | some r => !r.killed
                                       | none => false }
    | .waitPrevPids =>
      if st.waitForMonitor && st.mon ≠ .done then none else some st
    | .writeStart => some { st with log := st.log ++ [.newStart] }

/-- One step of the previous instance's monitor, or `none` if it cannot move. -/
def monStep (st : St) : Option St :=
  match st.mon with
  | .running => none                                   -- needs a signal first
  | .shellDead => some { st with mon := .drained, log := st.log ++ [.oldOutput] }
  | .drained => some { st with mon := .done, log := st.log ++ [.oldExited], row := none }
  | .done => none

/-- The logs of every maximal interleaving (bounded by `fuel`, which the
programs below never exhaust: each step advances the CLI or the monitor). -/
def runs (prog : List Instr) : Nat → St → List (List Ev)
  | 0, st => [st.log]
  | n + 1, st =>
    match cliStep prog st, monStep st with
    | none, none => [st.log]
    | some a, none => runs prog n a
    | none, some b => runs prog n b
    | some a, some b => runs prog n a ++ runs prog n b

/-- Every row of the previous instance precedes the new launch boundary. -/
def boundaryClean (log : List Ev) : Bool :=
  match log.idxOf? .newStart with
  | none => true
  | some i => (log.drop i).all (· == .newStart)

/-! ## Entry points -/

/-- `candle start svc` (and the start inside `start` when the service runs):
`previous_instance_pids`, then `handle_kill_command`, wait, launch. -/
def startProg : List Instr :=
  [.readPrevPids, .killReadRows, .killEntry, .waitPrevPids, .writeStart]

/-- `candle restart svc`: `handle_restart` first calls `handle_kill_command`
itself, then `start_one_service`, whose own kill finds the already-marked row. -/
def restartProg : List Instr :=
  [.killReadRows, .killEntry] ++ startProg

/-- `candle kill svc` followed by `candle start svc` is the same program. -/
def killThenStartProg : List Instr := restartProg

def fuel : Nat := 32

/-! ## Results -/

/-- `start` is safe: in **every** interleaving, the previous instance's late
output and exit land before the new boundary. -/
theorem start_boundary_clean :
    (runs startProg fuel init).all boundaryClean = true := by decide

/-- `restart` is not: some interleaving writes the previous instance's rows
*after* the new boundary. -/
theorem restart_boundary_violated :
    (runs restartProg fuel init).any (fun l => !boundaryClean l) = true := by decide

/-- In fact under `restart` the wait never covers the monitor, so the monitor can
put *all* its remaining rows after the boundary. -/
theorem restart_worst_case :
    [Ev.newStart, .oldOutput, .oldExited] ∈ runs restartProg fuel init := by decide

/-- The root cause: after `restart`'s own kill marks the row, `start`'s
`previous_instance_pids` leaves the monitor out of the wait set. -/
example :
    let st1 := (cliStep restartProg init).bind (cliStep restartProg)   -- kill: read, mark+signal
    let st2 := st1.bind (cliStep restartProg)                          -- readPrevPids
    st2.map (·.waitForMonitor) = some false := by decide

/-! ## Current code: every row tagged with its run, no wait

`start` no longer waits for the previous instance: `previous_instance_pids` and
`wait_for_pids_to_exit` are gone. Each row carries its run id, the old
monitor's rows the old run's and the new launch marker the new run's, and
readers select a command's highest run (`RunFilter.lean`). -/

def startNow : List Instr := [.killReadRows, .killEntry, .writeStart]
def restartNow : List Instr := [.killReadRows, .killEntry] ++ startNow

/-- The run each event is tagged with: the previous instance's rows keep its run. -/
def runOf : Ev → Nat
  | .oldOutput | .oldExited => 1
  | .newStart => 2

/-- The rows readers attribute to the latest run are exactly the new run's. -/
def latestRunClean (log : List Ev) : Bool :=
  let latest := (log.map runOf).foldl max 0
  log.all (fun e => runOf e != latest || e == .newStart)

/-- Without the wait, the previous instance's rows still land after the new
launch marker in some interleavings, for `start` as well as `restart`… -/
theorem now_rows_still_reorder :
    (runs startNow fuel init).any (fun l => !boundaryClean l) = true ∧
    (runs restartNow fuel init).any (fun l => !boundaryClean l) = true := by decide

/-- …but it no longer matters: in every interleaving the latest run, selected by
tag, contains none of them. -/
theorem now_latest_run_clean :
    (runs startNow fuel init).all latestRunClean = true ∧
    (runs restartNow fuel init).all latestRunClean = true := by decide

end Candle.Protocol
