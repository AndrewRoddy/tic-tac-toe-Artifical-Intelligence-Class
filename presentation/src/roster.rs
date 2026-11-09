//! The eight agents from the project, with descriptions and our measured results
//! (results/accuracy.csv and results/measurements.csv, 1 s per move).

use c4::tournament::Entrant;

pub struct AgentInfo {
    pub entrant: Entrant,
    pub title: &'static str,
    pub family: &'static str,
    pub how: &'static str,
    pub elo: f32,
    pub acc_8x7: f32,
    pub acc_9x8: f32,
}

pub const ROSTER: [AgentInfo; 8] = [
    AgentInfo {
        entrant: Entrant::Mcts,
        title: "MCTS (UCT)",
        family: "Search",
        how: "Monte Carlo tree search: plays thousands of quick random games from each move and grows a tree toward the moves that win most.",
        elo: 739.9,
        acc_8x7: 0.987,
        acc_9x8: 0.991,
    },
    AgentInfo {
        entrant: Entrant::Learned,
        title: "Alpha-beta (learned)",
        family: "Search + learning",
        how: "Iterative-deepening alpha-beta. The board evaluation uses threat and row-parity features weighted by logistic regression on 8x7 solver labels.",
        elo: 722.1,
        acc_8x7: 0.989,
        acc_9x8: 0.993,
    },
    AgentInfo {
        entrant: Entrant::Handcrafted,
        title: "Alpha-beta (handcrafted)",
        family: "Search",
        how: "Iterative-deepening alpha-beta with hand-tuned weights for threats on good and bad rows (Allis' odd/even row parity).",
        elo: 690.3,
        acc_8x7: 0.988,
        acc_9x8: 0.993,
    },
    AgentInfo {
        entrant: Entrant::NetworkMcts,
        title: "Network-guided MCTS",
        family: "Search + learning",
        how: "AlphaZero-style: the convolutional network gives move priors and a position value, and the tree search uses them in place of random playouts.",
        elo: 647.1,
        acc_8x7: 0.982,
        acc_9x8: 0.985,
    },
    AgentInfo {
        entrant: Entrant::Allis,
        title: "Allis rules",
        family: "Knowledge",
        how: "No search. Victor Allis' strategy rules: win, block, avoid unsafe squares, claimeven follow-up, and make threats on rows of the right parity.",
        elo: 160.4,
        acc_8x7: 0.883,
        acc_9x8: 0.897,
    },
    AgentInfo {
        entrant: Entrant::Perfect,
        title: "Exact solver",
        family: "Search",
        how: "Our bitboard alpha-beta solver with a transposition table. Perfect when it finishes in time, but early 9x8 positions are far too big to solve.",
        elo: 77.9,
        acc_8x7: 0.978,
        acc_9x8: 0.978,
    },
    AgentInfo {
        entrant: Entrant::Network,
        title: "Convolutional network",
        family: "Learning",
        how: "A fully convolutional net (6 layers, 64 channels) trained on 107k labelled 8x7 positions. It predicts win/draw/loss per column with no search.",
        elo: 36.5,
        acc_8x7: 0.924,
        acc_9x8: 0.910,
    },
    AgentInfo {
        entrant: Entrant::SafeRandom,
        title: "Safe random",
        family: "Baseline",
        how: "Plays at random, but always takes a winning move and blocks an immediate threat. The Elo anchor (0).",
        elo: 0.0,
        acc_8x7: 0.764,
        acc_9x8: 0.778,
    },
];

pub const DEFAULT_RED: usize = 0;
pub const DEFAULT_YELLOW: usize = 1;
