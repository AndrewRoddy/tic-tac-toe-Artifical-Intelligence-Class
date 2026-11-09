# Speaker notes

Ten minutes in total. The slides only show a title and a picture, so everything
worth saying is here. Each numbered step is one press of Next (right arrow, space
or click): it brings in the next piece of the picture, and is about ten seconds of
talking. The presenter window (press N) shows the current step and the next one.

## 1. Connect Four AI  (25 s, starts at 0:00)

1. Hey everyone, we're TJ and Andrew from group 3, and our project is Connect Four.
2. We trained agents on an 8 by 7 board and tested them on 9 by 8, which nobody has solved. Everything you'll see today, these slides included, is running live in our Rust code.

## 2. A board nobody has solved  (50 s, starts at 0:25)

1. This is the normal 7 by 6 board. It's solved, and if both sides play perfectly the first player wins.
2. Add a column and a row and you get 8 by 7. John Tromp solved this one too, also a first player win. This is where we trained.
3. One more of each and you're at 9 by 8. Nobody has solved it. We worked out it would take our hardware something like 15 years.
4. So our question was pretty simple. If we train agents on a board we can solve, do they still play well on one we can't? And if some methods carry over better than others, why?

## 3. How big is the game?  (30 s, starts at 1:15)

1. Just to give you a feel for the size. 7 by 6 has about 4.5 trillion legal positions.
2. For 8 by 7 the upper bound is around 10 to the 26.
3. And 9 by 8 is around 10 to the 34. This is a log scale, so every step up is millions of times bigger. You just can't brute force that.

## 4. An exact solver  (40 s, starts at 1:45)

1. Step one was an exact solver. It's alpha-beta search on bitboards with a transposition table, which is pretty much how the published Connect Four solvers work.
2. Give it any position and it tells you whether each column wins, draws or loses. These W, D and L labels were worked out live when the app started up.
3. We checked it against all 67,557 positions in the UCI dataset and 6,000 of Pascal Pons' test positions, and it got every single one right.  
   *On screen: 67,557 UCI positions  ·  6,000 Pons tests  ·  0 wrong*

## 5. Why not just solve 9x8?  (45 s, starts at 2:25)

1. So why didn't we just solve 9 by 8? Late in the game it's quick, about a tenth of a second.
2. But solve time blows up as the board gets emptier. At 24 stones the median is around 12 seconds, and the worst one we saw took 31 minutes.
3. At 16 stones every probe we ran went past ten minutes.
4. Even 8 by 7 with only six stones took more than half an hour. That wall is really the reason we needed learning at all.  
   *On screen: 8x7 with just 6 stones: over half an hour*

## 6. The data  (35 s, starts at 3:10)

1. We used the solver to build our data. About 107 thousand 8 by 7 positions between 14 and 44 stones, with every column labelled.
2. We also made 2,000 harder openings, around 10 to 12 stones, just for testing.
3. And we solved about 10 thousand late game 9 by 8 positions, so we could check how well things transfer. All of that ran on a 192 core cloud machine.  
   *On screen: 192 cores, one position each*

## 7. Eight agents  (30 s, starts at 3:45)

1. Then we built eight agents. First the search ones: MCTS, alpha-beta, and the exact solver itself.
2. Then the ones that learn: a learned evaluation, a neural network, and the network guiding MCTS.
3. And Allis' hand written rules, plus a safe random baseline that plays randomly but always takes a win and blocks a threat. Every agent gets one second per move.

## 8. Alpha-beta: how deep?  (45 s, starts at 4:15)

1. First up is alpha-beta with a handcrafted evaluation that looks at threats and row parity. Even at depth 1 it agrees with the solver about 91 percent of the time.
2. Every extra bit of depth helps a little.
3. Depth 8 gets to 97.5. Depth 12 barely adds anything, and it looks at 35 times as many nodes, around 380 thousand a move.
4. But in real games on 9 by 8 the deeper search still wins about two out of three, so the extra depth does pay off.  
   *On screen: but deeper still wins 2 of 3 games on 9x8*

## 9. Learning the evaluation  (40 s, starts at 5:00)

1. Next we tried learning the evaluation instead of tuning it by hand. Just guessing the most common result gets 56.6 percent.
2. Our hand tuned evaluation gets 67.8.
3. We fit a logistic regression on the same threat features, split into good rows and bad rows, using the solver's 8 by 7 labels. It gets 70.1, and it holds up on 9 by 8 too, 722 Elo against 690.  
   *On screen: on 9x8: Elo 722 learned vs 690 handcrafted*

## 10. Monte Carlo tree search  (45 s, starts at 5:40)

1. Monte Carlo tree search doesn't need an evaluation function at all. It plays a ton of random games to the end, about 740 thousand a second on 9 by 8.
2. It grows its tree toward whichever moves win the most. We added a rule for proven results, so it never misses a win or a loss that's right in front of it.  
   *On screen: + proven wins and losses*
3. At one second a move it ended up being our strongest agent.
4. And it keeps getting better the more time you give it.

## 11. A network for any board  (50 s, starts at 6:25)

1. For the neural network we went fully convolutional, so the exact same weights can run on any board size.
2. It's 6 layers with 64 channels. We trained it for 12 epochs on the CPU because our GPU setup failed.
3. Out comes a score for each column. It matches the solver on 92.4 percent of 8 by 7 moves and still 91 percent on 9 by 8, so what it learned really does transfer.  
   *On screen: 92.4% of 8x7 moves  ·  91% of 9x8 moves*
4. The catch is that one bad move can lose a whole game, so on its own it's pretty weak. When we use it to guide MCTS it gets to an Elo of 647.  
   *On screen: alone: weak  ·  guiding MCTS: Elo 647*

## 12. Victor Allis' strategy  (40 s, starts at 7:15)

1. We also coded up part of Victor Allis' strategy rules, with no search at all. Things like win, block, play safe moves, claimeven, and make threats on good rows.  
   *On screen: win  ·  block  ·  play safe  ·  claimeven  ·  threats on good rows*
2. On 9 by 8 it beats safe random 78 percent of the time, and 83 on 8 by 7, so the ideas do carry over.
3. Funny enough, turning off claimeven didn't hurt it at all.
4. Flipping the parity hurt a little. Our set of rules is just too small to play a strong game.

## 13. The 9x8 tournament  (45 s, starts at 7:55)

1. This is the main result. We ran a round robin on 9 by 8 where every pair of agents played 50 random openings, both colours, one second a move. That's 2,800 games, with safe random pinned at an Elo of 0.  
   *On screen: every pair  ·  50 openings  ·  both colours  ·  2,800 games*
2. Down at the bottom: the rules, the time limited solver, and the network on its own.
3. Then network guided MCTS, and the handcrafted alpha-beta.
4. And at the top, the learned alpha-beta at 722, and MCTS at 740.

## 14. Why it came out this way  (50 s, starts at 8:40)

1. So why did it turn out like this? Basically, search wins on a board you can't solve. The exact solver is perfect when it finishes, but it can't finish a 9 by 8 position in one second, so it falls back to playing near the middle.
2. Learned knowledge transfers well, but without search one mistake costs you the game.
3. Put search on top of that knowledge and most of the problem goes away.
4. This chart is the same story on 8 by 7, measured against the solver itself.  
   *On screen: search + knowledge wins*

## 15. Now watch them play  (30 s, starts at 9:30)

1. Alright, let's actually watch them play.
2. I'll pick two agents and a board size. These are the real agents from our results, playing live, and we can even put them on the 9 by 8 test board.
