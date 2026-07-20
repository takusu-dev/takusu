#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

from ortools.sat.python import cp_model


def can_overlap(a, b):
    return (a["allows_parallel"] and b["parallelizable"]) or (
        b["allows_parallel"] and a["parallelizable"]
    )


def solve(path, time_limit, workers):
    fixture = json.loads(Path(path).read_text())
    tasks = [
        {**task, "id": task.get("id", index)}
        for index, task in enumerate(fixture["tasks"])
    ]
    horizon = fixture.get("horizon")
    if horizon is None:
        horizon = max(task["end"] for task in tasks) + sum(
            task.get("duration", task.get("avg", 0)) for task in tasks
        )
    model = cp_model.CpModel()
    starts = {}
    ends = {}
    intervals = {}

    for index, task in enumerate(tasks):
        task_id = task.get("id", index)
        duration = task["duration"] if "duration" in task else task["avg"]
        earliest_start = task.get("start") or 0
        deadline = task["deadline"] if "deadline" in task else task["end"]
        start = model.new_int_var(0, horizon - duration, f"start_{task_id}")
        end = model.new_int_var(duration, horizon, f"end_{task_id}")
        model.add(end == start + duration)
        if task["fixed"]:
            model.add(start == earliest_start)
        else:
            model.add(start >= earliest_start)
        starts[task_id] = start
        ends[task_id] = end
        intervals[task_id] = model.new_interval_var(
            start, duration, end, f"interval_{task_id}"
        )

    for task in tasks:
        for dependency in task["depends"]:
            model.add(starts[task["id"]] >= ends[dependency])

    for index, left in enumerate(tasks):
        for right in tasks[index + 1 :]:
            if can_overlap(left, right):
                continue
            before = model.new_bool_var(f"before_{left['id']}_{right['id']}")
            model.add(ends[left["id"]] <= starts[right["id"]]).only_enforce_if(before)
            model.add(ends[right["id"]] <= starts[left["id"]]).only_enforce_if(~before)

    tardiness = []
    for task in tasks:
        deadline = task.get("deadline", task["end"])
        late = model.new_int_var(0, horizon, f"late_{task['id']}")
        model.add_max_equality(late, [0, ends[task["id"]] - deadline])
        tardiness.append(late * 100)

    model.minimize(sum(tardiness) + sum(ends.values()))
    solver = cp_model.CpSolver()
    solver.parameters.max_time_in_seconds = time_limit
    solver.parameters.num_search_workers = workers
    status = solver.solve(model)
    status_name = solver.status_name(status)
    result = {
        "fixture": str(path),
        "status": status_name,
        "objective": solver.objective_value if status in (cp_model.OPTIMAL, cp_model.FEASIBLE) else None,
        "best_bound": solver.best_objective_bound if status in (cp_model.OPTIMAL, cp_model.FEASIBLE) else None,
        "wall_time_seconds": solver.wall_time,
        "num_conflicts": solver.num_conflicts,
        "num_branches": solver.num_branches,
        "schedule": [],
    }
    if status in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        result["schedule"] = [
            {
                "id": task["id"],
                "start": solver.value(starts[task["id"]]),
                "end": solver.value(ends[task["id"]]),
            }
            for task in tasks
        ]
    print(json.dumps(result, indent=2))
    return 0 if status in (cp_model.OPTIMAL, cp_model.FEASIBLE) else 1


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "fixture",
        nargs="?",
        default=Path(__file__).parents[2]
        / "crates"
        / "takusu-core"
        / "benches"
        / "fixtures"
        / "quality_small.json",
        type=Path,
    )
    parser.add_argument("--time-limit", type=float, default=10.0)
    parser.add_argument("--workers", type=int, default=1)
    args = parser.parse_args()
    raise SystemExit(solve(args.fixture, args.time_limit, args.workers))


if __name__ == "__main__":
    main()
