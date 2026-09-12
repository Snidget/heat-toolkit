# Material Sort Page Design

## Scope

Add a second left-menu page for sorting only `p ... ! material box` lines.
Other script lines stay in their original positions.

## Data Flow

- `ScriptState` stores one shared script text for all pages.
- `TurnerPage` writes transformed script text back to `ScriptState`.
- `MaterialSortPage` reads the same `ScriptState`, so a script pasted on one page is available on the other page.

## Material List

- Materials are extracted from parsed `p` lines whose trailing text contains `! material box`.
- The material name is the text between the numeric tokens and `! material box`.
- Duplicate materials are shown once; per-material counts are not displayed in list rows.
- Each row has two logical text lines: material name, then thermal conductivity.
- Unknown conductivity is displayed as `Теплопроводность: нет данных`.
- Up/down controls are compact, stacked vertically on the right side of the row to keep long names from causing horizontal scrolling.
- Manual up/down controls reorder the material list and immediately apply that order to matching `p` material box lines.

## MTL Reading

- `.mtl` files use 89-byte records based on `MTL_parserV3.py`.
- Material names are decoded as `windows-1251`.
- Thermal conductivity numeric fields are decoded as ASCII.
- Sorting by conductivity uses `thermal_x`.
- Script/material matches use normalized names: trimmed whitespace, collapsed internal whitespace, and case-insensitive comparison.

## Preservation Rules

- Sorting moves only material box line contents across material box slots.
- Non-material lines, slot positions, tabs, spaces, and line endings are preserved.
- Unknown materials are kept after matched materials in their existing relative order.
