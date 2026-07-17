use std::hint::black_box;

mod common;

fn main() {
    let planner = common::build_planner();

    for _ in 0..20 {
        let plan = planner.plan();
        black_box(plan);
    }
}
