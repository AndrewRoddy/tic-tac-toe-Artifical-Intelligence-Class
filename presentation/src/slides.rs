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
    pub visual: Visual,
    /// One entry per press of Next: each reveals the next part of the visual and has a
    /// few seconds of things to say (shown in the presenter window, never on the slide).
    pub beats: &'static [Beat],
    /// The time budget for this slide; all of them add up to 10 minutes.
    pub seconds: u32,
}

/// One step of a slide.
pub struct Beat {
    /// An optional short line under the visual while this step is showing.
    pub caption: &'static str,
    pub say: &'static str,
}

const fn say(say: &'static str) -> Beat {
    Beat { caption: "", say }
}

const fn show(caption: &'static str, say: &'static str) -> Beat {
    Beat { caption, say }
}

pub enum Visual {
    /// A single board with a game on it (columns are 0-based).
    Board { columns: u32, rows: u32, moves: &'static [u32] },
    /// One board that physically grows, a stage per beat.
    Expand(&'static [Stage]),
    /// A horizontal bar chart.
    Bars(Bars),
    /// The eight agents; the beat each roster entry shows up on.
    Agents(&'static [usize]),
    /// The convolutional network: board, then layers, then column scores.
    Network,
    /// A 7x6 position with every column solved live by our solver.
    Solved,
}

pub struct Stage {
    pub columns: u32,
    pub rows: u32,
    pub name: &'static str,
    pub note: &'static str,
    /// Discs dropped in when this stage arrives (0-based columns; colours alternate on
    /// from the moves before).
    pub moves: &'static [u32],
}

pub struct Bars {
    pub caption: &'static str,
    /// Value at the left edge of the bars (so close values still look different).
    pub floor: f32,
    pub ceiling: f32,
    /// (label, plotted value, value text, highlighted, beat it shows up on)
    pub items: &'static [(&'static str, f32, &'static str, bool, usize)],
}

/// A 7x6 game, 1-based columns as `Board::from_moves` expects. Solved at startup.
pub const SOLVED_MOVES: &str = "4453432233554";

pub const SLIDES: &[Slide] = &[
    Slide {
        kicker: "group 3  \u{b7}  TJ Raklovits and Andrew Roddy",
        title: "Connect Four AI",
        visual: Visual::Board {
            columns: 9,
            rows: 8,
            moves: &[4, 4, 3, 5, 5, 3, 4, 2, 6, 4, 3, 3, 5, 2, 6, 6, 2],
        },
        beats: &[
            say("Hey everyone, we're TJ and Andrew from group 3, and our project is Connect Four."),
            say("We trained agents on an 8 by 7 board and tested them on 9 by 8, which nobody has solved. Everything you'll see today, these slides included, is running live in our Rust code."),
        ],
        seconds: 25,
    },
    Slide {
        kicker: "the question",
        title: "A board nobody has solved",
        visual: Visual::Expand(&[
            Stage { columns: 7, rows: 6, name: "7 \u{d7} 6", note: "solved\nfirst player wins", moves: &[3, 3, 2, 4, 4, 2, 3, 5] },
            Stage { columns: 8, rows: 7, name: "8 \u{d7} 7", note: "solved by John Tromp\nour training board", moves: &[7, 6] },
            Stage { columns: 9, rows: 8, name: "9 \u{d7} 8", note: "never solved\nour test board", moves: &[8, 3] },
            Stage { columns: 9, rows: 8, name: "?", note: "does what they learn\ncarry over?", moves: &[] },
        ]),
        beats: &[
            say("This is the normal 7 by 6 board. It's solved, and if both sides play perfectly the first player wins."),
            say("Add a column and a row and you get 8 by 7. John Tromp solved this one too, also a first player win. This is where we trained."),
            say("One more of each and you're at 9 by 8. Nobody has solved it. We worked out it would take our hardware something like 15 years."),
            say("So our question was pretty simple. If we train agents on a board we can solve, do they still play well on one we can't? And if some methods carry over better than others, why?"),
        ],
        seconds: 50,
    },
    Slide {
        kicker: "scale",
        title: "How big is the game?",
        visual: Visual::Bars(Bars {
            caption: "positions (log scale, powers of ten)",
            floor: 0.0,
            ceiling: 35.0,
            items: &[
                ("7x6", 12.65, "10^12.7", false, 0),
                ("8x7", 26.7, "10^26.7", false, 1),
                ("9x8", 34.3, "10^34.3", true, 2),
            ],
        }),
        beats: &[
            say("Just to give you a feel for the size. 7 by 6 has about 4.5 trillion legal positions."),
            say("For 8 by 7 the upper bound is around 10 to the 26."),
            say("And 9 by 8 is around 10 to the 34. This is a log scale, so every step up is millions of times bigger. You just can't brute force that."),
        ],
        seconds: 30,
    },
    Slide {
        kicker: "step 1",
        title: "An exact solver",
        visual: Visual::Solved,
        beats: &[
            say("Step one was an exact solver. It's alpha-beta search on bitboards with a transposition table, which is pretty much how the published Connect Four solvers work."),
            say("Give it any position and it tells you whether each column wins, draws or loses. These W, D and L labels were worked out live when the app started up."),
            show(
                "67,557 UCI positions  \u{b7}  6,000 Pons tests  \u{b7}  0 wrong",
                "We checked it against all 67,557 positions in the UCI dataset and 6,000 of Pascal Pons' test positions, and it got every single one right.",
            ),
        ],
        seconds: 40,
    },
    Slide {
        kicker: "the wall",
        title: "Why not just solve 9x8?",
        visual: Visual::Bars(Bars {
            caption: "median seconds to solve a 9x8 position, by stones",
            floor: -1.5,
            ceiling: 3.0,
            items: &[
                ("48 stones", -1.03, "0.09 s", false, 0),
                ("36 stones", -0.9, "0.13 s", false, 0),
                ("30 stones", -0.5, "0.32 s", false, 0),
                ("26 stones", 0.34, "2.2 s", false, 1),
                ("24 stones", 1.07, "11.7 s", false, 1),
                ("20 stones", 1.9, "80 s", false, 2),
                ("16 stones", 2.9, "> 600 s", true, 2),
            ],
        }),
        beats: &[
            say("So why didn't we just solve 9 by 8? Late in the game it's quick, about a tenth of a second."),
            say("But solve time blows up as the board gets emptier. At 24 stones the median is around 12 seconds, and the worst one we saw took 31 minutes."),
            say("At 16 stones every probe we ran went past ten minutes."),
            show(
                "8x7 with just 6 stones: over half an hour",
                "Even 8 by 7 with only six stones took more than half an hour. That wall is really the reason we needed learning at all.",
            ),
        ],
        seconds: 45,
    },
    Slide {
        kicker: "step 2",
        title: "The data",
        visual: Visual::Bars(Bars {
            caption: "solved positions",
            floor: 0.0,
            ceiling: 110_000.0,
            items: &[
                ("8x7 training", 106_898.0, "106,898", true, 0),
                ("8x7 openings", 2_000.0, "2,000", false, 1),
                ("9x8 late game", 9_990.0, "9,990", false, 2),
            ],
        }),
        beats: &[
            say("We used the solver to build our data. About 107 thousand 8 by 7 positions between 14 and 44 stones, with every column labelled."),
            say("We also made 2,000 harder openings, around 10 to 12 stones, just for testing."),
            show(
                "192 cores, one position each",
                "And we solved about 10 thousand late game 9 by 8 positions, so we could check how well things transfer. All of that ran on a 192 core cloud machine.",
            ),
        ],
        seconds: 35,
    },
    Slide {
        kicker: "step 3",
        title: "Eight agents",
        // MCTS, AB learned, AB handcrafted, network MCTS, Allis, solver, network, safe random.
        visual: Visual::Agents(&[0, 1, 0, 1, 2, 0, 1, 2]),
        beats: &[
            say("Then we built eight agents. First the search ones: MCTS, alpha-beta, and the exact solver itself."),
            say("Then the ones that learn: a learned evaluation, a neural network, and the network guiding MCTS."),
            say("And Allis' hand written rules, plus a safe random baseline that plays randomly but always takes a win and blocks a threat. Every agent gets one second per move."),
        ],
        seconds: 30,
    },
    Slide {
        kicker: "search",
        title: "Alpha-beta: how deep?",
        visual: Visual::Bars(Bars {
            caption: "8x7 move accuracy (%) by search depth",
            floor: 85.0,
            ceiling: 100.0,
            items: &[
                ("depth 1", 90.8, "90.8%", false, 0),
                ("depth 2", 93.8, "93.8%", false, 1),
                ("depth 4", 95.6, "95.6%", false, 1),
                ("depth 6", 96.3, "96.3%", false, 1),
                ("depth 8", 97.5, "97.5%", true, 2),
                ("depth 12", 97.8, "97.8%", false, 2),
            ],
        }),
        beats: &[
            say("First up is alpha-beta with a handcrafted evaluation that looks at threats and row parity. Even at depth 1 it agrees with the solver about 91 percent of the time."),
            say("Every extra bit of depth helps a little."),
            say("Depth 8 gets to 97.5. Depth 12 barely adds anything, and it looks at 35 times as many nodes, around 380 thousand a move."),
            show(
                "but deeper still wins 2 of 3 games on 9x8",
                "But in real games on 9 by 8 the deeper search still wins about two out of three, so the extra depth does pay off.",
            ),
        ],
        seconds: 45,
    },
    Slide {
        kicker: "search + learning",
        title: "Learning the evaluation",
        visual: Visual::Bars(Bars {
            caption: "held-out 8x7 result accuracy (%)",
            floor: 50.0,
            ceiling: 75.0,
            items: &[
                ("constant guess", 56.6, "56.6%", false, 0),
                ("handcrafted", 67.8, "67.8%", false, 1),
                ("learned", 70.1, "70.1%", true, 2),
            ],
        }),
        beats: &[
            say("Next we tried learning the evaluation instead of tuning it by hand. Just guessing the most common result gets 56.6 percent."),
            say("Our hand tuned evaluation gets 67.8."),
            show(
                "on 9x8: Elo 722 learned vs 690 handcrafted",
                "We fit a logistic regression on the same threat features, split into good rows and bad rows, using the solver's 8 by 7 labels. It gets 70.1, and it holds up on 9 by 8 too, 722 Elo against 690.",
            ),
        ],
        seconds: 40,
    },
    Slide {
        kicker: "search",
        title: "Monte Carlo tree search",
        visual: Visual::Bars(Bars {
            caption: "9x8 tournament Elo by time per move",
            floor: 600.0,
            ceiling: 780.0,
            items: &[
                ("0.5 s", 694.5, "695", false, 0),
                ("1 s", 739.9, "740", false, 2),
                ("5 s", 754.4, "754", false, 3),
                ("10 s", 760.7, "761", true, 3),
            ],
        }),
        beats: &[
            say("Monte Carlo tree search doesn't need an evaluation function at all. It plays a ton of random games to the end, about 740 thousand a second on 9 by 8."),
            show(
                "+ proven wins and losses",
                "It grows its tree toward whichever moves win the most. We added a rule for proven results, so it never misses a win or a loss that's right in front of it.",
            ),
            say("At one second a move it ended up being our strongest agent."),
            say("And it keeps getting better the more time you give it."),
        ],
        seconds: 45,
    },
    Slide {
        kicker: "learning",
        title: "A network for any board",
        visual: Visual::Network,
        beats: &[
            say("For the neural network we went fully convolutional, so the exact same weights can run on any board size."),
            say("It's 6 layers with 64 channels. We trained it for 12 epochs on the CPU because our GPU setup failed."),
            show(
                "92.4% of 8x7 moves  \u{b7}  91% of 9x8 moves",
                "Out comes a score for each column. It matches the solver on 92.4 percent of 8 by 7 moves and still 91 percent on 9 by 8, so what it learned really does transfer.",
            ),
            show(
                "alone: weak  \u{b7}  guiding MCTS: Elo 647",
                "The catch is that one bad move can lose a whole game, so on its own it's pretty weak. When we use it to guide MCTS it gets to an Elo of 647.",
            ),
        ],
        seconds: 50,
    },
    Slide {
        kicker: "rules",
        title: "Victor Allis' strategy",
        visual: Visual::Bars(Bars {
            caption: "9x8 score vs safe random (%)",
            floor: 50.0,
            ceiling: 90.0,
            items: &[
                ("all our rules", 78.2, "78.2%", true, 1),
                ("no claimeven", 83.5, "83.5%", false, 2),
                ("parity flipped", 75.2, "75.2%", false, 3),
            ],
        }),
        beats: &[
            show(
                "win  \u{b7}  block  \u{b7}  play safe  \u{b7}  claimeven  \u{b7}  threats on good rows",
                "We also coded up part of Victor Allis' strategy rules, with no search at all. Things like win, block, play safe moves, claimeven, and make threats on good rows.",
            ),
            say("On 9 by 8 it beats safe random 78 percent of the time, and 83 on 8 by 7, so the ideas do carry over."),
            say("Funny enough, turning off claimeven didn't hurt it at all."),
            say("Flipping the parity hurt a little. Our set of rules is just too small to play a strong game."),
        ],
        seconds: 40,
    },
    Slide {
        kicker: "results",
        title: "The 9x8 tournament",
        visual: Visual::Bars(Bars {
            caption: "9x8 Elo at 1 s per move",
            floor: 0.0,
            ceiling: 760.0,
            items: &[
                ("MCTS", 739.9, "740", true, 3),
                ("alpha-beta learned", 722.1, "722", false, 3),
                ("alpha-beta handcrafted", 690.3, "690", false, 2),
                ("network MCTS", 647.1, "647", false, 2),
                ("Allis rules", 160.4, "160", false, 1),
                ("exact solver", 77.9, "78", false, 1),
                ("network alone", 36.5, "37", false, 1),
                ("safe random", 0.0, "0", false, 1),
            ],
        }),
        beats: &[
            show(
                "every pair  \u{b7}  50 openings  \u{b7}  both colours  \u{b7}  2,800 games",
                "This is the main result. We ran a round robin on 9 by 8 where every pair of agents played 50 random openings, both colours, one second a move. That's 2,800 games, with safe random pinned at an Elo of 0.",
            ),
            say("Down at the bottom: the rules, the time limited solver, and the network on its own."),
            say("Then network guided MCTS, and the handcrafted alpha-beta."),
            say("And at the top, the learned alpha-beta at 722, and MCTS at 740."),
        ],
        seconds: 45,
    },
    Slide {
        kicker: "why",
        title: "Why it came out this way",
        visual: Visual::Bars(Bars {
            caption: "8x7: won positions kept against the solver (%)",
            floor: 50.0,
            ceiling: 100.0,
            items: &[
                ("alpha-beta learned", 97.0, "97.0%", false, 2),
                ("MCTS", 96.5, "96.5%", true, 2),
                ("network MCTS", 96.0, "96.0%", false, 2),
                ("exact solver (1 s)", 89.4, "89.4%", false, 0),
                ("network alone", 80.3, "80.3%", false, 1),
                ("Allis rules", 77.8, "77.8%", false, 1),
                ("safe random", 54.5, "54.5%", false, 1),
            ],
        }),
        beats: &[
            say("So why did it turn out like this? Basically, search wins on a board you can't solve. The exact solver is perfect when it finishes, but it can't finish a 9 by 8 position in one second, so it falls back to playing near the middle."),
            say("Learned knowledge transfers well, but without search one mistake costs you the game."),
            say("Put search on top of that knowledge and most of the problem goes away."),
            show(
                "search + knowledge wins",
                "This chart is the same story on 8 by 7, measured against the solver itself.",
            ),
        ],
        seconds: 50,
    },
    Slide {
        kicker: "demo",
        title: "Now watch them play",
        visual: Visual::Board {
            columns: 8,
            rows: 7,
            moves: &[3, 4, 3, 3, 4, 2, 5, 5, 2, 4, 1, 6],
        },
        beats: &[
            say("Alright, let's actually watch them play."),
            say("I'll pick two agents and a board size. These are the real agents from our results, playing live, and we can even put them on the 9 by 8 test board."),
        ],
        seconds: 30,
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

    #[test]
    fn growing_board_games_are_legal() {
        for slide in SLIDES {
            if let Visual::Expand(stages) = slide.visual {
                let mut heights = vec![0; 16];
                for stage in stages {
                    for &col in stage.moves {
                        assert!(col < stage.columns);
                        heights[col as usize] += 1;
                        assert!(heights[col as usize] <= stage.rows);
                    }
                }
            }
        }
    }

    #[test]
    fn beats_line_up() {
        assert_eq!(SLIDES.iter().map(|slide| slide.seconds).sum::<u32>(), 600);
        for slide in SLIDES {
            let beats = slide.beats.len();
            assert!(beats >= 1, "{}", slide.title);
            for beat in slide.beats {
                assert!(!beat.say.contains('\u{2014}') && !beat.say.contains('\u{2013}'));
            }
            match &slide.visual {
                Visual::Bars(bars) => assert!(bars.items.iter().all(|item| item.4 < beats), "{}", slide.title),
                Visual::Agents(list) => assert!(list.iter().all(|&beat| beat < beats)),
                Visual::Expand(stages) => assert_eq!(stages.len(), beats),
                _ => {}
            }
        }
    }
}
