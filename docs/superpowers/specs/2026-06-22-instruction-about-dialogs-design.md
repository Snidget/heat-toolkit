# Instruction and about dialogs design

## Goal

Separate page-specific instructions from global application information.

## Design

- `InstructionDialog` contains only the instruction text for the current page
  and accepts custom text and size for future pages.
- `AboutDialog` contains only application information:
  - `Версия 1.4`
  - `Разработчик:`
  - `Михаил Трусов`
  - `Институт пассивного дома`
- `TurnerPage` owns its `Инструкция` button, so future pages can provide their
  own instructions independently.
- `MainWindow` owns the bottom navigation button `О программе`, because this is
  global application information.
- The current turner instruction dialog is `400 x 245`.
- The global about dialog is `260 x 130`.

## Verification

- Run existing pytest tests.
- Run an offscreen UI smoke test for the new buttons and dialogs.
- Rebuild the executable.

## Repository Note

This workspace folder is not a git repository, so this design note cannot be
committed here.
