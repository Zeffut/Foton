#!/usr/bin/env python3
"""Checks on the shared voxel renderer."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import voxel


RED = ("#ff0000", "#cc0000", "#990000")


class CubeGeometry(unittest.TestCase):
    def test_one_cube_occupies_two_units_square(self):
        grid = voxel.Grid()
        voxel.cube(grid, 0, 0, 0, RED, unit=8)
        x0, x1, y0, y1 = voxel.bounds(grid)
        self.assertEqual((x1 - x0 + 1, y1 - y0 + 1), (16, 16))

    def test_all_three_faces_are_painted(self):
        grid = voxel.Grid()
        voxel.cube(grid, 0, 0, 0, RED, unit=8)
        self.assertEqual(set(grid.px.values()), set(RED))

    def test_i_moves_right_and_down_k_moves_left_and_down(self):
        right, left = voxel.Grid(), voxel.Grid()
        voxel.cube(right, 1, 0, 0, RED, unit=8)
        voxel.cube(left, 0, 0, 1, RED, unit=8)
        self.assertGreater(voxel.bounds(right)[0], voxel.bounds(left)[0])
        self.assertEqual(voxel.bounds(right)[2], voxel.bounds(left)[2])

    def test_j_moves_straight_up(self):
        base, above = voxel.Grid(), voxel.Grid()
        voxel.cube(base, 0, 0, 0, RED, unit=8)
        voxel.cube(above, 0, 1, 0, RED, unit=8)
        self.assertEqual(voxel.bounds(base)[0], voxel.bounds(above)[0])
        self.assertEqual(voxel.bounds(above)[2], voxel.bounds(base)[2] - 8)


class Runs(unittest.TestCase):
    def test_identical_neighbours_merge_into_one_run(self):
        grid = voxel.Grid()
        for x in range(5):
            grid.set(x, 0, "#abcdef")
        self.assertEqual(list(voxel.runs(grid, 0, 4, 0, 0)),
                         [(0, 0, 5, "#abcdef")])

    def test_a_colour_change_breaks_the_run(self):
        grid = voxel.Grid()
        grid.set(0, 0, "#111111")
        grid.set(1, 0, "#222222")
        self.assertEqual([r[2] for r in voxel.runs(grid, 0, 1, 0, 0)], [1, 1])

    def test_a_gap_breaks_the_run(self):
        grid = voxel.Grid()
        grid.set(0, 0, "#111111")
        grid.set(2, 0, "#111111")
        self.assertEqual([(r[0], r[2]) for r in voxel.runs(grid, 0, 2, 0, 0)],
                         [(0, 1), (2, 1)])


if __name__ == "__main__":
    unittest.main()
