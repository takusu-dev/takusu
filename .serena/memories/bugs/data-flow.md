# Data Flow Bugs

## Fixed
- [x] `build_planner()` silently dropped `depends` as `vec![]` → now parses JSON and resolves indices
- [x] No cycle detection anywhere → added DFS cycle detection in build_planner, update_task, replace_task

## Remaining

### [FIXED] `plan_in_range()` dependency index remap missing
`takusu-core/src/lib.rs:403-407` — sub-planner clones tasks but `depends` references original indices.
When pinned tasks are excluded, indices become wrong (dangling, self-dep). Affects reschedule range mode.
`plan_partial` path is not affected (uses full planner).

### [FIXED] `takusu-habit` never wired into planner
`HabitStore`/`HabitGenerator` implemented and tested but never called from `TakusuApp`.
Habit-generated tasks never appear in schedules. `generator.rs:238` hardcodes `depends: vec![]`.
Habit tasks are now persisted as `TaskRow` with `habit_id`, deduplicated by `(habit_id, date)`.
Non-pending habit tasks are preserved; stale pending ones are cleaned up.

## Note: single start_time per habit
Current `Habit` model has one `start_time` (TimeOfDay). Dedup key is `(habit_id, date)`.
To support multi-occurrence-per-day habits (e.g. medicine morning+evening), the model needs multiple start_times and a different dedup key like `(habit_id, date, time)`.

### [FIXED] CLI `task create`/`replace` drop `parallelizable`, `allows_parallel`
`takusu-cli/src/main.rs:616-617` — always `None`, defaults to `false`. No flags exposed.
Update subcommand has them, create/replace don't.

### [FIXED] `reschedule()` uses `Point(0)` not current time
`app.rs:455` — `generate_schedule()` uses `Timestamp::now()` but `reschedule()` uses zero.
May allow tasks in the past.
