//! The presentation's content: what we did, how, and why the results came out the way
//! they did. Every number comes from `results/` (measurements.csv, accuracy.csv,
//! DEVIATIONS.md) or `midterm_proposal.md`.

use std::time::{Duration, Instant};

use c4::board::{B76, Board};
use c4::outcome::Outcome;
use c4::solver::Solver;

pub struct Slide {
    pub kicker: &'static str,
    pub title: &'static str,
    pub lines: &'static [&'static str],
    pub visual: Visual,
    /// What to say (shown in the presenter window, never on the slide).
    pub notes: &'static str,
    /// The time budget for this slide; all of them add up to 10 minutes.
    pub seconds: u32,
}

pub enum Visual {
    /// A single board with a game on it (columns are 0-based).
    Board { columns: u32, rows: u32, moves: &'static [u32] },
    /// Empty boards side by side with captions.
    Boards(&'static [(u32, u32, &'static str)]),
    /// A horizontal bar chart.
    Bars(Bars),
    /// The eight agents, grouped by family.
    Agents,
    /// The convolutional network as a stack of layers.
    Network,
    /// A 7x6 position with every column solved live by our solver.
    Solved,
}

pub struct Bars {
    pub caption: &'static str,
    /// Value at the left edge of the bars (so close values still look different).
    pub floor: f32,
    pub ceiling: f32,
    /// (label, plotted value, value text, highlighted)
    pub items: &'static [(&'static str, f32, &'static str, bool)],
}

/// A 7x6 game, 1-based columns as `Board::from_moves` expects. Solved at startup.
pub const SOLVED_MOVES: &str = "4453432233554";

pub const SLIDES: &[Slide] = &[
    Slide {
        kicker: "AI  \u{b7}  group 3",
        title: "Connect Four AI",
        lines: &[
            "Train on 8x7. Test on 9x8.",
            "",
            "TJ Raklovits  \u{b7}  Andrew Roddy",
        ],
        visual: Visual::Board {
            columns: 9,
            rows: 8,
            moves: &[4, 4, 3, 5, 5, 3, 4, 2, 6, 4, 3, 3, 5, 2, 6, 6, 2],
        },
        notes: "Hi, we're TJ and Andrew, group 3. Our project: train Connect Four agents on an 8x7 board, then test them on a bigger 9x8 board that nobody has solved. Everything you'll see, including this deck, runs live in our Rust code.",
        seconds: 20,
    },
    Slide {
        kicker: "the question",
        title: "A board nobody has solved",
        lines: &[
            "7x6, the classic board, is solved:",
            "the first player wins.",
            "John Tromp solved 8x7: first player wins.",
            "9x8 is unsolved. For us it would take",
            "about 15 years of compute.",
            "So we train on 8x7 and test on 9x8.",
        ],
        visual: Visual::Boards(&[(7, 6, "7x6  solved"), (8, 7, "8x7  train"), (9, 8, "9x8  test")]),
        notes: "The classic 7x6 board is solved: with perfect play the first player wins. John Tromp also solved 8x7, and again the first player wins. 9x8 has no published solution; on our hardware it would take around fifteen years. So the question is: can agents trained on a board we CAN solve play well on one we can't? And why do some methods transfer and others not?",
        seconds: 50,
    },
    Slide {
        kicker: "scale",
        title: "How big is the game?",
        lines: &[
            "7x6: about 4.5 x 10^12 legal positions.",
            "8x7: up to 3^56, about 5 x 10^26 patterns.",
            "9x8: up to 3^72, about 2 x 10^34 patterns.",
            "",
            "Each step up is millions of times bigger.",
        ],
        visual: Visual::Bars(Bars {
            caption: "positions (log scale, powers of ten)",
            floor: 0.0,
            ceiling: 35.0,
            items: &[
                ("7x6", 12.65, "10^12.7", false),
                ("8x7", 26.7, "10^26.7", false),
                ("9x8", 34.3, "10^34.3", true),
            ],
        }),
        notes: "To give a sense of scale: 7x6 has about 4.5 trillion legal positions. 8x7 has up to 10 to the 26 cell patterns, and 9x8 up to 10 to the 34. Each step up is millions of times bigger, which is why brute force stops working.",
        seconds: 35,
    },
    Slide {
        kicker: "step 1",
        title: "An exact solver",
        lines: &[
            "Alpha-beta search on bitboards,",
            "with a transposition table.",
            "It labels every column: win, draw or loss.",
            "Checked on 67,557 UCI positions and",
            "6,000 Pons test positions: 0 mismatches.",
            "This position was solved just now, live.",
        ],
        visual: Visual::Solved,
        notes: "Step one was an exact solver: alpha-beta search on bitboards with a transposition table, like published Connect Four solvers. It labels every column as a win, draw or loss. We checked it on all 67,557 UCI positions and 6,000 of Pons' test positions with zero mismatches. The labels on the right were computed live when this app started.",
        seconds: 50,
    },
    Slide {
        kicker: "the wall",
        title: "Why not just solve 9x8?",
        lines: &[
            "Solve time explodes as the board empties.",
            "40 stones: 0.1 s.  28 stones: 0.9 s.",
            "24 stones: 11.7 s median, worst 1,857 s.",
            "16 stones: over 10 minutes, every time.",
            "8x7 with 6 stones: over 30 minutes.",
            "(20 and 16 stones: laptop runs.)",
        ],
        visual: Visual::Bars(Bars {
            caption: "median seconds to solve a 9x8 position, by stones",
            floor: -1.5,
            ceiling: 3.0,
            items: &[
                ("48 stones", -1.03, "0.09 s", false),
                ("36 stones", -0.9, "0.13 s", false),
                ("30 stones", -0.5, "0.32 s", false),
                ("26 stones", 0.34, "2.2 s", false),
                ("24 stones", 1.07, "11.7 s", false),
                ("20 stones", 1.9, "80 s", false),
                ("16 stones", 2.9, "> 600 s", true),
            ],
        }),
        notes: "So why not just solve 9x8? Solve time explodes as the board gets emptier. Late positions take a tenth of a second; at 24 stones the median is 12 seconds and the worst took 31 minutes; at 16 stones every one of our probes ran past ten minutes. Even 8x7 with six stones took over half an hour. That wall is the whole motivation for learning.",
        seconds: 45,
    },
    Slide {
        kicker: "step 2",
        title: "The data",
        lines: &[
            "106,898 8x7 positions, 14 to 44 stones,",
            "every column labelled by the solver.",
            "2,000 8x7 openings (10-12 stones) for tests.",
            "9,990 late 9x8 positions (24-60 stones).",
            "Solved one core per position",
            "on a 192-vCPU cloud machine.",
        ],
        visual: Visual::Bars(Bars {
            caption: "solved positions",
            floor: 0.0,
            ceiling: 110_000.0,
            items: &[
                ("8x7 training", 106_898.0, "106,898", true),
                ("8x7 openings", 2_000.0, "2,000", false),
                ("9x8 late game", 9_990.0, "9,990", false),
            ],
        }),
        notes: "We used the solver to build the data: about 107 thousand 8x7 positions from 14 to 44 stones, every column labelled; 2,000 harder opening positions for testing; and 10 thousand late 9x8 positions we could still solve exactly, for checking transfer. We ran one core per position on a 192-vCPU cloud machine.",
        seconds: 35,
    },
    Slide {
        kicker: "step 3",
        title: "Eight agents",
        lines: &[
            "Search, learning, rules,",
            "the exact solver, and a baseline.",
            "",
            "Each one picks a column",
            "within one second.",
        ],
        visual: Visual::Agents,
        notes: "Then we built eight agents across four families: search, learning, hand-written rules, and the exact solver, plus a safe-random baseline that takes wins and blocks threats. Each one gets one second per move.",
        seconds: 30,
    },
    Slide {
        kicker: "search",
        title: "Alpha-beta: how deep?",
        lines: &[
            "Depth 1: 90.8% of moves match the solver.",
            "Depth 8: 97.5%.  Depth 12: 97.8%.",
            "Nodes per move: 7 at depth 1,",
            "379,324 at depth 12.",
            "On 9x8 the deeper search wins",
            "about two games in three.",
        ],
        visual: Visual::Bars(Bars {
            caption: "8x7 move accuracy (%) by search depth",
            floor: 85.0,
            ceiling: 100.0,
            items: &[
                ("depth 1", 90.8, "90.8%", false),
                ("depth 2", 93.8, "93.8%", false),
                ("depth 4", 95.6, "95.6%", false),
                ("depth 6", 96.3, "96.3%", false),
                ("depth 8", 97.5, "97.5%", true),
                ("depth 12", 97.8, "97.8%", false),
            ],
        }),
        notes: "First, alpha-beta with a handcrafted evaluation of threats and row parity. Depth 1 already matches the solver 91% of the time; depth 8 gets 97.5%, and depth 12 only adds a little more while searching 35 times more nodes. On 9x8, the deeper search still wins about two games in three, so depth keeps paying off in real games.",
        seconds: 45,
    },
    Slide {
        kicker: "search + learning",
        title: "Learning the evaluation",
        lines: &[
            "Features: row threats for each player,",
            "split into good rows (row parity) and others.",
            "Logistic regression on 8x7 solver labels.",
            "It beats the hand-tuned weights on 8x7,",
            "and on 9x8: Elo 722 vs 690.",
        ],
        visual: Visual::Bars(Bars {
            caption: "held-out 8x7 result accuracy (%)",
            floor: 50.0,
            ceiling: 75.0,
            items: &[
                ("constant guess", 56.6, "56.6%", false),
                ("handcrafted", 67.8, "67.8%", false),
                ("learned", 70.1, "70.1%", true),
            ],
        }),
        notes: "Then we learned the evaluation instead of hand-tuning it: logistic regression on the same threat features, fit to the solver's 8x7 labels. It predicts held-out results better than the handcrafted weights, 70.1% against 67.8%, and on 9x8 it ranks higher too: Elo 722 against 690.",
        seconds: 40,
    },
    Slide {
        kicker: "search",
        title: "Monte Carlo tree search",
        lines: &[
            "Plays random games to the end:",
            "about 740,000 a second on 9x8.",
            "Grows a tree toward moves that win (UCT).",
            "Added a proven-result rule for instant",
            "wins and losses.",
            "More time keeps making it stronger.",
        ],
        visual: Visual::Bars(Bars {
            caption: "9x8 tournament Elo by time per move",
            floor: 600.0,
            ceiling: 780.0,
            items: &[
                ("0.5 s", 694.5, "695", false),
                ("1 s", 739.9, "740", false),
                ("5 s", 754.4, "754", false),
                ("10 s", 760.7, "761", true),
            ],
        }),
        notes: "Monte Carlo tree search needs no evaluation at all: it plays random games to the end, about 740 thousand a second on 9x8, and grows a tree toward the moves that win. We added a proven-result rule so it never misses an instant win or loss. It is the strongest agent, and it keeps getting stronger with more time.",
        seconds: 45,
    },
    Slide {
        kicker: "learning",
        title: "A network for any board",
        lines: &[
            "Fully convolutional, 64 channels x 6 layers,",
            "so the same weights run on 8x7 and 9x8.",
            "12 epochs on the CPU (the GPU failed).",
            "8x7 move accuracy 92.4%, 9x8 still 91.0%:",
            "the knowledge transfers.",
            "Alone it blunders. Inside MCTS: Elo 647.",
        ],
        visual: Visual::Network,
        notes: "The neural network is fully convolutional, so the same weights run on any board size. We trained it on 8x7 labels, on the CPU after the GPU failed. It matches the solver on 92.4% of 8x7 moves and still 91.0% on 9x8, so the knowledge transfers. But one wrong move in a game is enough to lose, so on its own it is weak; used to guide MCTS it reaches Elo 647.",
        seconds: 50,
    },
    Slide {
        kicker: "rules",
        title: "Victor Allis' strategy",
        lines: &[
            "No search: win, block, safe moves,",
            "claimeven, threats on good rows.",
            "Beats safe random 83% on 8x7, 78% on 9x8.",
            "Ablation: dropping claimeven did not hurt.",
            "The rules transfer, but our subset",
            "is too small to play strong games.",
        ],
        visual: Visual::Bars(Bars {
            caption: "9x8 score vs safe random (%)",
            floor: 50.0,
            ceiling: 90.0,
            items: &[
                ("all our rules", 78.2, "78.2%", true),
                ("no claimeven", 83.5, "83.5%", false),
                ("parity flipped", 75.2, "75.2%", false),
            ],
        }),
        notes: "We also coded part of Victor Allis' strategy rules, with no search. They beat safe random 83% of the time on 8x7 and 78% on 9x8, so the ideas carry over. Interestingly, removing the claimeven rule didn't hurt. Our subset of the rules is just too small to play strong games.",
        seconds: 35,
    },
    Slide {
        kicker: "results",
        title: "The 9x8 tournament",
        lines: &[
            "Every pair, 50 openings,",
            "both colours, 1 s per move.",
            "2,800 games.",
            "Elo with safe random fixed at 0.",
        ],
        visual: Visual::Bars(Bars {
            caption: "9x8 Elo at 1 s per move",
            floor: 0.0,
            ceiling: 760.0,
            items: &[
                ("MCTS", 739.9, "740", true),
                ("alpha-beta learned", 722.1, "722", false),
                ("alpha-beta handcrafted", 690.3, "690", false),
                ("network MCTS", 647.1, "647", false),
                ("Allis rules", 160.4, "160", false),
                ("exact solver", 77.9, "78", false),
                ("network alone", 36.5, "37", false),
                ("safe random", 0.0, "0", false),
            ],
        }),
        notes: "The main result: a round robin on 9x8, every pair, 50 random openings, both colours, one second a move, 2,800 games. MCTS comes first at Elo 740, then the learned and handcrafted alpha-betas, then network-guided MCTS. The rules, the time-limited solver and the network alone are far behind.",
        seconds: 45,
    },
    Slide {
        kicker: "why",
        title: "Why it came out this way",
        lines: &[
            "Search wins: MCTS, then both alpha-betas.",
            "The solver can't finish 9x8 in 1 s, so it",
            "falls back to the centre: Elo 78.",
            "The network transfers, but one bad move",
            "loses a game. Search on top fixes most of it.",
        ],
        visual: Visual::Bars(Bars {
            caption: "8x7: won positions kept against the solver (%)",
            floor: 50.0,
            ceiling: 100.0,
            items: &[
                ("alpha-beta learned", 97.0, "97.0%", false),
                ("MCTS", 96.5, "96.5%", true),
                ("network MCTS", 96.0, "96.0%", false),
                ("exact solver (1 s)", 89.4, "89.4%", false),
                ("network alone", 80.3, "80.3%", false),
                ("Allis rules", 77.8, "77.8%", false),
                ("safe random", 54.5, "54.5%", false),
            ],
        }),
        notes: "Why? Search wins on a board you can't solve. The exact solver is perfect, but it can't finish a 9x8 position in one second, so it falls back to playing near the centre and lands at Elo 78. Learned knowledge transfers well, but without search one mistake loses the game. Search with learned knowledge on top fixes most of that. The chart shows the same story on 8x7, against the solver itself.",
        seconds: 50,
    },
    Slide {
        kicker: "demo",
        title: "Now watch them play",
        lines: &[
            "Pick any two agents and a board size.",
            "",
            "Press Esc to open the arena.",
        ],
        visual: Visual::Board {
            columns: 8,
            rows: 7,
            moves: &[3, 4, 3, 3, 4, 2, 5, 5, 2, 4, 1, 6],
        },
        notes: "Now let's watch them play. I'll pick two agents and a board size, and you can see the real agents from our results playing live, including on the 9x8 test board.",
        seconds: 25,
    },
];

/// Solves every column of `SOLVED_MOVES` (7x6) with the project's solver.
/// `None` for a full column; the whole thing is `None` if it does not finish in time.
pub fn solve_position(limit: Duration) -> Option<Vec<Option<Outcome>>> {
    let board = Board::<B76>::from_moves(SOLVED_MOVES)?;
    let mut solver = Solver::<B76>::new(22);
    solver.label(board, Some(Instant::now() + limit)).map(|label| label.columns)
}

/// `SOLVED_MOVES` as 0-based columns.
pub fn solved_columns() -> Vec<u32> {
    SOLVED_MOVES.chars().filter_map(|ch| ch.to_digit(10)).map(|col| col - 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_live_position_solves_quickly() {
        let started = Instant::now();
        let columns = solve_position(Duration::from_secs(60)).expect("solves within a minute");
        eprintln!("solved in {:?}: {columns:?}", started.elapsed());
        assert_eq!(columns.len(), 7);
    }

    #[test]
    fn showcase_games_are_legal() {
        for slide in SLIDES {
            if let Visual::Board { columns, rows, moves } = slide.visual {
                let mut heights = vec![0; columns as usize];
                for &col in moves {
                    heights[col as usize] += 1;
                    assert!(heights[col as usize] <= rows, "{}: column {col} overflows", slide.title);
                }
            }
        }
    }
}
