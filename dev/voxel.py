#!/usr/bin/env python3
"""Isometric voxel rasteriser, shared by the logo and the website.

The mark in `.github/assets/readme/` is not a drawing: it is a list of voxel
coordinates scan-filled onto a pixel grid and emitted as one rectangle per run
of identical pixels, so it is a vector file that happens to be pixel art. The
site draws its scenes the same way, which is why this lives on its own -- the
logo needs one wall plane, the site needs a full `(i, j, k)` space, and both
need the result to stair-step identically.
"""


class Grid:
    """A sparse pixel canvas. Unset pixels are transparent."""

    def __init__(self):
        self.px = {}

    def set(self, x, y, color):
        self.px[(x, y)] = color

    def fill(self, points, color):
        """Scanline fill of a convex polygon in pixel coordinates."""
        ys = [p[1] for p in points]
        for y in range(int(min(ys)), int(max(ys)) + 1):
            crossings = []
            for i, (x0, y0) in enumerate(points):
                x1, y1 = points[(i + 1) % len(points)]
                if y0 == y1:
                    continue
                if min(y0, y1) <= y + 0.5 < max(y0, y1):
                    crossings.append(x0 + (y + 0.5 - y0) * (x1 - x0) / (y1 - y0))
            crossings.sort()
            for i in range(0, len(crossings) - 1, 2):
                for x in range(int(round(crossings[i])), int(round(crossings[i + 1]))):
                    self.set(x, y, color)


def cube(grid, i, j, k, faces, unit, ox=0, oy=0):
    """One voxel. `i` runs right-and-down, `k` left-and-down, `j` straight up.

    `faces` is (top, left, right) -- the three shades of one material, the
    trick the game itself uses to fake light on a block.
    """
    half = unit // 2
    px = (i - k) * unit + ox
    py = (i + k) * half - j * unit + oy
    top, left, right = faces
    grid.fill([(px, py), (px + unit, py - half),
               (px + 2 * unit, py), (px + unit, py + half)], top)
    grid.fill([(px, py), (px + unit, py + half),
               (px + unit, py + half + unit), (px, py + unit)], left)
    grid.fill([(px + unit, py + half), (px + 2 * unit, py),
               (px + 2 * unit, py + unit), (px + unit, py + half + unit)], right)


def bounds(grid):
    """Inclusive (x0, x1, y0, y1) of everything painted."""
    xs = [p[0] for p in grid.px]
    ys = [p[1] for p in grid.px]
    return min(xs), max(xs), min(ys), max(ys)


def runs(grid, x0, x1, y0, y1):
    """Yields (x, y, length, color) per horizontal run of identical pixels."""
    for y in range(y0, y1 + 1):
        x = x0
        while x <= x1:
            color = grid.px.get((x, y))
            if not color:
                x += 1
                continue
            length = 1
            while x + length <= x1 and grid.px.get((x + length, y)) == color:
                length += 1
            yield x, y, length, color
            x += length


def painter_order(voxels):
    """Sorts (i, j, k, faces) so nearer voxels are drawn last."""
    return sorted(voxels, key=lambda v: v[0] + v[1] + v[2])
