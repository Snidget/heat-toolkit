# Left tabs UI design

## Goal

Prepare the application for future feature pages by adding a left navigation
menu with horizontal text.

## Design

- `MainWindow` owns the application window, a fixed-width left navigation menu,
  and a `QStackedWidget`.
- The existing coordinate transformer UI is moved into `TurnerPage`.
- The first and only navigation item is named `Поворотник`.
- The `TurnerPage` working area keeps its old 320 px width.
- The window becomes wider only by the navigation menu width, while height stays
  the same:
  - old: 320 x 600
  - new: 460 x 600

## Boundaries

- The current transform logic, clipboard behavior, preview widget, and info
  dialog remain functionally unchanged.
- Future feature pages can be added as separate widgets without expanding
  `MainWindow`.

## Verification

- Run the existing pytest suite.
- Run Python compile checks for UI modules.
- Rebuild the executable.

## Repository Note

This workspace folder is not a git repository, so this design note cannot be
committed here.
