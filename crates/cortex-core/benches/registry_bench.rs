use criterion::{Criterion, black_box};

pub fn bench_registry(c: &mut Criterion) {
    c.bench_function("registry_register", |b| {
        b.iter(|| {
            // Placeholder — se llena cuando tengamos workers reales
            black_box(42)
        })
    });
}

criterion::criterion_group!(benches, bench_registry);
criterion::criterion_main!(benches);
