use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use takusu_ical::parse_ical;

const CALENDAR: &str = include_str!("fixtures/calendar.ics");

fn bench_parse_ical(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_ical");
    group.sample_size(100);

    group.bench_function("parse 50-event calendar", |b| {
        b.iter(|| {
            let tasks = parse_ical(CALENDAR).unwrap();
            black_box(tasks);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_parse_ical);
criterion_main!(benches);
