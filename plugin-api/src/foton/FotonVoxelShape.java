package foton;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import org.bukkit.util.BoundingBox;

/** A block shape read once from the server: its boxes, block-local. */
public final class FotonVoxelShape implements org.bukkit.util.VoxelShape {
    private final List<BoundingBox> boxes;

    public FotonVoxelShape(double[] values) {
        ArrayList<BoundingBox> read = new ArrayList<>();
        if (values != null)
            for (int i = 0; i + 5 < values.length; i += 6)
                read.add(new BoundingBox(values[i], values[i + 1], values[i + 2], values[i + 3], values[i + 4], values[i + 5]));
        boxes = List.copyOf(read);
    }

    @Override public Collection<BoundingBox> getBoundingBoxes() {
        ArrayList<BoundingBox> copy = new ArrayList<>(boxes.size());
        for (BoundingBox box : boxes) copy.add(box.clone());
        return copy;
    }

    @Override public boolean overlaps(BoundingBox other) {
        if (other == null) throw new IllegalArgumentException("Other cannot be null");
        for (BoundingBox box : boxes) if (box.overlaps(other)) return true;
        return false;
    }

    @Override public String toString() { return "FotonVoxelShape" + boxes; }
}
