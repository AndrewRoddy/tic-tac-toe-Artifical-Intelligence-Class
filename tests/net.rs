use std::error::Error;
use std::fs;
use std::path::Path;

use burn::backend::flex::FlexDevice;
use burn::backend::{Autodiff, Flex};
use c4::board::{B87, B98, Board, Cell, Geometry};
use c4::dataset::{self, ColumnLabelled};
use c4::net::{self, Forecast, Net, Schedule, Shape, Splits};
use c4::random::SplitMix;

const DATA: &str = "data/train_8x7.txt";
const SEED: u64 = 7;
const SMALL: Shape = Shape { width: 8, depth: 2 };
const HELD_OUT_SAMPLE: usize = 2_000;
const TRAIN_SAMPLE: usize = 2_000;
const WATCH_SAMPLE: usize = 300;
const EPOCHS: usize = 3;
const LEARNING_RATE: f64 = 0.01;
const HALF: f32 = 0.5;

#[derive(Clone, Copy)]
enum Part {
    Training,
    Held,
}

fn sample(part: Part, count: usize) -> Result<Vec<ColumnLabelled<B87>>, Box<dyn Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(DATA);
    let text = fs::read_to_string(path)?;
    let split = dataset::split(text.lines().collect());
    let lines = match part {
        Part::Training => split.train,
        Part::Held => split.held,
    };
    let examples = lines
        .into_iter()
        .take(count)
        .map(|line| dataset::parse_column_labelled::<B87>(line).ok_or("sample line parses"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(examples)
}

fn cell_names_moving_side<G: Geometry>() -> Result<(), Box<dyn Error>> {
    let board = Board::<G>::from_moves("1").ok_or("one move is legal")?;
    assert_eq!(board.cell(0, 0), Cell::Opponent);
    assert_eq!(board.cell(0, 1), Cell::Empty);
    assert_eq!(board.cell(1, 0), Cell::Empty);
    Ok(())
}

#[test]
fn cell_names_moving_side_u64() -> Result<(), Box<dyn Error>> {
    cell_names_moving_side::<B87>()
}

#[test]
fn cell_names_moving_side_u128() -> Result<(), Box<dyn Error>> {
    cell_names_moving_side::<B98>()
}

fn encode_planes<G: Geometry>() -> Result<(), Box<dyn Error>> {
    let board = Board::<G>::from_moves("1").ok_or("one move is legal")?;
    let mut values = Vec::new();
    net::encode(board, &mut values);
    let columns = usize::try_from(G::COLUMNS)?;
    let cells = usize::try_from(G::ROWS)? * columns;
    assert_eq!(values.len(), 3 * cells);
    let playable = 2 * cells;
    let above_stone = playable + columns;
    assert!(values[cells] > HALF, "opponent plane marks the stone");
    assert!(values[0] < HALF, "mover plane is empty");
    assert!(
        values[playable] < HALF,
        "filled bottom cell is not playable"
    );
    assert!(values[playable + 1] > HALF, "empty bottom cell is playable");
    assert!(
        values[above_stone] > HALF,
        "cell above the stone is playable"
    );
    assert!(
        values[above_stone + 1] < HALF,
        "cell above an empty cell is not playable"
    );
    Ok(())
}

#[test]
fn encode_planes_u64() -> Result<(), Box<dyn Error>> {
    encode_planes::<B87>()
}

#[test]
fn encode_planes_u128() -> Result<(), Box<dyn Error>> {
    encode_planes::<B98>()
}

#[test]
fn weights_round_trip() -> Result<(), Box<dyn Error>> {
    let device = FlexDevice;
    let model: Net<Flex> = Net::new(SMALL, &mut SplitMix::new(SEED), &device);
    let positions = ["", "1", "44", "4453"];
    let boards: Vec<Board<B87>> = positions
        .iter()
        .map(|moves| Board::<B87>::from_moves(moves).ok_or("legal moves"))
        .collect::<Result<_, _>>()?;
    let before: Vec<Option<Forecast>> = boards
        .iter()
        .map(|&board| model.forecast(board, &device))
        .collect();
    let bytes = net::to_weights(model).ok_or("weights serialise")?;
    let loaded: Net<Flex> = net::from_weights(SMALL, &bytes, &device).ok_or("weights load")?;
    for ((moves, board), expected) in positions.iter().zip(&boards).zip(&before) {
        assert_eq!(
            loaded.forecast(*board, &device),
            *expected,
            "moves {moves:?}"
        );
    }
    Ok(())
}

#[test]
fn parse_accepts_label() {
    assert!(dataset::parse_column_labelled::<B87>("1 + +-------").is_some());
    assert!(dataset::parse_column_labelled::<B87>("1111111 - .-------").is_some());
}

#[test]
fn parse_rejects_bad_result() {
    assert!(dataset::parse_column_labelled::<B87>("1 = +-------").is_none());
}

#[test]
fn parse_rejects_label_on_full() {
    assert!(dataset::parse_column_labelled::<B87>("1111111 + +-------").is_none());
}

#[test]
fn shipped_beats_baseline() -> Result<(), Box<dyn Error>> {
    let device = FlexDevice;
    let model = net::shipped::<Flex>(&device).ok_or("shipped weights load")?;
    let examples = sample(Part::Held, HELD_OUT_SAMPLE)?;
    let evaluation = net::evaluate(&model, &examples, &device).ok_or("evaluation runs")?;
    let accuracy = evaluation.tally.accuracy().ok_or("accuracy defined")?;
    let baseline = evaluation.tally.baseline().ok_or("baseline defined")?;
    assert!(
        accuracy > baseline,
        "accuracy {accuracy} beats baseline {baseline}"
    );
    Ok(())
}

#[test]
fn training_lowers_watched_loss() -> Result<(), Box<dyn Error>> {
    let device = FlexDevice;
    let training = sample(Part::Training, TRAIN_SAMPLE)?;
    let watch = sample(Part::Held, WATCH_SAMPLE)?;
    let schedule = Schedule {
        epochs: EPOCHS,
        learning_rate: LEARNING_RATE,
        seed: SEED,
    };
    let splits = Splits {
        training: &training,
        watch: &watch,
    };
    let trained = net::train::<Autodiff<Flex>, B87>(SMALL, splits, schedule, &device)
        .ok_or("training runs")?;
    let first = trained.reports.first().ok_or("first epoch reported")?;
    let last = trained.reports.last().ok_or("last epoch reported")?;
    assert!(
        last.watched_loss < first.watched_loss,
        "watched loss {} -> {}",
        first.watched_loss,
        last.watched_loss
    );
    Ok(())
}
