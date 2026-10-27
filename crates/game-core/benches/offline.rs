use criterion::{black_box, criterion_group, criterion_main, Criterion};
use game_core::GameState;
use game_data::{Class, GameData, Gender};
use std::time::Duration;

fn bench_offline(c: &mut Criterion) {
    let data = GameData::load(game_core::repo_data_dir()).expect("data");
    let mut group = c.benchmark_group("offline");
    group.sample_size(10);
    group.bench_function("12h", |b| {
        b.iter(|| {
            let mut state = GameState::new("bench", Class::Warrior, Gender::Male, false, &data, 42);
            state.simulate_offline(&data, black_box(Duration::from_secs(12 * 3600)));
        });
    });
    group.finish();
}

criterion_group!(benches, bench_offline);
criterion_main!(benches);
