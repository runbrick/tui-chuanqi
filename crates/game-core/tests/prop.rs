use game_core::GameState;
use game_data::{Class, GameData, Gender, Range};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]
    #[test]
    fn random_stats_do_not_panic(level in 1u16..40, atk in 0i32..80, seed in any::<u16>()) {
        let data = GameData::load(game_core::repo_data_dir()).unwrap();
        let mut state = GameState::new("随", Class::Warrior, Gender::Male, false, &data, u64::from(seed));
        state.level = level;
        state.base.attack = Range::new(0, atk);
        state.base.max_hp = 50 + i32::from(level) * 5;
        state.hp = state.base.max_hp;
        state.invalidate_derived();
        for _ in 0..200 {
            let _ = state.tick(&data);
            prop_assert!(state.hp >= 0);
            prop_assert!(state.gold < u64::MAX / 4);
        }
    }
}
