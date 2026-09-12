# 2D Turner Page

## Goal

Add a separate `2D Поворотник` page for 2D HEAT script fragments without changing the modular project structure or the existing 3D script buffer.

## Script Format

Supported transformable lines start with `r` or `R` and contain four coordinates. The original
command-letter case is preserved:

```text
r x1 y1 x2 y2 material name
```

Decimal numbers may use either comma or dot as a separator on input. Transformed output always uses dot as the decimal separator. The parser treats the four numeric fields as rectangle bounds. Any non-rectangle line, malformed line, indentation, tab spacing, material tail, line order, and line endings must be preserved.

## Buffer Ownership

The 2D page owns a dedicated `ScriptState`. The 3D `Поворотник` and `Сортировка` pages keep sharing the existing 3D script state.

## Controls

The page includes only the controls needed for the 2D workflow:

- `Вставить данные из буфера обмена`
- `Инструкция`
- `Поворот по часовой стрелке`
- `Поворот против часовой стрелки`
- `Отражение по X`
- `Отражение по Y`
- `Копировать данные в буфер обмена`

There is no XY/XZ projection selector on the 2D page.

## Transform Rules

- Clockwise rotation: `(x, y) -> (y, -x)`
- Counterclockwise rotation: `(x, y) -> (-y, x)`
- Mirror X: `(x, y) -> (x, -y)`
- Mirror Y: `(x, y) -> (-x, y)`

After every operation rectangle bounds are normalized back to `x1 <= x2` and `y1 <= y2`.
