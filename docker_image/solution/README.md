# filler solution

A Rust bot for the 42 `filler` project.

## Build

```
cargo build --release
```

The binary ends up at `target/release/filler`.

## Test

```
cargo test
```

Unit tests live in `src/lib.rs`, next to the code they cover: input
parsing, placement validation, boundary conditions, and the answer
output format.

## Run against the game_engine

From `docker_image/`:

```
./linux_game_engine -f maps/map01 -p1 solution/target/release/filler -p2 robots/bender
```

## Approach

`src/lib.rs` holds all the game logic (grid/piece parsing, placement
validation, move selection) so it can be unit tested directly. `src/main.rs`
is just the stdin/stdout loop: the engine spawns the bot once per game and
keeps the pipe open, sending one Anfield+Piece block per turn.

Each turn, every legal placement (exactly one cell overlapping our own
territory, none on the opponent's, fully in bounds) is scored by distance
from existing territory - growing outward on big open maps to claim space
first, but growing compactly on small maps where spreading out just leaves
us stuck in a corner. Placements that would wall us into a pocket too small
to keep growing in are filtered out via a bounded flood fill before picking
the best-scoring one left.
