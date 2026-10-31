use std::collections::{BTreeSet, HashMap};
use std::ops::RangeInclusive;

use c4::board::{Board, Geometry, Grid, format_moves};
use c4::dataset::sample;
use c4::random::SplitMix;

#[path = "common/oracle.rs"]
pub mod oracle;
use oracle::{Cells, Partial, grid_wins};

const SEEDS: u64 = 2_000;
const STONES: RangeInclusive<usize> = 1..=20;
const SMALL_SEEDS: u64 = 5_000;
const SMALL_STONES: RangeInclusive<usize> = 1..=4;

type Key = (Vec<(i64, i64)>, Vec<(i64, i64)>);

fn child_of<G: Geometry>(position: &Partial, mover: usize, column: u32) -> Option<Partial> {
    let cell = position.cell(column);
    if cell.1 == i64::from(G::ROWS) || grid_wins(&position.stones[mover], cell) {
        return None;
    }
    let mut next = position.clone();
    next.place(mover, column);
    (!has_winning_cell::<G>(&next.stones[1 - mover], &next.heights)).then_some(next)
}

fn has_winning_cell<G: Geometry>(own: &Cells, heights: &HashMap<i64, i64>) -> bool {
    (0..G::COLUMNS).any(|column| {
        let row = heights.get(&i64::from(column)).copied().unwrap_or(0);
        row < i64::from(G::ROWS) && grid_wins(own, (i64::from(column), row))
    })
}

fn sorted(cells: impl Iterator<Item = (i64, i64)>) -> Vec<(i64, i64)> {
    let mut list: Vec<(i64, i64)> = cells.collect();
    list.sort_unstable();
    list
}

fn canonical<G: Geometry>(stones: &[Cells; 2]) -> Key {
    let plain = (
        sorted(stones[0].iter().copied()),
        sorted(stones[1].iter().copied()),
    );
    let mirror = |cells: &Cells| {
        sorted(
            cells
                .iter()
                .map(|&(column, row)| (i64::from(G::COLUMNS) - 1 - column, row)),
        )
    };
    plain.min((mirror(&stones[0]), mirror(&stones[1])))
}

fn key_of<G: Geometry>(columns: &[u32]) -> Key {
    let mut position = Partial::default();
    for (step, &column) in columns.iter().enumerate() {
        position.place(step % 2, column);
    }
    canonical::<G>(&position.stones)
}

fn check_sample<G: Geometry>(columns: &[u32], board: &Board<G>, stones: usize) {
    assert_eq!(
        Board::<G>::from_moves(&format_moves(columns)).map(|replayed| replayed.key()),
        Some(board.key())
    );
    assert_eq!(columns.len(), stones);
    let mut position = Partial::default();
    for (step, &column) in columns.iter().enumerate() {
        let mover = step % 2;
        let cell = position.cell(column);
        assert!(
            !grid_wins(&position.stones[mover], cell),
            "move {step} wins"
        );
        position.place(mover, column);
        assert!(
            !has_winning_cell::<G>(&position.stones[1 - mover], &position.heights),
            "move {step} allows an immediate win"
        );
    }
}

fn check_seed<G: Geometry>(seed: u64) -> usize {
    let mut rng = SplitMix::new(seed);
    let mut found = 0_usize;
    for stones in STONES {
        if let Some(position) = sample::<G>(&mut rng, stones) {
            check_sample(&position.columns, &position.board, stones);
            found += 1;
        }
    }
    found
}

fn check_samples<G: Geometry>() {
    let found: usize = (0..SEEDS).map(check_seed::<G>).sum();
    assert!(found > 0);
}

fn keys_for_seed<G: Geometry>(seed: u64, sampled: &mut BTreeSet<Key>) {
    let mut rng = SplitMix::new(seed);
    for stones in SMALL_STONES {
        if let Some(position) = sample::<G>(&mut rng, stones) {
            sampled.insert(key_of::<G>(&position.columns));
        }
    }
}

fn extend<G: Geometry>(position: &Partial, placed: usize, depth: usize, found: &mut BTreeSet<Key>) {
    if placed > 0 {
        found.insert(canonical::<G>(&position.stones));
    }
    if placed == depth {
        return;
    }
    for column in 0..G::COLUMNS {
        if let Some(child) = child_of::<G>(position, placed % 2, column) {
            extend::<G>(&child, placed + 1, depth, found);
        }
    }
}

fn reachable<G: Geometry>(depth: usize) -> BTreeSet<Key> {
    let mut found = BTreeSet::new();
    extend::<G>(&Partial::default(), 0, depth, &mut found);
    found
}

fn complete<G: Geometry>() {
    let expected = reachable::<G>(*SMALL_STONES.end());
    let mut sampled = BTreeSet::new();
    for seed in 0..SMALL_SEEDS {
        keys_for_seed::<G>(seed, &mut sampled);
    }
    assert_eq!(sampled, expected);
}

#[test]
fn samples_hold_rules_narrow() {
    check_samples::<Grid<7, 6, u64>>();
}

#[test]
fn samples_hold_rules_wide() {
    check_samples::<Grid<7, 6, u128>>();
}

#[test]
fn complete_small_narrow() {
    complete::<Grid<4, 4, u64>>();
}

#[test]
fn complete_small_wide() {
    complete::<Grid<4, 4, u128>>();
}
