"""Import production STEP fixtures through Open CASCADE and measure them."""

from __future__ import annotations

import math
import sys
from pathlib import Path

from OCP.BRepBndLib import BRepBndLib
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.Bnd import Bnd_Box
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_SOLID
from OCP.TopExp import TopExp_Explorer


def import_dimensions(path: Path) -> tuple[tuple[float, float, float], tuple[float, float, float]]:
    reader = STEPControl_Reader()
    status = reader.ReadFile(str(path))
    if status != IFSelect_RetDone:
        raise RuntimeError(f"Open CASCADE could not read {path}: status {status}")

    transferred_roots = reader.TransferRoots()
    if transferred_roots < 1:
        raise RuntimeError(f"Open CASCADE transferred no STEP roots from {path}")

    shape = reader.OneShape()
    if shape.IsNull():
        raise RuntimeError(f"Open CASCADE returned a null shape for {path}")
    if not BRepCheck_Analyzer(shape).IsValid():
        raise RuntimeError(f"Open CASCADE reports invalid B-rep topology for {path}")

    solids = TopExp_Explorer(shape, TopAbs_SOLID)
    solid_count = 0
    while solids.More():
        solid_count += 1
        solids.Next()
    if solid_count != 1:
        raise RuntimeError(f"Expected one imported solid in {path}, got {solid_count}")

    bounds = Bnd_Box()
    bounds.SetGap(0.0)
    BRepBndLib.Add_s(shape, bounds)
    x_min, y_min, z_min, x_max, y_max, z_max = bounds.Get()
    return (
        (x_max - x_min, y_max - y_min, z_max - z_min),
        (x_min, y_min, z_min),
    )


def assert_dimensions(path: Path, expected: tuple[float, float, float], expected_origin: tuple[float, float, float]) -> None:
    actual, origin = import_dimensions(path)
    tolerance_mm = 0.001
    for axis, (measured, wanted) in enumerate(zip(actual, expected, strict=True)):
        if not math.isclose(measured, wanted, rel_tol=1e-9, abs_tol=tolerance_mm):
            raise AssertionError(
                f"{path.name}: axis {axis} measured {measured:.12g} mm, expected {wanted:.12g} mm"
            )
    for axis, (measured, wanted) in enumerate(zip(origin, expected_origin, strict=True)):
        if not math.isclose(measured, wanted, rel_tol=1e-9, abs_tol=tolerance_mm):
            raise AssertionError(
                f"{path.name}: origin axis {axis} measured {measured:.12g} mm, expected {wanted:.12g} mm"
            )
    print(f"Open CASCADE imported {path.name}: dimensions={actual} mm, origin={origin} mm")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: verify_step.py <fixture-directory>", file=sys.stderr)
        return 2

    fixture_directory = Path(sys.argv[1])
    assert_dimensions(
        fixture_directory / "building-scale.step",
        (1000.0, 500.0, 100.0),
        (0.0, 0.0, 0.0),
    )
    assert_dimensions(
        fixture_directory / "thin-feature-large-offset.step",
        (0.1, 100.0, 100.0),
        (1_000_000.0, 2000.0, 3000.0),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
