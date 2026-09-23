/-!
# Model of `LatestRunFilter`

Mirrors `rust/src/log_filters/latest_run_filter.rs`. Every row carries the run
it belongs to (`run_id`, `none` only before a service's first launch), and the
filter keeps a row iff its run is the highest seen for its command.

Unlike `LogFilter.lean`, nothing here assumes rows arrive in id order: the main
results hold for **every** list of rows. That is the point of the design, since
a previous instance's monitor can write its last rows after a relaunch.

* `batch_eq_spec`: seeded with the database's latest run per command
  (`seed_latest_runs`), filtering any batch of stored rows returns exactly the
  rows of each command's latest run.
* `stream_never_superseded`: while streaming, a row is only ever shown if no
  row of a newer run (for its command) came before it, and it is not older
  than the seeded run.
-/

namespace Candle.RunFilter

structure Row where
  id : Nat
  cmd : Nat
  run : Option Nat
  deriving DecidableEq, Repr

/-! ## `Option Nat` ordered with `none` least, as in Rust -/

def omax : Option Nat → Option Nat → Option Nat
  | none, b => b
  | a, none => a
  | some a, some b => some (max a b)

/-- `a ≤ b` in Rust's `Option` order. -/
def ole (a b : Option Nat) : Prop := omax a b = b

theorem omax_comm (a b : Option Nat) : omax a b = omax b a := by
  cases a <;> cases b <;> simp [omax, Nat.max_comm]

theorem omax_assoc (a b c : Option Nat) : omax (omax a b) c = omax a (omax b c) := by
  cases a <;> cases b <;> cases c <;> simp [omax, Nat.max_assoc]

theorem omax_self (a : Option Nat) : omax a a = a := by cases a <;> simp [omax]

theorem ole_omax_left (a b : Option Nat) : ole a (omax a b) := by
  unfold ole; rw [← omax_assoc, omax_self]

theorem ole_omax_right (a b : Option Nat) : ole b (omax a b) := by
  rw [omax_comm]; exact ole_omax_left b a

theorem ole_trans {a b c : Option Nat} (h1 : ole a b) (h2 : ole b c) : ole a c := by
  unfold ole at *; rw [← h2, ← omax_assoc, h1]

/-! ## The filter as implemented -/

/-- Latest run per command (`HashMap<String, Option<i64>>`, missing = `None`). -/
abbrev Latest := Nat → Option Nat

/-- `note_run` -/
def update (m : Latest) (r : Row) : Latest :=
  fun c => if c = r.cmd then omax (m c) r.run else m c

/-- `filter`: note the row's run, then keep it iff it is the latest. -/
def filter : Latest → List Row → Latest × List Row
  | m, [] => (m, [])
  | m, r :: rs =>
    let m' := update m r
    let (m'', out) := filter m' rs
    (m'', if r.run = m' r.cmd then r :: out else out)

/-- The latest run of command `c` among `rows`: what `latest_run_ids` computes
with `max(run_id) ... group by command_name`. -/
def maxRun (rows : List Row) (c : Nat) : Option Nat :=
  rows.foldr (fun r acc => if r.cmd = c then omax r.run acc else acc) none

/-- `seed_latest_runs`: start from the database's latest runs. -/
def seed (db : List Row) : Latest := maxRun db

/-! ## Batch correctness, for any row order -/

theorem run_le_maxRun {db : List Row} {r : Row} (h : r ∈ db) :
    ole r.run (maxRun db r.cmd) := by
  induction db with
  | nil => simp at h
  | cons x t ih =>
    simp only [maxRun, List.foldr_cons]
    rcases List.mem_cons.1 h with rfl | ht
    · simp only [↓reduceIte]; exact ole_omax_left _ _
    · have := ih ht
      split
      · exact ole_trans this (ole_omax_right _ _)
      · exact this

/-- Rows already in the database never move a seeded filter. -/
theorem update_seed {db : List Row} {r : Row} (h : r ∈ db) : update (seed db) r = seed db := by
  funext c
  unfold update seed
  split
  · rename_i hc; subst hc
    have := run_le_maxRun h
    unfold ole at this; rw [omax_comm]; exact this
  · rfl

/-- **Batch correctness.** Seeded from the database, filtering any batch of
stored rows, in any order, returns exactly each command's latest run. -/
theorem batch_eq_spec (db batch : List Row) (hsub : ∀ r ∈ batch, r ∈ db) :
    (filter (seed db) batch).2 = batch.filter (fun r => r.run = maxRun db r.cmd) := by
  induction batch with
  | nil => rfl
  | cons r t ih =>
    have hr : r ∈ db := hsub r (by simp)
    simp only [filter, update_seed hr]
    rw [ih (fun x hx => hsub x (by simp [hx]))]
    by_cases h : r.run = maxRun db r.cmd <;> simp [h, seed]

/-! ## Streaming: a superseded run is never shown -/

theorem filter_fst (m : Latest) (rows : List Row) : (filter m rows).1 = rows.foldl update m := by
  induction rows generalizing m with
  | nil => rfl
  | cons r t ih => simp only [filter, List.foldl_cons]; exact ih _

theorem filter_append (m : Latest) (a b : List Row) :
    (filter m (a ++ b)).2 = (filter m a).2 ++ (filter (filter m a).1 b).2 := by
  induction a generalizing m with
  | nil => rfl
  | cons r t ih =>
    simp only [List.cons_append, filter]
    rw [ih]
    split <;> simp

/-- The filter's state only grows, and covers every row seen. -/
theorem foldl_update_ge (m : Latest) (pre : List Row) (c : Nat) :
    ole (m c) ((pre.foldl update m) c) ∧
    ∀ x ∈ pre, x.cmd = c → ole x.run ((pre.foldl update m) c) := by
  induction pre generalizing m with
  | nil => exact ⟨omax_self _, by simp⟩
  | cons r t ih =>
    obtain ⟨ih1, ih2⟩ := ih (update m r)
    simp only [List.foldl_cons]
    have step : ole (m c) (update m r c) := by
      unfold update; split
      · exact ole_omax_left _ _
      · exact omax_self _
    refine ⟨ole_trans step ih1, ?_⟩
    intro x hx hxc
    rcases List.mem_cons.1 hx with rfl | hx
    · have : ole x.run (update m x c) := by
        unfold update; simp only [hxc.symm, ↓reduceIte]; exact ole_omax_right _ _
      exact ole_trans this ih1
    · exact ih2 x hx hxc

/-- **Streaming safety.** However rows arrive, a row `r` shown after the rows
`pre` has a run at least as new as the seeded run and as every earlier row of
its command, i.e. rows from a superseded run are never shown. -/
theorem stream_never_superseded (m : Latest) (pre : List Row) (r : Row)
    (hshown : r ∈ (filter (pre.foldl update m) [r]).2) :
    ole (m r.cmd) r.run ∧ ∀ x ∈ pre, x.cmd = r.cmd → ole x.run r.run := by
  simp only [filter] at hshown
  have heq : r.run = update (pre.foldl update m) r r.cmd := by
    split at hshown
    · assumption
    · simp at hshown
  have hlast : ole ((pre.foldl update m) r.cmd) r.run := by
    rw [heq]; unfold update; simp only [↓reduceIte]; exact ole_omax_left _ _
  obtain ⟨h1, h2⟩ := foldl_update_ge m pre r.cmd
  exact ⟨ole_trans h1 hlast, fun x hx hc => ole_trans (h2 x hx hc) hlast⟩

/-- The streamed output is the concatenation of those per-row decisions. -/
theorem stream_decomposes (m : Latest) (pre : List Row) (r : Row) (post : List Row) :
    (filter m (pre ++ r :: post)).2 =
      (filter m pre).2 ++ (filter (pre.foldl update m) [r]).2 ++
        (filter ((pre ++ [r]).foldl update m) post).2 := by
  rw [show pre ++ r :: post = (pre ++ [r]) ++ post by simp, filter_append, filter_append,
    filter_fst, filter_fst]

/-! ## The restart trace from `LogFilter.lean`, now tagged with runs

Run 1's monitor writes its late output and exit after run 4 was launched. -/

def restartTrace : List Row :=
  [⟨1, 0, some 1⟩, ⟨2, 0, some 1⟩, ⟨3, 0, some 1⟩,   -- run 1
   ⟨4, 0, some 4⟩,                                   -- relaunch: run 4
   ⟨5, 0, some 1⟩, ⟨6, 0, some 1⟩,                   -- run 1's late output and exit
   ⟨7, 0, some 4⟩, ⟨8, 0, some 4⟩]                   -- run 4

example : (filter (seed restartTrace) restartTrace).2 =
    [⟨4, 0, some 4⟩, ⟨7, 0, some 4⟩, ⟨8, 0, some 4⟩] := by decide

end Candle.RunFilter
