use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use takusu_core::{NormalDist, Planner, Point, SleepConfig, Task};

fn generate_tasks(rng: &mut impl Rng, count: usize) -> Planner {
    let mut planner = Planner::new(Point(0), SleepConfig::disabled());

    for i in 0..count {
        let start_slot = rng.gen_range(0..=500);
        let deadline_slot = start_slot + rng.gen_range(20..400);
        let avg = rng.gen_range(1..=20);
        let sigma = rng.gen_range(0..=10);

        let mut depends = Vec::new();
        if i >= 2 {
            let dep_count = rng.gen_range(0..=2).min(i);
            let mut possible: Vec<usize> = (0..i).collect();
            for _ in 0..dep_count {
                if possible.is_empty() {
                    break;
                }
                let idx = rng.gen_range(0..possible.len());
                depends.push(possible.remove(idx));
            }
        }

        planner
            .add(Task {
                id: 0,
                start: Some(Point(start_slot as i64)),
                end: Point(deadline_slot as i64),
                cost_estimate: NormalDist::new(avg as u64, sigma as u64),
                depends,
                parallelizable: rng.gen_bool(0.2),
                allows_parallel: rng.gen_bool(0.2),
                abandonability: rng.r#gen::<f64>(),
            })
            .unwrap();
    }

    planner
}

fn bench_plan_25(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    let planner = generate_tasks(&mut rng, 25);

    let mut group = c.benchmark_group("plan");
    group.sample_size(10);

    group.bench_function("plan 25 tasks", |b| {
        b.iter(|| {
            let plan = planner.plan();
            black_box(plan);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_plan_25);
criterion_main!(benches);
