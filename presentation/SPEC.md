# Connect Four Arena 3D — Presentation App Spec

# TODO

- [ ] Model the colors and style off of C:/Coding/Watch-Ember
- [ ] Make a presentation mode, where if I press play in the top right or something, it begins the presentation
  - this presentation should kind of explain everything that we did and how like certain things were solved and why they were solved by
  - Make everything in the presentation 3d down to the text
- [ ] Make a multiple board mode where I can press plus and expand the number of board
- [ ] Make a lot of cool animations. Like a winning animation or effect stuff like that
- [ ] Make a 'board expanding' animation where we literally see the size of the board increase like it widens, the increases in height, make it a really cool animation

A Bevy app for the class presentation. It shows the project in 3D: pick two of
our eight agents, grow or shrink the board, and watch the agents play.

## Goals

1. **Choose the agents.** Pick any of the 8 agents (same names as the paper) for Red
   (moves first) and for Yellow. Each one shows a short "how it works" line and
   our measured results (9×8 Elo at 1 s, 8×7 and 9×8 accuracy).
2. **Grow and shrink the board.** Click **−** / **+** to step through a ladder of sizes:
   `4×4 → 5×4 → 6×5 → 7×6 (standard) → 8×7 (training) → 9×8 (test)`.
   The 3D board rebuilds with a short scale animation, and the camera moves to fit.
3. **Watch a match.** Pressing Play runs the real agents from the `c4` crate in a
   background thread with the chosen time per move. Each disc drops into its
   column with gravity and a bounce. When someone wins, the four winning discs glow.

Not goals: training models, running the full tournament, network play, saving games.

## Architecture

```
presentation/           standalone Cargo package (own workspace, own lockfile)
  Cargo.toml            bevy 0.20 + c4 = { path = ".." }
  src/main.rs           App setup, plugins, resources, states
  src/roster.rs         agent metadata: names, descriptions, results from results/*.csv
  src/sizes.rs          board size ladder + type erasure over Geometry (macro)
  src/engine.rs         background worker thread that owns the agents and board
  src/scene.rs          3D: board mesh with round holes, discs, lights, table, camera
  src/ui.rs             Bevy UI panels, buttons, status text, keyboard shortcuts
```

The root `c4` crate is **not changed**. The app uses only its public API:
`tournament::Entrant` (`build::<G>()`, `name()`), `board::{Board, Grid, Cell}`,
`dataset::sample`, and `random::SplitMix`.

### Type erasure over board size

Agents are generic over `G: Geometry` (sizes fixed at compile time). A macro
instantiates one `Session<Grid<W, H, Bits>>` for each size on the ladder and puts it
behind a `trait Match` object (`legal`, `play`, `choose`, `wins`, `is_full`). Sizes
up to 64 bits use `u64`, and 9×8 uses `u128`, matching `B76`/`B87`/`B98`.

### Engine thread

`Box<dyn Agent<G>>` is not `Send`, so the agents are **built inside** the worker
thread and never leave it.

- UI → worker: `Think { per_move, count }` (play up to `count` moves, each with
  `per_move` to think) or the channel is dropped (stop). The time travels with each
  request, so it can change mid-match.
- `per_move` is the agent's deadline and also the **minimum** time a move takes: a
  fast agent waits out the rest, a slow one overruns and the overrun is not carried
  over. At 1 ms every agent effectively plays as fast as it can.
- Below 10 ms per move the UI asks for about a frame's worth of moves at once (up to
  16), so fast games are not limited to one move per frame.
- Worker → UI: `Opened { moves }`, `Moved { col, millis, verdict }`, `Failed(msg)`.
- The UI asks for a move only after the previous disc has landed and the match is
  not paused. So Pause and Step work, and the animation never falls behind the game.
- Reset or a new match drops the channel. An old worker stops after its current
  think, which is at most the time per move. Messages carry a match id, so stale
  messages are ignored.

### Match settings

| Setting                  | Values                                        | Default                         |
| ------------------------ | --------------------------------------------- | ------------------------------- |
| Red agent / Yellow agent | the 8 roster agents                           | mcts vs alpha-beta-learned      |
| Board size               | the ladder above                              | 9×8 (test board)                |
| Time per move            | 1 ms, 10 ms, 50 ms, 0.1, 0.25, 0.5, 1, 2, 5 s | 1 s (as in the main tournament) |
| Opening                  | empty, random 4 stones, random 8 stones       | empty                           |

Random openings use `c4::dataset::sample` (random, no immediate wins), like the
tournament's openings. Agent seeds come from the clock, so a match is different each time.

## 3D scene

- **Board:** a custom mesh, a blue plate with a round hole in every cell (front face,
  back face, hole walls, outer rim), plus two feet. Built again on every resize.
- **Discs:** cylinders turned to face the camera. Red = first player, yellow = second.
  Animated fall (gravity, one damped bounce), sped up to 4x at short move times;
  below 0.1 s per move discs appear in place with no drop.
- **"Thinking" disc:** the next player's disc hovers and bobs above the board, with a
  progress bar for the time per move.
- **Win:** the 4+ discs in the winning line glow and pulse. A banner names the winner.
- **Camera:** orbit with right-drag, zoom with the scroll wheel, a slow idle sway, and
  automatic framing when the board size changes. Directional light with shadows,
  ambient light, a dark table.

## UI layout

```
┌────────────── Connect Four: Train on 8x7, Test on 9x8 ──────────────┐
│ RED (first)        │                              │ YELLOW (second) │
│ [ safe-random    ] │                              │ [ ...         ] │
│ [ alpha-beta-... ] │          3D board            │                 │
│  ...8 agents       │                              │  ...8 agents    │
│ ── card ──         │                              │ ── card ──      │
│ how it works       │                              │ how it works    │
│ Elo / accuracy     │                              │ Elo / accuracy  │
├────────────────────┴──────────────────────────────┴─────────────────┤
│ Board [−] 9×8 test [+]  Time [−] 1.0s [+]  Opening [empty]          │
│ [▶ Play] [⏸ Pause] [⏭ Step] [⟲ Reset]   status: Red (mcts) thinking  │
│ moves: 5 5 4 6 …                                                     │
└──────────────────────────────────────────────────────────────────────┘
```

### Series and score

- **Play** starts a series: when a match ends the winner's score goes up (or draws),
  the result shows briefly (6x the move time, 0.05 s to 1.5 s), and the next match
  starts by itself. Space on the result screen stops the series there.
- The score (Red wins : Yellow wins, draws, games) sits under the title.
- Every control works mid-match:
  - Picking a different agent **resets the score** and restarts the match.
  - Changing the board size restarts the match on the new board (score kept).
  - Time per move applies from the next move.
  - Opening applies from the next match.
- Reset stops play, clears the board and the score.

### Keyboard shortcuts (for presenting)

| Key               | Action                    |
| ----------------- | ------------------------- |
| Space             | Play / Pause              |
| S                 | Step one move             |
| R                 | Reset                     |
| `+` / `-`         | Grow / shrink the board   |
| `[` / `]`         | Less / more time per move |
| Right-drag, wheel | Orbit, zoom               |

## Results shown on agent cards

From `results/accuracy.csv` and `results/measurements.csv` (1 s per move):

| Agent                  | 9×8 Elo    | 8×7 held-out acc. | 9×8 late acc. |
| ---------------------- | ---------- | ----------------- | ------------- |
| mcts                   | 740        | 98.7%             | 99.1%         |
| alpha-beta-learned     | 722        | 98.9%             | 99.3%         |
| alpha-beta-handcrafted | 690        | 98.8%             | 99.3%         |
| network-mcts           | 647        | 98.2%             | 98.5%         |
| allis                  | 160        | 88.3%             | 89.7%         |
| perfect                | 78         | 97.8%             | 97.8%         |
| network                | 37         | 92.4%             | 91.0%         |
| safe-random            | 0 (anchor) | 76.4%             | 77.8%         |

## Build and run

```
cd presentation
cargo run --release
```

Dev builds use `opt-level = 3` for dependencies (including `c4`), so the agents are
not slowed down in debug builds.

## Acceptance checks

- `cargo build --release` and `cargo clippy` succeed with no warnings.
- The rules glue has unit tests: size ladder ↔ geometry, win-line detection, and a
  headless match that runs safe-random vs safe-random to the end on every ladder size
  (legal moves only, and the result agrees with `Board::wins`).
- Manual check: every agent pair can start on 9×8; resizing rebuilds the board; and
  Pause, Step and Reset work while a worker is thinking.
