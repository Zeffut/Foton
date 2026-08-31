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

    def test_i_and_k_each_move_the_voxel_down_from_baseline(self):
        """Guards against a bug that drops the (i+k)*half vertical term for
        both axes -- the symmetry test above would pass unchanged since both
        sides would land on the same wrong value, and the logo never
        exercises k != 0 so the byte-identity gate would not catch it either.
        """
        baseline, i_only, k_only = voxel.Grid(), voxel.Grid(), voxel.Grid()
        voxel.cube(baseline, 0, 0, 0, RED, unit=8)
        voxel.cube(i_only, 1, 0, 0, RED, unit=8)
        voxel.cube(k_only, 0, 0, 1, RED, unit=8)
        self.assertGreater(voxel.bounds(i_only)[2], voxel.bounds(baseline)[2])
        self.assertGreater(voxel.bounds(k_only)[2], voxel.bounds(baseline)[2])

    def test_j_moves_straight_up(self):
        base, above = voxel.Grid(), voxel.Grid()
        voxel.cube(base, 0, 0, 0, RED, unit=8)
        voxel.cube(above, 0, 1, 0, RED, unit=8)
        self.assertEqual(voxel.bounds(base)[0], voxel.bounds(above)[0])
        self.assertEqual(voxel.bounds(above)[2], voxel.bounds(base)[2] - 8)


class GridStorage(unittest.TestCase):
    def test_set_keeps_out_of_range_coordinates(self):
        """The shared Grid intentionally drops the old logo Grid's
        0 <= x < size clipping -- callers bound their own runs()/bounds()
        calls instead. Pins that .set() never re-grows that clip.
        """
        grid = voxel.Grid()
        grid.set(-5, 999, "#123456")
        self.assertEqual(grid.px.get((-5, 999)), "#123456")


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


class PainterOrder(unittest.TestCase):
    def test_the_nearer_voxel_wins_the_overlap_regardless_of_input_order(self):
        """(i, j, k) and (i+1, j+1, k+1) project to the same screen position
        -- shifting all three axes by the same amount moves a voxel straight
        along the camera's line of sight without moving it on screen -- but
        the second is three closer in painter_order's i+j+k depth key. Fed in
        either order, the nearer one must be the last one drawn, so it must
        win the overlapping pixel both times.
        """
        BLUE = ("#0000ff", "#0000cc", "#000099")
        far = (0, 0, 0, RED)
        near = (1, 1, 1, BLUE)
        for voxels in ([far, near], [near, far]):
            grid = voxel.Grid()
            for i, j, k, faces in voxel.painter_order(voxels):
                voxel.cube(grid, i, j, k, faces, unit=8)
            self.assertEqual(grid.px[(0, 0)], BLUE[1])


if __name__ == "__main__":
    unittest.main()
