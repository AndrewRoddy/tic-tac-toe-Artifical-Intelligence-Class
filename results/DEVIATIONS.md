# Deviations and Gaps

## Deviations from the plan

1. **No opening book for the solver agent (A1).** Two 6-stone 8x7 positions each took more than 1,800 s to solve on one core. A book from the 2,000 test openings puts test positions into the results. The book is for 8x7 only, so no 9x8 result changes.

2. **Solver matches start at 16-20 stones.** The plan uses the 10-12 stone opening set. From one of these openings, the solver did not complete its first move in 30 minutes. The matches use 200 held-out positions instead. Each agent plays each position once on each side.

3. **Both MCTS agents (B1, B3) use a proven-result rule.** A move that wins at once gets a known win. A move that lets the opponent win at once gets a known loss. Plain UCT does not do this. B1 accuracy changed by less than its 95% interval.

4. **The Allis agent (A4) uses a subset of the rules.** It uses win, block, the safe-move filter, the claimeven follow-up, and threat creation on good rows. The other Allis rules are not in the agent (baseinverse, vertical, aftereven, lowinverse, highinverse, baseclaim, before, specialbefore).

5. **The network (B2) trained on the CPU.** The laptop GPU failed during training. The final model trained for 12 epochs on the CPU.

6. **The training set has 106,898 positions with 14-44 stones.** The plan gives about 100,000. A band of 6,898 positions with 14-15 stones was added. Every 10th line is held out.

7. **The tournament also ran at 0.5 s, 5 s, and 10 s per move.** The plan gives 1 s only. The 1 s run is the main result.

## Gaps in the data

1. **Solver matches: 221 results are unknown.** In these games, the agent had a theoretical win and did not win. The log does not show a draw or a loss. In `games.csv`, `agent_result` and `first_result` are empty for these rows.

2. **Solver matches: moves are not recorded.** The `moves` and `plies` columns are empty for all 3,200 solver-match rows.

3. **Accuracy is in summary form only.** `accuracy.csv` gives one row for each game phase. Results for each position were not kept.

4. **Some studies have no raw files.** The values come from run reports, not from saved files. These studies are the depth study, the depth duels, the Allis ablation, the MCTS budgets, and the network sweep.

5. **Network sweep values cannot be reproduced.** The sweep ran before the weight start values were fully seeded. It also used an older data split, before the 14-15 stone band was added. The sweep is valid only to compare network shapes, not as a final result.

6. **The final network is reproducible at the weight level only.** Two runs give the same weights. The saved files are not byte-identical, because Burn writes a new ID for each run.

7. **MCTS budget rows come from two code versions.** The `notes` column gives "before" or "after" the proven-result rule. Rows from the two versions are not comparable.

8. **The hardest positions are slightly under-represented.** The opening set skipped 17 positions that hit the 3,600 s limit. The 9x8 late set dropped 10 positions with 24-27 stones for the same reason.
