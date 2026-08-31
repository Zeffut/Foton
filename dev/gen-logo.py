#!/usr/bin/env python3
"""Draw the Foton mark, for the README and for the server list.

The logo is a list of voxel coordinates, not a drawing: an F built from cubes,
projected 2:1 isometric and scan-filled onto a 64x64 pixel grid, painter-sorted
so near blocks cover far ones. The output is SVG -- one rectangle per run of
identical pixels -- so it is a vector file that happens to be pixel art. It is
drawn 1:1 against the 64-pixel icon and stays exact at every integer multiple of
that; at 32 pixels, the size Minecraft renders a server icon in the list, it is
still pixel-crisp.

    python3 dev/gen-logo.py

It writes three files from the one definition: the SVG source, the README PNG,
and `package-content/favicon.png` -- the 64x64 icon the server sends to clients,
which is where most people will actually see this mark.

Changing the mark means changing CELLS or the palette. Nothing is hand-drawn,
so the three outputs cannot drift apart.
"""

import pathlib
import shutil
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import voxel

REPO = pathlib.Path(__file__).resolve().parent.parent
ASSETS = REPO / ".github" / "assets" / "readme"

GRID = 64  # pixel canvas the mark is drawn on
MARGIN = 2  # pixels of clear space around the mark
PNG_SIZE = 320
# The server list icon is 64x64, and the mark is drawn on a 64x64 grid, so one
# source pixel lands on exactly one icon pixel.
FAVICON_SIZE = GRID

# Three shades per material, one per visible cube face: (top, left, right).
# The same trick Minecraft uses to fake light on a block.
STONE = ("#7d8794", "#525c6b", "#363e4b")
GLOW = ("#ffe9a8", "#f5b942", "#d18a1f")

# An F on the isometric wall plane: full stem, a two-cell top arm, a one-cell
# middle arm. The top row is the lit one.
CELLS = [(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (1, 4), (2, 4), (1, 2)]
LIT_ROW = 4


Grid = voxel.Grid


def cube(grid, x, y, faces, unit):
    """One voxel at grid position (x, y) on the z = 0 plane."""
    voxel.cube(grid, x, 0, 0, faces, unit,
               ox=unit, oy=-y * unit + GRID - unit)


def draw(unit):
    grid = voxel.Grid()  # oversized while drawing, cropped by center()
    for x, y in sorted(CELLS, key=lambda c: (c[0], c[1])):
        cube(grid, x, y, GLOW if y == LIT_ROW else STONE, unit)
    return grid


def extent(grid):
    xs = [p[0] for p in grid.px]
    ys = [p[1] for p in grid.px]
    return max(xs) - min(xs) + 1, max(ys) - min(ys) + 1


def largest_fitting_unit():
    """The biggest cube edge whose mark still clears MARGIN on a GRID canvas.

    Kept an even number so `unit // 2` is exact and the isometric top faces stay
    symmetrical, and the mark is drawn at 1:1 against the 64-pixel icon, so no
    fractional scaling ever blurs a pixel.
    """
    room = GRID - 2 * MARGIN
    best = 2
    for unit in range(2, 33, 2):
        width, height = extent(draw(unit))
        if width <= room and height <= room:
            best = unit
    return best


def center(grid, size=GRID):
    """Crops and centers the drawing on a `size` x `size` canvas."""
    xs = [p[0] for p in grid.px]
    ys = [p[1] for p in grid.px]
    dx = (size - (max(xs) - min(xs) + 1)) // 2 - min(xs)
    dy = (size - (max(ys) - min(ys) + 1)) // 2 - min(ys)
    moved = voxel.Grid()
    for (x, y), color in grid.px.items():
        moved.set(x + dx, y + dy, color)
    return moved


def to_svg(grid, scale=10):
    """Merge each horizontal run of identical pixels into one rectangle."""
    size = GRID * scale
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" '
             f'width="{size}" height="{size}" shape-rendering="crispEdges">']
    for x, y, run, color in voxel.runs(grid, 0, GRID - 1, 0, GRID - 1):
        parts.append(f'<rect x="{x * scale}" y="{y * scale}" '
                     f'width="{run * scale}" height="{scale}" fill="{color}"/>')
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


def main():
    unit = largest_fitting_unit()
    mark = center(draw(unit))
    svg_path = ASSETS / "foton-logo.svg"
    svg_path.write_text(to_svg(mark))
    width, height = extent(mark)
    print(f"wrote {svg_path.relative_to(REPO)} "
          f"(cube edge {unit}px, mark {width}x{height} on a {GRID}px grid)")

    if not shutil.which("rsvg-convert"):
        print("rsvg-convert not found; the raster files were left as they were", file=sys.stderr)
        return 1
    for path, size in ((ASSETS / "foton-logo.png", PNG_SIZE),
                       (REPO / "package-content" / "favicon.png", FAVICON_SIZE)):
        subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size),
                        "-o", str(path), str(svg_path)], check=True)
        print(f"wrote {path.relative_to(REPO)} ({size}px)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
