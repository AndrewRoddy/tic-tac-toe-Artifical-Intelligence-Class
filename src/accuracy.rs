use crate::outcome::Outcome;

const Z_95: f64 = 1.959_963_984_540_054;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    pub low: f64,
    pub high: f64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Tally {
    positions: u32,
    correct: u32,
    baseline_sum: f64,
}

impl Tally {
    pub fn record(&mut self, columns: &[Option<Outcome>], column: u32) -> Option<()> {
        let best = columns.iter().flatten().max().copied()?;
        let legal = u32::try_from(columns.iter().flatten().count()).ok()?;
        let best_count =
            u32::try_from(columns.iter().filter(|label| **label == Some(best)).count()).ok()?;
        let chosen = usize::try_from(column)
            .ok()
            .and_then(|index| columns.get(index).copied().flatten());
        self.positions += 1;
        self.correct += u32::from(chosen == Some(best));
        self.baseline_sum += f64::from(best_count) / f64::from(legal);
        Some(())
    }

    #[must_use]
    pub const fn positions(&self) -> u32 {
        self.positions
    }

    #[must_use]
    pub fn accuracy(&self) -> Option<f64> {
        (self.positions > 0).then(|| f64::from(self.correct) / f64::from(self.positions))
    }

    #[must_use]
    pub fn baseline(&self) -> Option<f64> {
        (self.positions > 0).then(|| self.baseline_sum / f64::from(self.positions))
    }

    #[must_use]
    pub fn interval(&self) -> Option<Interval> {
        wilson(self.correct, self.positions)
    }
}

#[must_use]
fn wilson(correct: u32, total: u32) -> Option<Interval> {
    if total == 0 {
        return None;
    }
    let n = f64::from(total);
    let rate = f64::from(correct) / n;
    let z_squared = Z_95 * Z_95;
    let denominator = 1.0 + z_squared / n;
    let centre = (rate + z_squared / (2.0 * n)) / denominator;
    let half = Z_95 * (rate * (1.0 - rate) / n + z_squared / (4.0 * n * n)).sqrt() / denominator;
    Some(Interval {
        low: (centre - half).max(0.0),
        high: (centre + half).min(1.0),
    })
}
