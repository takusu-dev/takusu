#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


CONFIGS = {
    "stress_30d": (150, 30, 0.28, 0.25, 17),
    "stress_30d_dense": (150, 30, 0.55, 0.55, 23),
    "stress_30d_mixed": (150, 30, 0.38, 0.38, 31),
    "stress_90d": (450, 90, 0.25, 0.20, 47),
}


def next_random(state):
    state[0] = (1664525 * state[0] + 1013904223) & 0xFFFFFFFF
    return state[0]


def make_fixture(name):
    task_count, days, dependency_probability, conflict_probability, seed = CONFIGS[name]
    state = [seed]
    slots_per_day = 288
    horizon = days * slots_per_day
    tasks = []

    for task_id in range(task_count):
        random = next_random(state)
        day = task_id * days // task_count
        duration = 4 + random % 28
        deadline = min(
            horizon,
            max(
                day * slots_per_day + 180,
                day * slots_per_day + 240 + next_random(state) % 720,
            ),
        )
        deadline = max(deadline, duration + 24)
        fixed = task_id % 10 == 0
        if fixed:
            start = day * slots_per_day + 24 + (task_id // 10 % 8) * 24
            start = min(start, deadline - duration)
        elif next_random(state) % 1000 < 420:
            start = day * slots_per_day + next_random(state) % 144
            start = min(start, deadline - duration)
        else:
            start = None

        depends = []
        if task_id > 0 and next_random(state) / 2**32 < dependency_probability:
            depends.append(task_id - 1)
        if task_id > 3 and next_random(state) / 2**32 < dependency_probability * 0.65:
            depends.append(task_id - 1 - next_random(state) % min(task_id, 16))
        depends = sorted(set(depends))

        if next_random(state) / 2**32 < conflict_probability:
            conflict_day = (task_id // 12) % days
            deadline = min(horizon, max(deadline, conflict_day * slots_per_day + 360))
            if start is not None:
                start = min(start, deadline - duration)

        tasks.append(
            {
                "start": start,
                "end": deadline,
                "avg": duration,
                "sigma": 1 + next_random(state) % max(2, duration // 3),
                "depends": depends,
                "parallelizable": next_random(state) % 100 < 22,
                "allows_parallel": next_random(state) % 100 < 16,
                "abandonability": round((next_random(state) % 100) / 100, 2),
                "fixed": fixed,
                "habit_group": task_id % 24 if task_id % 5 != 0 else None,
            }
        )

    return {
        "name": name,
        "now": 0,
        "horizon": horizon,
        "sleep": {"day_start": 0, "start": 264, "end": 360, "enabled": True},
        "tasks": tasks,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("names", nargs="*", choices=sorted(CONFIGS), default=None)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for name in args.names or sorted(CONFIGS):
        path = args.output_dir / f"{name}.json"
        path.write_text(json.dumps(make_fixture(name), indent=2) + "\n")
        print(f"{path}: {len(make_fixture(name)['tasks'])} tasks")


if __name__ == "__main__":
    main()
