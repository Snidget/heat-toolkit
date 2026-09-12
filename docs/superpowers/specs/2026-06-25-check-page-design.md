# Check Page

## Goal

Add a `Проверка` page that reports how many unique HEAT3 planes are used by the current 3D model along each axis.

## Data Source

The page uses the shared 3D `ScriptState`, the same buffer used by `Поворотник` and `Сортировка`. If the script was already pasted on one of those pages, the check page immediately shows the same model. Pasting on `Проверка` updates the shared 3D buffer.

## Counting Rule

The checker parses all valid 3D object lines supported by the existing parser: `p`, `b`, and `e`.

For every parsed object:

- add `x1` and `x2` to the X plane set;
- add `y1` and `y2` to the Y plane set;
- add `z1` and `z2` to the Z plane set.

Touching objects that share the same coordinate reuse the same plane. Invalid or unsupported lines are ignored.

## Limit

HEAT3 allows up to 150 unique planes per axis. The UI displays:

```text
X: N из 150
Y: N из 150
Z: N из 150
Объектов учтено: N
```

If any axis exceeds 150, the result text is shown in warning styling.

## UI

The page contains:

- `Вставить данные из буфера обмена`
- current plane usage information
- a short hint about the 150-plane limit
- internal cavity check information

## Internal Cavity Check

The checker uses coordinates from valid `p`, `b`, and `e` objects to construct
the grid. Its material occupancy follows HEAT3 script semantics: `p` fills a
material volume, `e` cuts material out, and `b` contributes boundary surfaces
without filling volume. Overlapping `p`/`e` operations are applied in script
order, so the last object at a cell prevails.

Algorithm:

1. Build sorted unique X, Y, and Z plane coordinates from all positive-volume boxes.
2. Treat the space between adjacent planes as a discrete 3D cell grid.
3. Apply `p` and `e` operations in script order to mark material and empty
   cells; do not mark `b` boxes as occupied.
4. Flood-fill empty cells connected to the outer boundary of the model bounding grid.
5. Any remaining empty cells are internal cavities.

The UI displays:

```text
Пустоты: не обнаружены
```

or:

```text
Пустоты: обнаружено N областей
1. X x1..x2, Y y1..y2, Z z1..z2
```

If the discretized grid exceeds the safety limit, the cavity check is skipped and the UI shows a warning. Plane counting still runs in that case.
