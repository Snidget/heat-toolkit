# BC box enable transform design

## Goal

When transforming HEAT3 `b` lines for BC boxes, update the `%enable=XXXXXX`
face mask together with the box coordinates.

## Mask Order

The six bits are ordered as:

```text
x1 x2 y1 y2 z1 z2
```

For example, `%enable=110111` means `y1` is disabled.

## Behavior

- Only `b` lines are treated as BC boxes.
- Only masks matching `%enable=` followed by exactly six `0` or `1` digits are
  changed.
- The rest of the line, including numeric extra values and comments such as
  `! BC box`, is preserved by the existing parser/serializer.
- Each GUI transform has a matching mask permutation with the same coordinate
  field mapping used by `src/transforms.py`.

## Example

For `swap_xy_xz`, old `y1` becomes new `z1`, so:

```text
%enable=110111 -> %enable=111101
```

## Repository Note

This workspace folder is not a git repository, so this design note cannot be
committed here.
