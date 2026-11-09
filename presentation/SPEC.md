# Connect Four Arena 3D — Presentation App Spec

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

## Presentation mode

- **present (p)** in the top-right corner (or the P key) opens a 15-slide deck that walks
  through the project: the question, the scale, the solver, the data, each agent family,
  the tournament, and why the results came out as they did. Every number is from
  `results/` or `midterm_proposal.md`; the content lives in `src/slides.rs`.
- **All 3D:** the 2D UI is hidden; titles, body text, labels and numbers are extruded
  meshes built from the Windows fonts' glyph outlines (`meshtext`, `src/text3d.rs`).
  Visuals are 3D too: boards with dropping discs, growing bar charts, agent tiles, and
  a layer-stack picture of the network.
- **Live solve:** slide 4's 7x6 position is solved by the project's own solver in a
  background thread at startup (about 70 ms), and its W/D/L column labels are shown.
- **Keys:** Right / Space / click next, Left back, Home first slide, Esc back to the
  arena (also after the last slide). The slides sit on a stage far from the arena table,
  so the arena's match keeps its state.
- **Transitions:** opening the deck flies the camera from the arena table to the stage.
  On every slide change the old pieces fly off the way you are going and the new ones
  sweep in from the other side, staggered; the camera settles at a slightly different
  angle for each slide and drifts gently. Each change also gives a quick camera
  push-in and a flash of the stage light, so every click has some impact.
- **Always moving** (`src/backdrop.rs`): a see-through 7x6 board floats low behind the
  slides playing a real game (MCTS vs learned alpha-beta through the engine thread,
  from a random 4-stone opening); the winning line glows, then it starts again. It
  glides to a new spot on each slide. Faint discs drift down far behind everything.
- Dev aid: `C4_SLIDE=n` with `C4_SCREENSHOT` opens slide n for a screenshot; `a` in `C4_KEYS` returns to the arena; `>` in
  `C4_KEYS` goes to the next slide, `C4_KEYS_AT=secs` delays the keys,
  `C4_AFTER_WIN=secs` shoots that long after the first game ends, `C4_YELLOW=n`
  picks Yellow's agent.

### Presenter window and speaker notes

- Every slide has speaker notes and a time budget (`src/slides.rs`); the budgets add
  up to 10 minutes. The same notes are in `SPEAKER_NOTES.md`.
- **N** during the slides opens a second window (drag it to your own screen): this
  slide's notes, the talk clock (red past 10:00), time on this slide against its
  budget, whether you are ahead or behind plan, and the next slide's title. N again or
  closing the window puts it away. The projector only ever shows the 3D slides.

## Robustness and performance

Measured on the presentation laptop (Intel Iris Xe, 2400 x 1350 at 150% scaling):

- Bevy 0.20's GPU light clustering made the Intel driver lose the device (a crash)
  when games restart many times a second; it is turned off (CPU clustering is free
  with our few lights).
- **Memory leaks found and fixed** (they caused stalls, then a lost device, after a
  minute or two, even idle):
  - Bevy's `system_font_discovery` leaked on every 2D text change (hundreds of MB a
    second in fast games). It is off; the 2D UI loads Segoe UI and Times New Roman
    from the Windows font folder into font assets at startup instead.
  - Adding a mesh to an entity after it was spawned, and swapping materials on live
    entities, also grew render memory. HUD labels spawn a fresh child per change;
    pooled discs are spawned with their mesh and material, keep one colour per pool
    slot (slot k is always move k), and the win glow is a halo spawned over each disc.
  - Unused pool discs are parked below the table instead of hidden.
  - After the fixes: 9 boards at 1 ms for 90 s holds about 1.3 GB at about 40 fps with
    no stalls; normal play holds about 1.6 GB.
- 3D text meshes are cached with a bound (600), and HUD labels re-mesh at most about
  12 times a second.
- MSAA is off on both cameras with FXAA instead (36 -> 29 ms a frame with the HUD);
  small HUD text is a single front face; shadows use 2 cascades.
- With more than 2 boards, or below 0.1 s per move, discs use a plain 20-sided mesh
  instead of the ridged one (hundreds of full-detail discs are too much for the GPU).
- `C4_FPS=1` logs the frame rate. The 2D backup mode (U) is the fastest setting.

## 3D scene

- **Board:** a custom mesh, a blue plate with a round hole in every cell (front face,
  back face, hole walls, outer rim), plus two feet. Built again on every resize.
- **Discs:** cylinders turned to face the camera. Red = first player, yellow = second.
  Animated fall (gravity, one damped bounce), sped up to 4x at short move times;
  below 0.1 s per move discs appear in place with no drop.
- **"Thinking" disc:** the next player's disc hovers and bobs above the board, with a
  progress bar for the time per move.
- **Win** (`src/effects.rs`): the 4+ winning discs glow and pulse, pop forward, a
  glowing beam draws itself through them, confetti bursts out in the winner's colours
  and a coloured light flashes. A banner names the winner and their score number
  bumps. The effects (not the glow) are skipped below 0.1 s per move.
- **Quality:** pieces are lathed meshes like the real ones (raised outer rim, a
  recessed face with two concentric ridges, rounded edges) in glossy clear-coated
  plastic; the board's holes are bevelled on both faces; the camera has soft bloom so
  glows, the win beam and flashes shine. Used in the arena and on the slides.
- **Landing:** each disc gives the board a small rock back when it lands.
- **Resize morph:** arena boards are one tile per cell plus a stretchable frame. On a
  resize the frame slides outward while new columns unfold, then the top rises while
  new rows grow in (shrinking runs height first, then width), about 1.3 s. Agents wait
  for the morph before moving; with several boards each one glides to its new spot.
- **Camera:** orbit with right-drag, zoom with the scroll wheel, a slow idle sway, and
  automatic framing when the board size changes. Directional light with shadows,
  ambient light, a dark table.

## 3D HUD and backup mode

- **The arena UI is 3D by default** (`src/hud.rs`): every button, label, number and
  bar is a mesh (extruded text, blocks), drawn by a second camera on render layer 1 so
  the scene never covers it and the orbit camera never moves it. It mirrors the flat
  layout below. Buttons are picked with Bevy's mesh picking: they lift and tilt toward
  you on hover, sink when pressed, and on click spring back with a flash and a ring of
  light. Selected agents sit raised in their player's colour; the score numbers jump.
- **Shared readout** (`src/readout.rs`): every string both UIs show is computed once a
  frame, so they always agree.
- **Backup mode:** U, or the "2D UI" / "3D UI" button, switches to the flat Ember UI
  (`src/ui.rs`) and also turns off bloom and shadows for slower machines.
  `C4_LITE=1` starts in backup mode. The slides are always 3D.

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
  the result shows briefly (6x the move time, 0.1 s to 1.5 s), and the next match
  starts by itself. Space on the result screen stops the series there.
- The score (Red wins : Yellow wins, draws, games) sits under the title.
- Every control works mid-match:
  - Picking a different agent **resets the score** and restarts the match.
  - Changing the board size restarts the match on the new board (score kept).
  - Time per move applies from the next move.
  - Opening applies from the next match.
- Reset stops play, clears the board and the score.

### Several boards at once

- **boards - / +** (or `,` / `.`) runs 1 to 9 matches side by side between the same two
  agents, in a grid (one row up to 3, then 2 x 2, 3 x 2, 3 x 3). Each board is a table
  entity with its own `Game` and engine thread; Play, Pause, Step and Reset apply to
  all of them, and every result goes into the one score.
- With several rows the camera tilts down so the back rows show over the front ones.
- The status line summarises all boards; the move list shows board 1. The hovering
  "thinking" disc is shown only with a single board.

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
