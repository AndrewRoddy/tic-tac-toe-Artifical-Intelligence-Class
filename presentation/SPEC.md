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
- **Visuals first, words in the notes:** each slide shows only a small kicker, a
  title and the page number. Everything else that used to be body text is in the
  speaker notes (`SPEAKER_NOTES.md` and the presenter window). The visual is large and
  centred under the title: boards with dropping discs, growing bar charts, agent
  tiles, and a layer-stack picture of the network.
- **Beats, not walls of slides:** each slide is 2 to 4 beats (51 in all, about ten
  seconds each). Next reveals the slide's next piece: the next bar or group of bars,
  the next family of agents, the W/D/L labels, the network's layers, then its column
  scores. A beat can carry one short line under the visual (`Beat::caption`), which
  leaves when the next beat comes. Dots over the page number fill in per beat. Back
  steps back a beat (rebuilding the slide as it was then); from a slide's first beat
  it goes to the previous slide's last beat.
- **The growing board** (slide 2, `Visual::Expand`): one board that physically grows
  7x6, then 8x7, then 9x8, one beat each. It is the arena's morphing board
  (`scene::spawn_shaped_board` / `morph_shaped_board`): the frame slides outward while
  the new column unfolds, then the top rises while the new row grows in. The discs
  already played slide along with their columns, and a couple of new ones drop into
  the new space once it is done. The size (`7 x 6`, ...) is written large beside it
  and swaps on each beat; the last beat asks the question.
- **3D visuals, 2D text:** all words (titles, bar labels, values, captions, the W/D/L
  labels) are flat 2D text pinned to points on the stage. They follow the camera's
  drift, always face the screen, and are sized from the window height so they stay
  sharp and readable. They fade and slide in with the slide's pieces.
- **Live solve:** slide 4's 7x6 position is solved by the project's own solver in a
  background thread at startup (about 70 ms), and its W/D/L column labels are shown.
- **Keys:** Right / Space / click next beat, Left back a beat, Home first slide, Esc back to the
  arena (also after the last slide). The slides sit on a stage far from the arena table,
  so the arena's match keeps its state.
- **Transitions:** opening the deck flies the camera from the arena table to the stage.
  On every slide change the old pieces fly off the way you are going and the new ones
  sweep in from the other side, staggered; the camera settles at a slightly different
  angle for each slide and drifts gently. Each change also gives a quick camera
  push-in and a flash of the stage light, so every click has some impact.
- **Always moving** (`src/backdrop.rs`): faint discs drift down far behind everything.
  The see-through backdrop board that used to play behind the slides is hidden, so
  nothing sits behind the centred visuals.
- Dev aid: `C4_SLIDE=n` with `C4_SCREENSHOT` opens slide n for a screenshot; `a` in `C4_KEYS` returns to the arena; `>` in
  `C4_KEYS` goes one beat on (`>>>` three beats), `C4_KEYS_AT=secs` delays the keys,
  `C4_AFTER_WIN=secs` shoots that long after the first game ends, `C4_YELLOW=n`
  picks Yellow's agent.

### Presenter window and speaker notes

- Every slide has a time budget and a few lines to say per beat (`src/slides.rs`);
  the budgets add up to 10 minutes. The same notes, numbered by beat, are in
  `SPEAKER_NOTES.md`.
- **N** during the slides opens a second window (drag it to your own screen): this
  beat's lines, which step of the slide you are on, the talk clock (red past 10:00),
  time on this slide against its budget, whether you are ahead or behind plan, and
  what the next press brings (the next beat's lines, or the next slide's title). N again or
  closing the window puts it away. The projector only ever shows the slides.
- Notes are written to be read aloud, in plain spoken language, with no em dashes.
- Dev aid: `C4_NOTES_SHOT=notes.png` (with `C4_SCREENSHOT`) opens the window by itself
  and saves a screenshot of it too.

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
    entities, also grew render memory. Pooled discs are spawned with their mesh and material, keep one colour per pool
    slot (slot k is always move k), and the win glow is a halo spawned over each disc.
  - Unused pool discs are parked below the table instead of hidden.
  - After the fixes: 9 boards at 1 ms for 90 s holds about 1.3 GB at about 40 fps with
    no stalls; normal play holds about 1.6 GB.
- MSAA is off with FXAA instead; shadows use 2 cascades.
- Systems that need the window size ask for the primary window, so opening the
  presenter window cannot stall the slides or the camera.
- With more than 2 boards, or below 0.1 s per move, discs use a plain 20-sided mesh
  instead of the ridged one (hundreds of full-detail discs are too much for the GPU).
- `C4_FPS=1` logs the frame rate.
- More than 9 boards: no win beams, lights or confetti (the glow still marks wins),
  and the screen-edge flash only fires at animated speeds, at most every 0.8 s.

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

## Arena UI (2D)

- **The arena UI is flat 2D, always** (`src/ui.rs`); the 3D HUD and its toggle are
  gone. Text sizes were raised for a high-DPI laptop screen (body 15 to 19 px, values
  19 px, scores 46 px).
- **Fits any window:** the layout is drawn for 1600 x 920 and `ui::fit_ui` sets
  `UiScale` to fit the actual window (smaller when maximised on a 1080p laptop at
  125%, larger on a big projector). Slide text divides by the same scale so it keeps
  its size. F11 toggles borderless fullscreen. `C4_WINDOW=1536x816` starts at another
  size, for screenshots.
- **Shared readout** (`src/readout.rs`): every string the UI shows is computed once a
  frame from the game state.
- **The boards are never behind a panel:** the UI measures its top bar, side panels
  and bottom bar every frame and hands the sizes to the camera (`scene::UiMargins`),
  which frames the boards in the space left between them.
- **Animations:**
  - panels slide in (with a small overshoot) whenever the arena opens, including when
    coming back from the slides;
  - buttons ease their colours, spring up on hover, squash when pressed, flash and
    send out a ripple ring (in the player's colour for agent picks) when clicked;
  - a value that changes (board size, time, board count) pops and flashes orange;
  - a score that goes up bumps, and a "+1" floats up off it;
  - a win flashes the window edge in the winner's colour, and the result banner grows
    in and pulses orange;
  - switching agent slides the new agent's card in; its Elo bar eases to the new value.

## UI layout

```
 Connect Four AI              RED  2 : 1  YELLOW                   [Slides (P)]
 Trained on 8 x 7, ...     MCTS (UCT)  vs  Alpha-beta (learned)
                               draws 0  .  games 3
+------------------+                                        +------------------+
| Red . moves first|                                        | Yellow . second  |
| CHOOSE AN AGENT  |               3D board(s)              | CHOOSE AN AGENT  |
| [ 8 agents ... ] |                                        | [ 8 agents ... ] |
| card: name, how, |                                        | card ...         |
| results, Elo bar |                                        |                  |
+------------------+                                        +------------------+
+----------------------------------------------------------------------------+
| BOARD SIZE          TIME PER MOVE   BOARDS AT ONCE   STARTING POSITION      |
| [-] 9 x 8 . test [+] [-] 1 s [+]   [-] 4 [+]        [Empty board]          |
|                                                [ Play ] [One move] [Reset] |
| Red (MCTS (UCT)) is thinking...                         THINKING [=====   ] |
| Moves (4): 4 5 5 5                                                         |
| Space play/pause  S one move  R reset  - + board size  [ ] time  , . boards |
+----------------------------------------------------------------------------+
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

- **Boards at once - / +** (or `,` / `.`) steps through 1 to 9, then 12, 16, 20, 25,
  30, 36, 42, 49, 64, 81, 100, 121, 144, 196 and 256 matches between the same two
  agents. Up to 9 they stand in rows going back (one row up to 3, then 2 x 2, 3 x 2,
  3 x 3); past 9 they stack into a flat wall facing the camera, a little wider than
  tall. Each board is a table
  entity with its own `Game` and engine thread; Play, Pause, Step and Reset apply to
  all of them, and every result goes into the one score.
- With several rows going back the camera tilts down so the back rows show over the
  front ones; a wall is seen straight on. Every board runs its own engine thread, so
  hundreds of boards share the CPU and each agent gets less thinking per move.
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
