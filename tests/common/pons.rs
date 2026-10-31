use std::error::Error;

use c4::board::B76;
use c4::dataset::{Labelled, parse_pons};
use c4::solver::{Label, Solver};

// Pascal Pons, end-game test set Test_L3_R1: http://blog.gamesolver.org/data/Test_L3_R1
const PONS: &str = include_str!("../data/Test_L3_R1");

pub(crate) type Case = (Labelled<B76>, Label);

pub(crate) fn labelled(
    solver: &mut Solver<B76>,
    keep: impl Fn(usize) -> bool,
) -> Result<Vec<Case>, Box<dyn Error>> {
    let mut cases = Vec::new();
    for (_, line) in PONS.lines().enumerate().filter(|(index, _)| keep(*index)) {
        let sample = parse_pons::<B76>(line).ok_or(line)?;
        let label = solver.label(sample.board, None).ok_or(line)?;
        assert_eq!(label.result, sample.result, "{line}");
        cases.push((sample, label));
    }
    Ok(cases)
}
