/-!
# The start / kill / monitor protocol

The CLI and the detached monitor of the *previous* instance share two tables:
the `processes` row and the append-only `process_output` log. `start` writes a
`process_start_initiated` row that opens the new run, without waiting for the
previous instance's monitor to finish. Every row is tagged with its run, and
readers select a command's highest run (`RunFilter.lean`).

This file models each CLI entry point as a straight-line program over the
shared state and explores **every interleaving** with the old monitor.

Modelling choices (all conservative for the property checked):
* The old service's shell dies as soon as it is signalled; `kill` then returns
  (`kill_process_tree_and_wait` waits only for the shell's process tree, which
  does not include the monitor, its parent).
* After its shell dies the old monitor may write late output (drained from the
  pipes, `drain_after_exit`), then writes `process_exited` and deletes its row.
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
  shellAlive : Bool
  mon : Mon
  log : List Ev
  deriving DecidableEq, Repr

/-- The previous instance is up and running; nothing has been killed. -/
def init : St :=
  { pc := 0, row := some ⟨false⟩, snap := none,
    shellAlive := true, mon := .running, log := [] }

/-- One CLI step, or `none` if finished. -/
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

/-- `candle start svc` when the service runs: `handle_kill_command`, launch. -/
def startProg : List Instr := [.killReadRows, .killEntry, .writeStart]

/-- `candle restart svc`: `handle_restart` first calls `handle_kill_command`
itself, then `start_one_service`, whose own kill finds the already-marked row. -/
def restartProg : List Instr := [.killReadRows, .killEntry] ++ startProg

def fuel : Nat := 32

/-! ## Results -/

/-- The run each event is tagged with: the previous instance's rows keep its run. -/
def runOf : Ev → Nat
  | .oldOutput | .oldExited => 1
  | .newStart => 2

/-- The rows readers attribute to the latest run are exactly the new run's. -/
def latestRunClean (log : List Ev) : Bool :=
  let latest := (log.map runOf).foldl max 0
  log.all (fun e => runOf e != latest || e == .newStart)

/-- The previous instance's rows can land after the new launch marker in some
interleavings, for `start` as well as `restart`… -/
theorem rows_can_reorder :
    (runs startProg fuel init).any (fun l => !boundaryClean l) = true ∧
    (runs restartProg fuel init).any (fun l => !boundaryClean l) = true := by decide

/-- …but it does not matter: in every interleaving the latest run, selected by
tag, contains none of them. -/
theorem latest_run_clean :
    (runs startProg fuel init).all latestRunClean = true ∧
    (runs restartProg fuel init).all latestRunClean = true := by decide

end Candle.Protocol
