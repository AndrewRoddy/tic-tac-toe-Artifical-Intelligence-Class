# Speaker notes

Ten minutes in total. The same notes (and a timer) show in the presenter
window: press N while presenting to open it on your own screen.

## 1. Connect Four AI  (20 s, at 0:00)

Hi, we're TJ and Andrew, group 3. Our project: train Connect Four agents on an 8x7 board, then test them on a bigger 9x8 board that nobody has solved. Everything you'll see, including this deck, runs live in our Rust code.

## 2. A board nobody has solved  (50 s, at 0:20)

The classic 7x6 board is solved: with perfect play the first player wins. John Tromp also solved 8x7, and again the first player wins. 9x8 has no published solution; on our hardware it would take around fifteen years. So the question is: can agents trained on a board we CAN solve play well on one we can't? And why do some methods transfer and others not?

## 3. How big is the game?  (35 s, at 1:10)

To give a sense of scale: 7x6 has about 4.5 trillion legal positions. 8x7 has up to 10 to the 26 cell patterns, and 9x8 up to 10 to the 34. Each step up is millions of times bigger, which is why brute force stops working.

## 4. An exact solver  (50 s, at 1:45)

Step one was an exact solver: alpha-beta search on bitboards with a transposition table, like published Connect Four solvers. It labels every column as a win, draw or loss. We checked it on all 67,557 UCI positions and 6,000 of Pons' test positions with zero mismatches. The labels on the right were computed live when this app started.

## 5. Why not just solve 9x8?  (45 s, at 2:35)

So why not just solve 9x8? Solve time explodes as the board gets emptier. Late positions take a tenth of a second; at 24 stones the median is 12 seconds and the worst took 31 minutes; at 16 stones every one of our probes ran past ten minutes. Even 8x7 with six stones took over half an hour. That wall is the whole motivation for learning.

## 6. The data  (35 s, at 3:20)

We used the solver to build the data: about 107 thousand 8x7 positions from 14 to 44 stones, every column labelled; 2,000 harder opening positions for testing; and 10 thousand late 9x8 positions we could still solve exactly, for checking transfer. We ran one core per position on a 192-vCPU cloud machine.

## 7. Eight agents  (30 s, at 3:55)

Then we built eight agents across four families: search, learning, hand-written rules, and the exact solver, plus a safe-random baseline that takes wins and blocks threats. Each one gets one second per move.

## 8. Alpha-beta: how deep?  (45 s, at 4:25)

First, alpha-beta with a handcrafted evaluation of threats and row parity. Depth 1 already matches the solver 91% of the time; depth 8 gets 97.5%, and depth 12 only adds a little more while searching 35 times more nodes. On 9x8, the deeper search still wins about two games in three, so depth keeps paying off in real games.

## 9. Learning the evaluation  (40 s, at 5:10)

Then we learned the evaluation instead of hand-tuning it: logistic regression on the same threat features, fit to the solver's 8x7 labels. It predicts held-out results better than the handcrafted weights, 70.1% against 67.8%, and on 9x8 it ranks higher too: Elo 722 against 690.

## 10. Monte Carlo tree search  (45 s, at 5:50)

Monte Carlo tree search needs no evaluation at all: it plays random games to the end, about 740 thousand a second on 9x8, and grows a tree toward the moves that win. We added a proven-result rule so it never misses an instant win or loss. It is the strongest agent, and it keeps getting stronger with more time.

## 11. A network for any board  (50 s, at 6:35)

The neural network is fully convolutional, so the same weights run on any board size. We trained it on 8x7 labels, on the CPU after the GPU failed. It matches the solver on 92.4% of 8x7 moves and still 91.0% on 9x8, so the knowledge transfers. But one wrong move in a game is enough to lose, so on its own it is weak; used to guide MCTS it reaches Elo 647.

## 12. Victor Allis' strategy  (35 s, at 7:25)

We also coded part of Victor Allis' strategy rules, with no search. They beat safe random 83% of the time on 8x7 and 78% on 9x8, so the ideas carry over. Interestingly, removing the claimeven rule didn't hurt. Our subset of the rules is just too small to play strong games.

## 13. The 9x8 tournament  (45 s, at 8:00)

The main result: a round robin on 9x8, every pair, 50 random openings, both colours, one second a move, 2,800 games. MCTS comes first at Elo 740, then the learned and handcrafted alpha-betas, then network-guided MCTS. The rules, the time-limited solver and the network alone are far behind.

## 14. Why it came out this way  (50 s, at 8:45)

Why? Search wins on a board you can't solve. The exact solver is perfect, but it can't finish a 9x8 position in one second, so it falls back to playing near the centre and lands at Elo 78. Learned knowledge transfers well, but without search one mistake loses the game. Search with learned knowledge on top fixes most of that. The chart shows the same story on 8x7, against the solver itself.

## 15. Now watch them play  (25 s, at 9:35)

Now let's watch them play. I'll pick two agents and a board size, and you can see the real agents from our results playing live, including on the 9x8 test board.
