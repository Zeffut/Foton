#!/usr/bin/env python3
"""Draw the website's voxel scenes with the mark's own renderer.

Rasterised at five display pixels per source pixel, the scale the mark itself
is published at -- a 64px grid rendered to a 320px PNG -- so a pixel in a scene
is the size of a pixel in the logo beside it. The cube edge is larger than the
mark's 10: that number is the largest edge fitting a 64px icon canvas, and
nothing here has to fit one.

    python3 dev/gen-scenes.py
"""

import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
DEV = REPO / "dev"
OUT = REPO / "site" / "content"
sys.path.insert(0, str(DEV))
import voxel  # noqa: E402

UNIT, SCALE = 12, 5

GRASS = ("#7cae4e", "#5b8a3c", "#43682b")
STONE = ("#7d8794", "#525c6b", "#363e4b")
DEEP = ("#5a6472", "#3a4350", "#252c36")
GLOW = ("#ffe9a8", "#f5b942", "#d18a1f")


def render(voxels, label):
    grid = voxel.Grid()
    for i, j, k, faces in voxel.painter_order(voxels):
        voxel.cube(grid, i, j, k, faces, UNIT)
    x0, x1, y0, y1 = voxel.bounds(grid)
    w, h = (x1 - x0 + 1) * SCALE, (y1 - y0 + 1) * SCALE
    parts = [f'<svg class="scene" viewBox="0 0 {w} {h}" width="{w}" height="{h}" '
             f'shape-rendering="crispEdges" role="img" aria-label="{label}">']
    for x, y, run, color in voxel.runs(grid, x0, x1, y0, y1):
        parts.append(f'<rect x="{(x - x0) * SCALE}" y="{(y - y0) * SCALE}" '
                     f'width="{run * SCALE}" height="{SCALE}" fill="{color}"/>')
    parts.append("</svg>")
    return "".join(parts) + "\n"


def disc(radius_sq, size=5):
    c = (size - 1) / 2
    return [(i, k) for i in range(size) for k in range(size)
            if (i - c) ** 2 + (k - c) ** 2 <= radius_sq]


def island():
    out = []
    for layer, (radius, material) in enumerate(
            [(5.0, GRASS), (5.0, STONE), (2.0, STONE), (0.5, DEEP)]):
        for i, k in disc(radius):
            out.append((i, -layer, k, material))
    for i, k in [(0, 2), (4, 1)]:
        out.append((i, 1, k, GLOW))
    return out


def crates():
    """The dependency graph as a ziggurat: seven leaves, core, login, binary."""
    tiers = [(0, range(0, 7), range(0, 3), DEEP),
             (1, range(1, 6), range(0, 3), STONE),
             (2, range(2, 5), range(1, 2), STONE),
             (3, range(3, 4), range(1, 2), GLOW)]
    return [(i, j, k, mat) for j, ii, kk, mat in tiers for i in ii for k in kk]


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name, voxels, label in (
            ("_scene-island.svg", island(), "An isometric voxel island"),
            ("_scene-crates.svg", crates(),
             "The crate dependency graph as a stepped isometric structure")):
        (OUT / name).write_text(render(voxels, label), encoding="utf-8")
        print(f"wrote site/content/{name} ({len(voxels)} cubes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
