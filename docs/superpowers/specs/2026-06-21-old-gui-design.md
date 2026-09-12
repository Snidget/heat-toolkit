# Old GUI restoration design

## Goal

Restore the visible interface of the modular V2 application so it matches
`Исходники/HEAT_GUI_and_RenderV5.py`, while keeping the current package
structure and safer V2 parser/transform modules.

## Scope

- Main window size: 320 x 600.
- Preview: 300 x 200 minimum, 200 maximum height.
- Layout: the same simple vertical Qt layout as the old monolithic version.
- Controls: the same button order, labels, button colors, and info button
  placement as the old version.
- Architecture: keep `main.py`, `src/ui/*`, `src/parser.py`,
  `src/transforms.py`, and `src/config.py` separated.
- Functional check: compare every transform in V2 with the behavior in
  `HEAT_GUI_and_RenderV5.py` on representative input.

## Non-goals

- No rewrite back to the old monolithic file.
- No unrelated refactoring.
- No behavioral changes beyond restoring old-version equivalence.

## Verification

- Run Python compile checks for changed modules.
- Run a transform comparison script against the old formulas.
- Launch check is optional because the GUI is desktop-only and may need an
  interactive display.

## Repository note

This workspace folder is not a git repository, so the design note cannot be
committed here.
