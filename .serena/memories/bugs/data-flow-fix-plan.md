# Fix Plan for Data Flow Bugs (from `mem:bugs/data-flow`)

## Order: CRITICAL → MEDIUM → LOW

## Status: ALL DONE (2026-06-26)

### 1. Fix `plan_in_range()` dependency index remap
**File:** `crates/takusu-core/src/lib.rs:384-418`
**Approach:**
- After adding each task to sub-planner, build a remap table: `orig_index → sub_index` (only for non-pinned tasks)
- For each task in sub-planner, remap `depends`: filter out indices not in the remap table (pinned/out-of-scope), and remap remaining indices to sub-planner space
- Sub-planner is stored as `&mut Planner`, so iterate `sub_planner.tasks()` after clone-and-add loop and fix up depends
- Need to add `tasks_mut()` to `Planner` or restructure the loop
- Write a test: original has task A→B→C, pin B (middle), verify sub-planner A→C with no dangling index
- Write a test: original has chain A→B, pin A, verify sub-planner B has no dependency on missing A

### 2. Wire habits into planner
**File:** `crates/takusu-local-lib/src/app.rs`
**Approach:**
- In `build_planner()`, after loading task rows, also load habits from `storage.list_habits()`
- Process habits through `takusu_habit::HabitStore` and generate tasks for the schedule window
- Generated tasks get `depends: vec![]` (future: expose depends on `Habit`)
- Merge generated tasks with existing task rows
- Or: add a separate `load_habit_tasks()` method that generates and creates tasks via storage
- This is the bigger change; may want to defer to a separate PR

### 3. Add `--parallelizable` / `--allows_parallel` flags to CLI task create/replace
**File:** `crates/takusu-cli/src/main.rs:616-617`, `:708-709`
**Approach:**
- Check `TaskCommands::Update` for existing arg definitions (around line 200-203) and replicate for Create/Replace
- Add `#[arg(long)]` flags to `CreateArgs` and `ReplaceArgs` structs
- Update the `CreateTask` construction to use these flags instead of hardcoded `None`

### 4. Fix `reschedule()` to use current time
**File:** `crates/takusu-local-lib/src/app.rs:455`
**Approach:**
- Change `Point(0)` to `Point::from_timestamp(Timestamp::now(), 5)` matching `generate_schedule()`
- Verify reschedule tests still pass
