import Candle.LogFilter

/-!
# A corrected `LatestExecutionLogFilter`, proved against the spec

The defect in `LogFilter.lean` is that `reported_start_result : bool` is not
tied to a position in the log: the pre-scan in `check_latest_launch_status` sets
it from rows *after* a stale exit, and a replayed older `process_started` sets it
for the newer launch. The fix records *which* row reported the start result:

    struct LaunchStatus { start_log_id: i64, start_result_id: Option<i64> }

and only accepts a start result whose id is past `start_log_id`. An `exited` row
is hidden iff no start result lies between the launch marker and it.

Main results:
* `batchFix_eq_spec`   : batch mode (check + filter on the same rows) returns
                         exactly the rows of the latest launch, minus stale exits.
* `streamFix_eq_spec`  : streaming mode returns exactly the non-stale rows at or
                         after some launch marker.
* `stream_orig_eq_fix` : the *current* code agrees with the fix in streaming
                         mode, so the defect is confined to batch/replay use.
-/

namespace Candle.LogFilter

/-! ## Basic facts about chronological logs -/

theorem chrono_append {a b : List Log} :
    Chrono (a ++ b) ↔ Chrono a ∧ Chrono b ∧ ∀ x ∈ a, ∀ y ∈ b, x.id < y.id := by
  simp [Chrono, List.pairwise_append]

theorem chrono_id_inj {ls : List Log} (h : Chrono ls) {x y : Log}
    (hx : x ∈ ls) (hy : y ∈ ls) (hid : x.id = y.id) : x = y := by
  induction ls with
  | nil => simp at hx
  | cons a t ih =>
    simp only [Chrono, List.pairwise_cons] at h
    rcases List.mem_cons.1 hx with rfl | hx' <;> rcases List.mem_cons.1 hy with rfl | hy'
    · rfl
    · exact absurd hid (Nat.ne_of_lt (h.1 _ hy'))
    · exact absurd hid.symm (Nat.ne_of_lt (h.1 _ hx'))
    · exact ih h.2 hx' hy'

/-! ## The fixed filter -/

structure StatusF where
  startId : Nat
  resultId : Option Nat
  deriving DecidableEq, Repr

/-- Record a start result, but only the first one past the launch marker. -/
def StatusF.noteResult (st : StatusF) (l : Log) : StatusF :=
  if isStartResult l.ty ∧ st.resultId = none ∧ st.startId < l.id then
    { st with resultId := some l.id } else st

def checkStepF (s : Option StatusF) (l : Log) : Option StatusF :=
  if l.ty = .startInitiated then some ⟨l.id, none⟩
  else s.map (·.noteResult l)

def checkF (ls : List Log) : Option StatusF := ls.foldl checkStepF none

/-- Is `l` an exit that precedes this launch's start result? -/
def StatusF.hidesExit (st : StatusF) (l : Log) : Bool :=
  l.ty == .exited && (match st.resultId with | none => true | some f => decide (l.id < f))

def filterStepF (b : Behavior) (s : Option StatusF) (l : Log) : Option StatusF × Bool :=
  let s1 :=
    if l.ty = .startInitiated then
      match s with
      | none => some ⟨l.id, none⟩
      | some st => if l.id > st.startId then some ⟨l.id, none⟩ else s
    else s
  let incl :=
    match s1 with
    | some st => !st.hidesExit l && decide (l.id ≥ st.startId)
    | none => b == .showLogsFromPreviousLaunch
  (s1.map (·.noteResult l), incl)

def filterF (b : Behavior) : Option StatusF → List Log → Option StatusF × List Log
  | s, [] => (s, [])
  | s, l :: ls =>
    let (s', inc) := filterStepF b s l
    let (s'', out) := filterF b s' ls
    (s'', if inc then l :: out else out)

def batchF (b : Behavior) (ls : List Log) : List Log := (filterF b (checkF ls) ls).2
def streamAfterF (b : Behavior) (initial rest : List Log) : List Log :=
  (filterF b (checkF initial) rest).2

/-- The fix still behaves as intended on the concrete restart trace. -/
example : batchF .onlyShowAfterRecentLaunch restartTrace =
    [⟨4, .startInitiated⟩, ⟨6, .started⟩, ⟨7, .stdout⟩] := by decide

/-! ## What the state means

`Inv seen s`: `s` is the correct summary of the rows `seen` so far. -/

def Inv (seen : List Log) : Option StatusF → Prop
  | none => ∀ x ∈ seen, x.ty ≠ .startInitiated
  | some ⟨B, res⟩ =>
    ⟨B, .startInitiated⟩ ∈ seen ∧
    (∀ x ∈ seen, x.ty = .startInitiated → x.id ≤ B) ∧
    (match res with
     | none => ∀ x ∈ seen, isStartResult x.ty = true → x.id < B
     | some f => B < f ∧ (∃ x ∈ seen, isStartResult x.ty = true ∧ x.id = f) ∧
                 ∀ x ∈ seen, isStartResult x.ty = true → B < x.id → f ≤ x.id)

theorem isStartResult_ne_start {t : LogType} (h : isStartResult t = true) :
    t ≠ .startInitiated := by cases t <;> simp_all [isStartResult]

theorem isStartResult_ne_exited {t : LogType} (h : isStartResult t = true) :
    t ≠ .exited := by cases t <;> simp_all [isStartResult]

/-- One step of the pre-scan preserves the invariant when the new row is newer
than everything seen. -/
theorem inv_checkStep {seen : List Log} {s : Option StatusF} {l : Log}
    (hinv : Inv seen s) (hnew : ∀ x ∈ seen, x.id < l.id) :
    Inv (seen ++ [l]) (checkStepF s l) := by
  unfold checkStepF
  by_cases hs : l.ty = .startInitiated
  · rw [if_pos hs]; simp only [Inv]
    refine ⟨?_, ?_, ?_⟩
    · have : (⟨l.id, .startInitiated⟩ : Log) = l := by cases l; simp_all
      exact List.mem_append_right _ (by simp [this])
    · intro x hx _
      rcases List.mem_append.1 hx with hx | hx
      · exact Nat.le_of_lt (hnew x hx)
      · simp at hx; subst hx; exact Nat.le_refl _
    · intro x hx hr
      rcases List.mem_append.1 hx with hx | hx
      · exact hnew x hx
      · simp at hx; subst hx; exact absurd hs (isStartResult_ne_start hr)
  · rw [if_neg hs]
    cases s with
    | none =>
      simp only [Option.map, Inv] at hinv ⊢
      intro x hx
      rcases List.mem_append.1 hx with hx | hx
      · exact hinv x hx
      · simp at hx; subst hx; exact hs
    | some st =>
      obtain ⟨B, res⟩ := st
      simp only [Inv] at hinv
      obtain ⟨hB, hstarts, hres⟩ := hinv
      have hBl : B < l.id := hnew _ hB
      show Inv (seen ++ [l]) (some (StatusF.noteResult ⟨B, res⟩ l))
      dsimp only [StatusF.noteResult]
      by_cases hc : isStartResult l.ty = true ∧ res = none ∧ B < l.id
      · rw [if_pos hc]
        obtain ⟨hr, rfl, _⟩ := hc
        simp only [Inv]
        refine ⟨List.mem_append_left _ hB, ?_, hBl, ⟨l, by simp, hr, rfl⟩, ?_⟩
        · intro x hx hxs
          rcases List.mem_append.1 hx with hx | hx
          · exact hstarts x hx hxs
          · simp at hx; subst hx; exact absurd hxs hs
        · intro x hx hxr hBx
          rcases List.mem_append.1 hx with hx | hx
          · exact absurd (hres x hx hxr) (Nat.not_lt.2 (Nat.le_of_lt hBx))
          · simp at hx; subst hx; exact Nat.le_refl _
      · rw [if_neg hc]
        simp only [Inv]
        refine ⟨List.mem_append_left _ hB, ?_, ?_⟩
        · intro x hx hxs
          rcases List.mem_append.1 hx with hx | hx
          · exact hstarts x hx hxs
          · simp at hx; subst hx; exact absurd hxs hs
        · cases res with
          | none =>
            intro x hx hxr
            rcases List.mem_append.1 hx with hx | hx
            · exact hres x hx hxr
            · simp at hx; subst hx; exact absurd ⟨hxr, rfl, hBl⟩ hc
          | some f =>
            obtain ⟨hBf, hex, hmin⟩ := hres
            refine ⟨hBf, ?_, ?_⟩
            · obtain ⟨x, hx, h1, h2⟩ := hex; exact ⟨x, List.mem_append_left _ hx, h1, h2⟩
            · intro x hx hxr hBx
              rcases List.mem_append.1 hx with hx | hx
              · exact hmin x hx hxr hBx
              · simp at hx; subst hx
                obtain ⟨y, hy, _, rfl⟩ := hex
                exact Nat.le_of_lt (hnew y hy)

theorem inv_check_append (pre rest : List Log) (s : Option StatusF)
    (hinv : Inv pre s) (hc : Chrono (pre ++ rest)) :
    Inv (pre ++ rest) (rest.foldl checkStepF s) := by
  induction rest generalizing pre s with
  | nil => simpa using hinv
  | cons l t ih =>
    have hnew : ∀ x ∈ pre, x.id < l.id := by
      intro x hx; exact (chrono_append.1 hc).2.2 x hx l (by simp)
    have h1 := inv_checkStep hinv hnew
    have := ih (pre ++ [l]) (checkStepF s l) h1 (by simpa using hc)
    simpa using this

theorem inv_check {ls : List Log} (hc : Chrono ls) : Inv ls (checkF ls) := by
  have := inv_check_append [] ls none (by simp [Inv]) (by simpa using hc)
  simpa [checkF] using this

/-! ## The spec -/

/-- Rows at or after the latest launch marker in `ls`. -/
def lastStart (ls : List Log) : Option Nat := (checkF ls).map (·.startId)

/-- Batch spec: the latest launch's rows, minus stale exits. With no launch
marker, `ShowLogsFromPreviousLaunch` shows everything and the other behavior
shows nothing. -/
def batchSpec (b : Behavior) (ls : List Log) (r : Log) : Bool :=
  match lastStart ls with
  | some B => decide (B ≤ r.id) && !decide (StaleExit ls r)
  | none => b == .showLogsFromPreviousLaunch

/-- Streaming spec: a row is shown iff it is not a stale exit and (some launch
marker at or before it exists, or we show logs without a launch). -/
def streamSpec (b : Behavior) (seen : List Log) (r : Log) : Bool :=
  if seen.any (fun s => s.ty = .startInitiated ∧ s.id ≤ r.id)
  then !decide (StaleExit seen r)
  else b == .showLogsFromPreviousLaunch

/-! ## Batch correctness -/

theorem hidesExit_iff (st : StatusF) (l : Log) :
    st.hidesExit l = true ↔ l.ty = .exited ∧ ∀ f, st.resultId = some f → l.id < f := by
  obtain ⟨B, res⟩ := st
  cases res <;> simp [StatusF.hidesExit]

/-- The inclusion decision for a fixed state `some st`. -/
theorem stale_iff {ls : List Log} (hc : Chrono ls) {B : Nat} {res : Option Nat}
    (hinv : Inv ls (some ⟨B, res⟩)) {r : Log} (hr : r ∈ ls) (hBr : B ≤ r.id) :
    StaleExit ls r ↔ (⟨B, res⟩ : StatusF).hidesExit r = true := by
  obtain ⟨hB, hstarts, hres⟩ := hinv
  rw [hidesExit_iff]
  constructor
  · rintro ⟨hex, s, hs, hss, hsr, hnone⟩
    refine ⟨hex, ?_⟩
    intro f hf
    cases res with
    | none => cases hf
    | some f' =>
      simp only [Option.some.injEq] at hf; subst hf
      obtain ⟨hBf, ⟨x, hx, hxr, hxf⟩, _⟩ := hres
      have hsB := hstarts s hs hss
      rcases Nat.lt_or_ge r.id f' with h | h
      · exact h
      · exfalso
        have hne : x.id ≠ r.id := by
          intro heq
          have := chrono_id_inj hc hx hr heq
          subst this; exact isStartResult_ne_exited hxr hex
        exact hnone x hx hxr ⟨by omega, by omega⟩
  · rintro ⟨hex, hres'⟩
    have hBr' : B < r.id := by
      rcases Nat.lt_or_eq_of_le hBr with h | h
      · exact h
      · have := chrono_id_inj hc hB hr h
        rw [← this] at hex; cases hex
    refine ⟨hex, ⟨B, .startInitiated⟩, hB, rfl, hBr', ?_⟩
    rintro x hx hxr ⟨h1, h2⟩
    simp only at h1
    cases res with
    | none => have := hres x hx hxr; omega
    | some f =>
      obtain ⟨_, _, hmin⟩ := hres
      have := hmin x hx hxr h1
      have := hres' f rfl
      omega

/-- Turn `stale_iff` into an equation between the two Boolean decisions. -/
theorem hides_eq_stale {ls : List Log} (hc : Chrono ls) {B : Nat} {res : Option Nat}
    (hinv : Inv ls (some ⟨B, res⟩)) {r : Log} (hr : r ∈ ls) (hBr : B ≤ r.id) :
    (⟨B, res⟩ : StatusF).hidesExit r = decide (StaleExit ls r) := by
  have key := stale_iff hc hinv hr hBr
  by_cases hst : StaleExit ls r
  · simp [hst, key.1 hst]
  · cases h : (⟨B, res⟩ : StatusF).hidesExit r
    · simp [hst]
    · exact absurd (key.2 h) hst

/-- During the batch pass over `ls`, the pre-scanned state never changes. -/
theorem filterStepF_const {b : Behavior} {ls : List Log} (hc : Chrono ls)
    {st : StatusF} (hinv : Inv ls (some st)) {l : Log} (hl : l ∈ ls) :
    (filterStepF b (some st) l).1 = some st := by
  obtain ⟨B, res⟩ := st
  obtain ⟨hB, hstarts, hres⟩ := hinv
  simp only [filterStepF]
  have hs1 : (if l.ty = .startInitiated then
      (if l.id > B then some (⟨l.id, none⟩ : StatusF) else some ⟨B, res⟩)
      else some ⟨B, res⟩) = some ⟨B, res⟩ := by
    split
    · rename_i h; have := hstarts l hl h; simp; omega
    · rfl
  simp only [hs1, Option.map, StatusF.noteResult]
  split
  · rename_i h
    obtain ⟨hr, rfl, hBl⟩ := h
    exact absurd (hres l hl hr) (by omega)
  · rfl

theorem filterF_const {b : Behavior} {ls : List Log} (hc : Chrono ls)
    {st : StatusF} (hinv : Inv ls (some st)) :
    ∀ t, (∀ l ∈ t, l ∈ ls) →
      (filterF b (some st) t).2 = t.filter (fun l => (filterStepF b (some st) l).2) := by
  intro t ht
  induction t with
  | nil => rfl
  | cons l t ih =>
    have hl : l ∈ ls := ht l (by simp)
    have hconst := filterStepF_const (b := b) hc hinv hl
    have e : filterStepF b (some st) l = (some st, (filterStepF b (some st) l).2) :=
      Prod.ext hconst rfl
    rw [filterF.eq_2, e]
    simp only
    rw [ih (fun x hx => ht x (by simp [hx]))]
    by_cases h : (filterStepF b (some st) l).2 = true <;> simp [List.filter_cons, h]

theorem filterF_none {b : Behavior} {ls : List Log}
    (hinv : Inv ls none) :
    ∀ t, (∀ l ∈ t, l ∈ ls) →
      (filterF b none t).2 = t.filter (fun _ => b == .showLogsFromPreviousLaunch) := by
  intro t ht
  induction t with
  | nil => rfl
  | cons l t ih =>
    have hns : l.ty ≠ .startInitiated := hinv l (ht l (by simp))
    simp only [filterF, filterStepF, hns, if_false, Option.map]
    rw [ih (fun x hx => ht x (by simp [hx]))]
    by_cases h : (b == .showLogsFromPreviousLaunch) = true <;> simp [List.filter_cons, h]

/-- **Batch correctness of the fix.** For any chronological batch, check + filter
returns exactly the rows the spec allows. -/
theorem batchFix_eq_spec (b : Behavior) (ls : List Log) (hc : Chrono ls) :
    batchF b ls = ls.filter (batchSpec b ls) := by
  have hinv := inv_check hc
  unfold batchF batchSpec lastStart
  cases hs : checkF ls with
  | none =>
    rw [hs] at hinv
    simpa using filterF_none (b := b) hinv ls (fun _ h => h)
  | some st =>
    rw [hs] at hinv
    rw [filterF_const hc hinv ls (fun _ h => h)]
    apply List.filter_congr
    intro r hr
    obtain ⟨B, res⟩ := st
    have hconst := filterStepF_const (b := b) hc hinv hr
    simp only [Option.map]
    -- unfold the inclusion bit with the constant state
    have hstarts := hinv.2.1
    have hs1 : (if r.ty = .startInitiated then
        (if r.id > B then some (⟨r.id, none⟩ : StatusF) else some ⟨B, res⟩)
        else some ⟨B, res⟩) = some ⟨B, res⟩ := by
      split
      · rename_i h; have := hstarts r hr h; simp; omega
      · rfl
    simp only [filterStepF, hs1]
    by_cases hBr : B ≤ r.id
    · rw [hides_eq_stale hc hinv hr hBr]; simp [hBr]
    · simp [hBr]

/-! ## Streaming correctness -/

/-- In streaming mode each row is newer than everything seen, and one filter
step updates the state exactly as the pre-scan would. -/
theorem filterStepF_fst_eq_check {b : Behavior} {seen : List Log} {s : Option StatusF}
    (hinv : Inv seen s) {l : Log} (hnew : ∀ x ∈ seen, x.id < l.id) :
    (filterStepF b s l).1 = checkStepF s l := by
  cases s with
  | none =>
    by_cases hs : l.ty = .startInitiated <;> simp [filterStepF, checkStepF, hs, Option.map,
      StatusF.noteResult]
  | some st =>
    obtain ⟨B, res⟩ := st
    have hBl : B < l.id := hnew _ hinv.1
    by_cases hs : l.ty = .startInitiated
    · have hnr : ¬ isStartResult l.ty = true := fun h => isStartResult_ne_start h hs
      simp [filterStepF, checkStepF, hs, hBl, Option.map, StatusF.noteResult, hnr]
    · simp [filterStepF, checkStepF, hs]

theorem streamStep_correct {b : Behavior} {seen : List Log} {s : Option StatusF}
    (hinv : Inv seen s) {l : Log} (hc : Chrono (seen ++ [l])) :
    (filterStepF b s l).2 = streamSpec b (seen ++ [l]) l := by
  have hnew : ∀ x ∈ seen, x.id < l.id := fun x hx => (chrono_append.1 hc).2.2 x hx l (by simp)
  have hinv' := inv_checkStep hinv hnew
  have hl : l ∈ seen ++ [l] := by simp
  -- the state used for the decision is `s1`, which equals `checkStepF s l` up to
  -- the start-result bookkeeping; for an exit that bookkeeping is a no-op.
  unfold streamSpec
  cases s with
  | none =>
    by_cases hs : l.ty = .startInitiated
    · have hany : (seen ++ [l]).any (fun x => x.ty = .startInitiated ∧ x.id ≤ l.id) = true :=
        List.any_eq_true.2 ⟨l, hl, by simp [hs]⟩
      have hnst : ¬ StaleExit (seen ++ [l]) l := fun h => by rw [h.1] at hs; cases hs
      rw [hany, if_pos rfl]
      simp [filterStepF, hs, StatusF.hidesExit, hnst]
    · have hany : (seen ++ [l]).any (fun x => x.ty = .startInitiated ∧ x.id ≤ l.id) = false := by
        simp only [List.any_eq_false, decide_eq_true_eq, not_and]
        intro x hx hxs
        rcases List.mem_append.1 hx with hx | hx
        · exact absurd hxs (hinv x hx)
        · simp at hx; subst hx; exact absurd hxs hs
      rw [hany]
      simp [filterStepF, hs]
  | some st =>
    obtain ⟨B, res⟩ := st
    have hBl : B < l.id := hnew _ hinv.1
    have hany : (seen ++ [l]).any (fun x => x.ty = .startInitiated ∧ x.id ≤ l.id) = true :=
      List.any_eq_true.2 ⟨⟨B, .startInitiated⟩, List.mem_append_left _ hinv.1,
        by simp; omega⟩
    rw [hany, if_pos rfl]
    by_cases hs : l.ty = .startInitiated
    · have hnst : ¬ StaleExit (seen ++ [l]) l := fun h => by rw [h.1] at hs; cases hs
      simp [filterStepF, hs, hBl, StatusF.hidesExit, hnst]
    · -- s1 = some ⟨B,res⟩ and `checkStepF` agrees with it on exits
      have hdec : (filterStepF b (some ⟨B, res⟩) l).2 = (!(⟨B, res⟩ : StatusF).hidesExit l) := by
        simp [filterStepF, hs, Nat.le_of_lt hBl]
      rw [hdec]
      by_cases hex : l.ty = .exited
      · have hnr : ¬ isStartResult l.ty = true := fun h => isStartResult_ne_exited h hex
        have hsame : checkStepF (some ⟨B, res⟩) l = some ⟨B, res⟩ := by
          simp [checkStepF, hs, Option.map, StatusF.noteResult, hnr]
        rw [hsame] at hinv'
        rw [hides_eq_stale hc hinv' hl (Nat.le_of_lt hBl)]
      · have hnst : ¬ StaleExit (seen ++ [l]) l := fun h => hex h.1
        simp [StatusF.hidesExit, hex, hnst]

/-- The streaming spec for `r` only depends on rows older than `r`. -/
theorem streamSpec_extend {b : Behavior} {ls t : List Log} {r : Log}
    (ht : ∀ y ∈ t, r.id < y.id) :
    streamSpec b (ls ++ t) r = streamSpec b ls r := by
  have hany : (ls ++ t).any (fun s => s.ty = .startInitiated ∧ s.id ≤ r.id) =
      ls.any (fun s => s.ty = .startInitiated ∧ s.id ≤ r.id) := by
    have : t.any (fun s => s.ty = .startInitiated ∧ s.id ≤ r.id) = false := by
      simp only [List.any_eq_false, decide_eq_true_eq, not_and]
      intro y hy _; have := ht y hy; omega
    rw [List.any_append, this, Bool.or_false]
  have hst : StaleExit (ls ++ t) r ↔ StaleExit ls r := by
    unfold StaleExit
    constructor
    · rintro ⟨hex, s, hs, hss, hsr, hno⟩
      refine ⟨hex, s, ?_, hss, hsr, fun x hx => hno x (List.mem_append_left _ hx)⟩
      rcases List.mem_append.1 hs with hs | hs
      · exact hs
      · have := ht s hs; omega
    · rintro ⟨hex, s, hs, hss, hsr, hno⟩
      refine ⟨hex, s, List.mem_append_left _ hs, hss, hsr, ?_⟩
      intro x hx hxr ⟨h1, h2⟩
      rcases List.mem_append.1 hx with hx | hx
      · exact hno x hx hxr ⟨h1, h2⟩
      · have := ht x hx; omega
  unfold streamSpec
  rw [hany]
  simp only [hst]

/-- **Streaming correctness of the fix**: starting from a state that correctly
summarizes `seen` (e.g. `checkF initial`), each streamed row is shown exactly
when the streaming spec allows it. -/
theorem streamFix_eq_spec_gen (b : Behavior) (seen rest : List Log) (s : Option StatusF)
    (hinv : Inv seen s) (hc : Chrono (seen ++ rest)) :
    (filterF b s rest).2 = rest.filter (streamSpec b (seen ++ rest)) := by
  induction rest generalizing seen s with
  | nil => rfl
  | cons l t ih =>
    have hc' : Chrono ((seen ++ [l]) ++ t) := by simpa using hc
    have hc1 : Chrono (seen ++ [l]) := (chrono_append.1 hc').1
    have hlt : ∀ y ∈ t, l.id < y.id := fun y hy => (chrono_append.1 hc').2.2 l (by simp) y hy
    have hnew : ∀ x ∈ seen, x.id < l.id := fun x hx => (chrono_append.1 hc1).2.2 x hx l (by simp)
    have hstep := streamStep_correct (b := b) hinv hc1
    have hfst := filterStepF_fst_eq_check (b := b) hinv hnew
    have hinv' := inv_checkStep hinv hnew
    rw [← hfst] at hinv'
    have ih' := ih (seen ++ [l]) _ hinv' hc'
    have hspec : streamSpec b (seen ++ l :: t) l = streamSpec b (seen ++ [l]) l := by
      have := streamSpec_extend (b := b) (ls := seen ++ [l]) hlt; simpa using this
    simp only [filterF]
    rw [ih', List.filter_cons, hspec, ← hstep]
    simp

theorem streamFix_eq_spec (b : Behavior) (initial rest : List Log)
    (hc : Chrono (initial ++ rest)) :
    streamAfterF b initial rest = rest.filter (streamSpec b (initial ++ rest)) :=
  streamFix_eq_spec_gen b initial rest _ (inv_check (chrono_append.1 hc).1) hc

/-! ## The current code is correct in streaming mode

Abstracting the fixed state to the current one (`resultId.isSome` is
`reported_start_result`), the two filters take identical steps whenever each row
is newer than everything seen before it. -/

def absS : Option StatusF → Option Status :=
  Option.map (fun st => ⟨st.startId, st.resultId.isSome⟩)

theorem checkStep_abs {seen : List Log} {s : Option StatusF} {l : Log}
    (hinv : Inv seen s) (hnew : ∀ x ∈ seen, x.id < l.id) :
    checkStep (absS s) l = absS (checkStepF s l) := by
  cases s with
  | none => by_cases hs : l.ty = .startInitiated <;> simp [checkStep, checkStepF, absS, hs]
  | some st =>
    obtain ⟨B, res⟩ := st
    have hBl : B < l.id := hnew _ hinv.1
    by_cases hs : l.ty = .startInitiated
    · simp [checkStep, checkStepF, absS, hs]
    · by_cases hr : isStartResult l.ty = true
      · cases res <;> simp [checkStep, checkStepF, absS, hs, hr, StatusF.noteResult, hBl]
      · simp [checkStep, checkStepF, absS, hs, hr, StatusF.noteResult]

theorem check_abs_gen (pre rest : List Log) (s : Option StatusF)
    (hinv : Inv pre s) (hc : Chrono (pre ++ rest)) :
    rest.foldl checkStep (absS s) = absS (rest.foldl checkStepF s) := by
  induction rest generalizing pre s with
  | nil => rfl
  | cons l t ih =>
    have hnew : ∀ x ∈ pre, x.id < l.id := fun x hx => (chrono_append.1 hc).2.2 x hx l (by simp)
    simp only [List.foldl_cons]
    rw [checkStep_abs hinv hnew]
    exact ih (pre ++ [l]) _ (inv_checkStep hinv hnew) (by simpa using hc)

theorem check_abs {ls : List Log} (hc : Chrono ls) : check ls = absS (checkF ls) := by
  have := check_abs_gen [] ls none (by simp [Inv]) (by simpa using hc)
  simpa [check, checkF, absS] using this

theorem filterStep_abs {b : Behavior} {seen : List Log} {s : Option StatusF} {l : Log}
    (hinv : Inv seen s) (hnew : ∀ x ∈ seen, x.id < l.id) :
    filterStep b (absS s) l = (absS (filterStepF b s l).1, (filterStepF b s l).2) := by
  cases s with
  | none =>
    by_cases hs : l.ty = .startInitiated
    · have hr : ¬ isStartResult l.ty = true := fun h => isStartResult_ne_start h hs
      simp [filterStep, filterStepF, absS, hs, StatusF.noteResult, StatusF.hidesExit, isStartResult]
    · simp [filterStep, filterStepF, absS, hs]
  | some st =>
    obtain ⟨B, res⟩ := st
    have hBl : B < l.id := hnew _ hinv.1
    by_cases hs : l.ty = .startInitiated
    · have hr : ¬ isStartResult l.ty = true := fun h => isStartResult_ne_start h hs
      simp [filterStep, filterStepF, absS, hs, hBl, StatusF.noteResult, StatusF.hidesExit, isStartResult]
    · -- the recorded result (if any) is an older row, so `l.id < f` is false
      have hf : ∀ f, res = some f → f < l.id := by
        intro f hf; subst hf
        obtain ⟨y, hy, _, rfl⟩ := hinv.2.2.2.1
        exact hnew y hy
      by_cases hr : isStartResult l.ty = true
      · have hex : l.ty ≠ .exited := isStartResult_ne_exited hr
        cases res <;> simp [filterStep, filterStepF, absS, hs, hr, hex, hBl,
          StatusF.noteResult, StatusF.hidesExit]
      · cases res with
        | none =>
          rcases l with ⟨id, ty⟩
          cases ty <;> simp_all [filterStep, filterStepF, absS, StatusF.noteResult,
            StatusF.hidesExit, isStartResult]
        | some f =>
          have := hf f rfl
          simp [filterStep, filterStepF, absS, hs, hr, StatusF.noteResult,
            StatusF.hidesExit, Nat.not_lt.2 (Nat.le_of_lt this)]

theorem filter_abs_gen (b : Behavior) (seen rest : List Log) (s : Option StatusF)
    (hinv : Inv seen s) (hc : Chrono (seen ++ rest)) :
    (filter b (absS s) rest).2 = (filterF b s rest).2 := by
  induction rest generalizing seen s with
  | nil => rfl
  | cons l t ih =>
    have hnew : ∀ x ∈ seen, x.id < l.id := fun x hx => (chrono_append.1 hc).2.2 x hx l (by simp)
    have hfst := filterStepF_fst_eq_check (b := b) hinv hnew
    have hinv' := inv_checkStep hinv hnew
    rw [← hfst] at hinv'
    simp only [filter, filterF]
    rw [filterStep_abs hinv hnew]
    simp only
    rw [ih (seen ++ [l]) _ hinv' (by simpa using hc)]

/-- **The current code is correct in streaming mode**: it agrees with the fix,
hence (by `streamFix_eq_spec`) with the spec. The defect is confined to batch
use (`logs`, `wait-for-log`'s initial scan, `watch`'s first print). -/
theorem stream_orig_eq_fix (b : Behavior) (initial rest : List Log)
    (hc : Chrono (initial ++ rest)) :
    streamAfter b initial rest = streamAfterF b initial rest := by
  have hc1 := (chrono_append.1 hc).1
  unfold streamAfter streamAfterF
  rw [check_abs hc1]
  exact filter_abs_gen b initial rest _ (inv_check hc1) hc

theorem stream_orig_eq_spec (b : Behavior) (initial rest : List Log)
    (hc : Chrono (initial ++ rest)) :
    streamAfter b initial rest = rest.filter (streamSpec b (initial ++ rest)) := by
  rw [stream_orig_eq_fix b initial rest hc, streamFix_eq_spec b initial rest hc]

/-- …whereas in batch mode the current code violates the spec. -/
theorem batch_orig_ne_spec :
    batch .onlyShowAfterRecentLaunch restartTrace ≠
      restartTrace.filter (batchSpec .onlyShowAfterRecentLaunch restartTrace) := by decide

end Candle.LogFilter
