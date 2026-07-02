# idraw

A small, stylish terminal painting app. Each terminal cell holds two pixels
(half-block rendering), with a pico-8-inspired 16-color palette.

```
cargo run --release
```

Or install the `idraw` binary to `~/.cargo/bin`:

```
cargo install --path .
```

## Controls

**Mouse** — left click/drag paints, right click/drag erases, wheel changes
brush size. Click the sidebar to pick tools and colors. With the select tool,
drag to marquee a region; drag inside the selection to move its pixels.
Middle-drag pans the view.

The canvas never shrinks: if the terminal gets smaller, the drawing overflows
the visible area and you pan around it (the title shows the `@ x,y` offset).

**Keyboard**

| Key | Action |
|-----|--------|
| `b` `e` `f` `i` `m` | brush / eraser / fill / color picker / select (`Tab` cycles) |
| `1`–`8`, `[` `]` | select / cycle color |
| `+` `-` | brush size (1–4) |
| arrows + `space` | move pixel cursor and apply tool (with a selection: nudge it 1px) |
| `y` / `p` | copy selection / paste at cursor |
| `d` / `Del` | delete selection contents |
| `Esc` | deselect (quits when nothing is selected) |
| `c` | color picker: edit the selected palette slot (HSV square + hue bar; pixels drawn with that slot recolor live) |
| `u` / `r` | undo / redo |
| `x` | clear canvas |
| `Shift`+arrows | pan the view |
| `s` | save as `paintings/painting-NN.png` (8× scale, folder is gitignored) |
| `U` | self-update when new commits are on `origin` (pull + reinstall + restart) |
| `?` | help overlay |
| `q` / `Esc` | quit |
