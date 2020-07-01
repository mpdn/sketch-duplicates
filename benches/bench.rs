use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaChaRng;

use sketch_duplicates::DuplicatesSketch;

fn sketch_benches(c: &mut Criterion) {
    let mut rng = ChaChaRng::seed_from_u64(42);

    let n_string = 10000;
    let n_bytes = 1000;

    let strings: Vec<_> = (0..n_string)
        .map(|_| {
            let len = rng.gen_range(0, n_bytes);
            let mut buf = vec![0; len];
            rng.fill(&mut buf[..]);
            buf
        })
        .collect();

    c.bench_function("insert", |b| {
        b.iter(|| {
            let mut sketch = DuplicatesSketch::new(4, 4096);
            strings.iter().for_each(|buf| sketch.insert(buf));
            black_box(sketch);
        })
    });
}

criterion_group!(benches, sketch_benches);
criterion_main!(benches);
