use criterion::{Criterion, criterion_group, criterion_main};
use rand::rngs::StdRng;
use rand::{Rng, RngExt, SeedableRng};
use std::hint::black_box;
use takusu_core::{NormalDist, Planner, Point, SleepConfig, Task};

fn generate_tasks(rng: &mut impl Rng, count: usize) -> Planner {
    let mut planner = Planner::new(Point(0), SleepConfig::disabled());

    for i in 0..count {
        let start_slot = rng.random_range(0..=500);
        let deadline_slot = start_slot + rng.random_range(20..400);
        let avg = rng.random_range(1..=20);
        let sigma = rng.random_range(0..=10);

        let mut depends = Vec::new();
        if i >= 2 {
            let dep_count = rng.random_range(0..=2).min(i);
            let mut possible: Vec<usize> = (0..i).collect();
            for _ in 0..dep_count {
                if possible.is_empty() {
                    break;
                }
                let idx = rng.random_range(0..possible.len());
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
                parallelizable: rng.random_bool(0.2),
                allows_parallel: rng.random_bool(0.2),
                abandonability: rng.random::<f64>(),
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

fn bench_plan_partial_25(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    let planner = generate_tasks(&mut rng, 25);

    let plan = planner.plan();
    let pinned: Vec<_> = plan.schedules.iter().take(5).cloned().collect();

    let mut group = c.benchmark_group("plan_partial");
    group.sample_size(10);

    group.bench_function("plan_partial 25 tasks (5 pinned)", |b| {
        b.iter(|| {
            let plan = planner.plan_partial(&pinned);
            black_box(plan);
        })
    });

    group.finish();
}

fn bench_plan_50(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(43);
    let planner = generate_tasks(&mut rng, 50);

    let mut group = c.benchmark_group("plan");
    group.sample_size(10);

    group.bench_function("plan 50 tasks", |b| {
        b.iter(|| {
            let plan = planner.plan();
            black_box(plan);
        })
    });

    group.finish();
}

fn bench_plan_100(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(44);
    let planner = generate_tasks(&mut rng, 100);

    let mut group = c.benchmark_group("plan");
    group.sample_size(10);

    group.bench_function("plan 100 tasks", |b| {
        b.iter(|| {
            let plan = planner.plan();
            black_box(plan);
        })
    });

    group.finish();
}

fn bench_evaluate(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    let planner = generate_tasks(&mut rng, 25);
    let plan = planner.plan();

    let mut group = c.benchmark_group("evaluate");
    group.sample_size(100);

    group.bench_function("evaluate 25-task plan", |b| {
        b.iter(|| {
            let score = takusu_core::evaluate::evaluate(&planner, &plan, 0.0, 1.0);
            black_box(score);
        })
    });

    group.finish();
}

fn bench_plan_range_25(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    let planner = generate_tasks(&mut rng, 25);
    let plan = planner.plan();
    let range = takusu_core::RescheduleRange {
        from: Point(200),
        until: Point(700),
    };

    let mut group = c.benchmark_group("plan_range");
    group.sample_size(10);

    group.bench_function("plan_in_range 25 tasks", |b| {
        b.iter(|| {
            let plan = planner.plan_in_range(&range, &plan.schedules, &[]);
            black_box(plan);
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_plan_25,
    bench_plan_partial_25,
    bench_plan_50,
    bench_plan_100,
    bench_evaluate,
    bench_plan_range_25,
);
criterion_main!(benches);
