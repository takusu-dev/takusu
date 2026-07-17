use std::hint::black_box;
use std::time::Instant;

mod common;

fn main() {
    // Force a single SA chain so planner.plan() is deterministic across runs.
    // The benchmark itself measures evaluate() on the resulting fixed plan.
    unsafe { std::env::set_var("RAYON_NUM_THREADS", "1") };

    let planner = common::build_planner();

    // Generate a plan once. We then measure evaluate() on a fixed plan,
    // so any timing change is purely in the evaluation function, not the
    // stochastic SA search path.
    let plan = planner.plan();

    // warm up
    let _ = takusu_core::evaluate::evaluate(black_box(&planner), black_box(&plan), 1.0, 1.0);

    let runs = 100_000;
    let score = takusu_core::evaluate::evaluate(black_box(&planner), black_box(&plan), 1.0, 1.0);
    let start = Instant::now();
    for _ in 0..runs {
        black_box(takusu_core::evaluate::evaluate(
            black_box(&planner),
            black_box(&plan),
            1.0,
            1.0,
        ));
    }
    let elapsed = start.elapsed();

    println!("runs: {runs}");
    println!("score: {score:.6}");
    println!("total time: {:.3}s", elapsed.as_secs_f64());
    println!(
        "mean time per evaluate: {:.6} µs",
        elapsed.as_secs_f64() / runs as f64 * 1_000_000.0
    );
}
