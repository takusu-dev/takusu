use takusu_core::{NormalDist, Planner, Point, SleepConfig, Task};

#[derive(serde::Deserialize)]
struct Fixture {
    now: i64,
    sleep: SleepFixture,
    tasks: Vec<TaskFixture>,
}

#[derive(serde::Deserialize)]
struct SleepFixture {
    day_start: i64,
    start: i64,
    end: i64,
    enabled: bool,
}

#[derive(serde::Deserialize)]
struct TaskFixture {
    start: Option<i64>,
    end: i64,
    avg: u64,
    sigma: u64,
    depends: Vec<usize>,
    parallelizable: bool,
    allows_parallel: bool,
    abandonability: f64,
    fixed: bool,
    habit_group: Option<usize>,
}

pub fn build_planner() -> Planner {
    let fixture: Fixture = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/benches/fixtures/realworld_tasks.json"
    )))
    .unwrap();

    let sleep = SleepConfig {
        day_start: fixture.sleep.day_start,
        start: fixture.sleep.start,
        end: fixture.sleep.end,
        enabled: fixture.sleep.enabled,
    };

    let mut planner = Planner::new(Point(fixture.now), sleep);
    for t in fixture.tasks {
        planner
            .add(Task {
                id: 0,
                start: t.start.map(Point),
                end: Point(t.end),
                cost_estimate: NormalDist::new(t.avg, t.sigma),
                depends: t.depends,
                parallelizable: t.parallelizable,
                allows_parallel: t.allows_parallel,
                abandonability: t.abandonability,
                fixed: t.fixed,
                habit_group: t.habit_group,
            })
            .unwrap();
    }
    planner
}
