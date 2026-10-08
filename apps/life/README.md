# Game of Life

A Wardian **page app**: Conway's Game of Life, with the whole simulation in WebAssembly. Press
**Play**, or **Step** one generation at a time. Click or drag on the board to draw cells. Load a
classic pattern, fill the board at random, or switch the rule to HighLife or Seeds.

Why a page: the world is a grid of thousands of cells, and the app needs a canvas and live
controls. A module app passes only numbers. It is a page and not a suite because it has one job,
and every panel works on the same world.

| File | What it is |
|---|---|
| `index.html` | The page: the run, world and help panels, and the board. |
| `app.js` | Plays the generations, draws the board, reads clicks, and holds the patterns. |
| `src/lib.rs` | The world: `init`, `set_rule`, `set_cell`, `get_cell`, `clear`, `randomize`, `step`, `population`, `render`. |
| `app.wasm` | Built from `src/lib.rs` by `./build.sh`. |
| `ui/` | The Wardian component library: theme, button, field, card, switch and Arrange. |

How the data moves:

- The world lives inside the module's memory. It is two grids of one byte per cell, plus a
  "heat" grid that remembers how recently each cell died.
- `step(n)` reads one grid and writes the next generation into the other, then swaps them. It
  does this `n` times in one call and returns the population. At high speed the page asks for
  many generations per frame, so JavaScript never touches a cell in the loop.
- `render(ptr, live, dead, glow, trail)` writes one RGBA pixel per cell into a buffer the page
  reserved once with `alloc`. The page reads `memory.buffer` again after the call, puts the
  pixels on a small canvas, and scales it up onto the board without smoothing.
- A click or drag calls `set_cell` for each cell on the line, so a fast drag leaves no gaps.

The edges wrap around: a glider that leaves on the right comes back on the left.

Patterns: glider, lightweight spaceship, Gosper glider gun, pulsar, R-pentomino, acorn, and the
HighLife replicator. They are written in RLE, the text format Life programs share, so adding
one to `PATTERNS` in `app.js` takes one line.

Motion: the board starts playing when it opens. If your system asks for less motion
(`prefers-reduced-motion`), it starts paused and says why.

Limits:

- The largest grid is 640 × 400 cells. A bigger grid is possible (`init` allows 2000 × 2000), but
  the cells become too small to draw with the mouse.
- The page cannot keep anything between visits. Your drawing ends when you close the page.

The panels are Arrange panels (`data-panel`): each viewer may reorder them, move them between the
two columns, or hide them.
