//! The advancement screen's tree layout, run at build time.
//!
//! Vanilla parity: `TreeNodePosition`, a Reingold-Tilford / Buchheim layout
//! that writes an `(x, y)` back into every visible advancement's `DisplayInfo`.
//! Vanilla runs it in `ServerAdvancementManager.apply` after each datapack
//! reload; Foton's advancement set is fixed at build time, so the same walk
//! runs here and the coordinates are baked into the generated data.
//!
//! One deliberate difference: vanilla iterates an advancement's children out of
//! a `ReferenceOpenHashSet`, whose order comes from identity hash codes and so
//! differs between JVM runs. Foton sorts children by key, which makes the
//! layout reproducible. Both orders satisfy the layout's own invariant -- no
//! two visible advancements share a cell -- so the only observable difference
//! is which row a sibling lands on.

/// One node of the layout walk.
struct Node {
    /// The advancement this node positions.
    advancement: usize,
    parent: Option<usize>,
    previous_sibling: Option<usize>,
    /// Vanilla's `childIndex`, which is one-based.
    child_index: i32,
    children: Vec<usize>,
    ancestor: usize,
    thread: Option<usize>,
    x: i32,
    y: f32,
    modifier: f32,
    change: f32,
    shift: f32,
}

/// Positions every visible advancement under one root.
///
/// `children_of[i]` must list the advancements whose parent is advancement `i`,
/// and `has_display[i]` whether advancement `i` is drawn at all. Returns one
/// `(advancement index, x, y)` per positioned advancement.
///
/// Vanilla parity: `TreeNodePosition.run`.
pub fn run(
    root: usize,
    children_of: &[Vec<usize>],
    has_display: &[bool],
) -> Vec<(usize, f32, f32)> {
    assert!(
        has_display[root],
        "cannot position the children of an invisible root"
    );

    let mut arena: Vec<Node> = Vec::new();
    let root_node = build(&mut arena, root, None, None, 1, 0, children_of, has_display);

    first_walk(&mut arena, root_node);
    let root_y = arena[root_node].y;
    let min = second_walk(&mut arena, root_node, 0.0, 0, root_y);
    if min < 0.0 {
        third_walk(&mut arena, root_node, -min);
    }

    let mut positions = Vec::new();
    finalize(&arena, root_node, &mut positions);
    positions
}

#[expect(
    clippy::too_many_arguments,
    reason = "mirrors vanilla's TreeNodePosition constructor, which takes the same five plus the two lookup tables"
)]
fn build(
    arena: &mut Vec<Node>,
    advancement: usize,
    parent: Option<usize>,
    previous_sibling: Option<usize>,
    child_index: i32,
    depth: i32,
    children_of: &[Vec<usize>],
    has_display: &[bool],
) -> usize {
    let me = arena.len();
    arena.push(Node {
        advancement,
        parent,
        previous_sibling,
        child_index,
        children: Vec::new(),
        ancestor: me,
        thread: None,
        x: depth,
        y: -1.0,
        modifier: 0.0,
        change: 0.0,
        shift: 0.0,
    });

    let mut previous = None;
    for &child in &children_of[advancement] {
        previous = add_child(arena, me, child, previous, depth, children_of, has_display);
    }
    me
}

/// Vanilla parity: `TreeNodePosition.addChild`, including the way a
/// display-less advancement is skipped and its own children are spliced in at
/// the same level.
fn add_child(
    arena: &mut Vec<Node>,
    parent_node: usize,
    advancement: usize,
    previous: Option<usize>,
    parent_depth: i32,
    children_of: &[Vec<usize>],
    has_display: &[bool],
) -> Option<usize> {
    if has_display[advancement] {
        let child_index = i32::try_from(arena[parent_node].children.len()).unwrap_or(i32::MAX) + 1;
        let node = build(
            arena,
            advancement,
            Some(parent_node),
            previous,
            child_index,
            parent_depth + 1,
            children_of,
            has_display,
        );
        arena[parent_node].children.push(node);
        return Some(node);
    }

    let mut previous = previous;
    for &grandchild in &children_of[advancement] {
        previous = add_child(
            arena,
            parent_node,
            grandchild,
            previous,
            parent_depth,
            children_of,
            has_display,
        );
    }
    previous
}

fn first_walk(arena: &mut [Node], node: usize) {
    if arena[node].children.is_empty() {
        arena[node].y = match arena[node].previous_sibling {
            Some(previous) => arena[previous].y + 1.0,
            None => 0.0,
        };
        return;
    }

    let mut default_ancestor: Option<usize> = None;
    for index in 0..arena[node].children.len() {
        let child = arena[node].children[index];
        first_walk(arena, child);
        default_ancestor = Some(apportion(arena, child, default_ancestor.unwrap_or(child)));
    }

    execute_shifts(arena, node);
    let first = arena[node].children[0];
    let last = arena[node].children[arena[node].children.len() - 1];
    let midpoint = f32::midpoint(arena[first].y, arena[last].y);
    match arena[node].previous_sibling {
        Some(previous) => {
            arena[node].y = arena[previous].y + 1.0;
            arena[node].modifier = arena[node].y - midpoint;
        }
        None => arena[node].y = midpoint,
    }
}

fn second_walk(arena: &mut [Node], node: usize, mod_sum: f32, depth: i32, mut min: f32) -> f32 {
    arena[node].y += mod_sum;
    arena[node].x = depth;
    if arena[node].y < min {
        min = arena[node].y;
    }

    let modifier = arena[node].modifier;
    for index in 0..arena[node].children.len() {
        let child = arena[node].children[index];
        min = second_walk(arena, child, mod_sum + modifier, depth + 1, min);
    }
    min
}

fn third_walk(arena: &mut [Node], node: usize, offset: f32) {
    arena[node].y += offset;
    for index in 0..arena[node].children.len() {
        let child = arena[node].children[index];
        third_walk(arena, child, offset);
    }
}

fn execute_shifts(arena: &mut [Node], node: usize) {
    let mut shift = 0.0;
    let mut change = 0.0;
    for index in (0..arena[node].children.len()).rev() {
        let child = arena[node].children[index];
        arena[child].y += shift;
        arena[child].modifier += shift;
        change += arena[child].change;
        shift += arena[child].shift + change;
    }
}

fn previous_or_thread(arena: &[Node], node: usize) -> Option<usize> {
    if let Some(thread) = arena[node].thread {
        return Some(thread);
    }
    arena[node].children.first().copied()
}

fn next_or_thread(arena: &[Node], node: usize) -> Option<usize> {
    if let Some(thread) = arena[node].thread {
        return Some(thread);
    }
    arena[node].children.last().copied()
}

/// Vanilla parity: `TreeNodePosition.apportion`.
///
/// Vanilla's four contour cursors are the terse `v` pairs; they are spelled
/// out here as `inner_right`, `outer_right`, `inner_left` and `outer_left`.
/// The `sir`/`sor`/`sil`/`sol` sums keep their vanilla names, being just the
/// running modifier totals of those same four.
fn apportion(arena: &mut [Node], node: usize, mut default_ancestor: usize) -> usize {
    let Some(previous_sibling) = arena[node].previous_sibling else {
        return default_ancestor;
    };
    let parent = arena[node]
        .parent
        .expect("a node with a previous sibling always has a parent");

    let mut inner_right = node;
    let mut outer_right = node;
    let mut inner_left = previous_sibling;
    let mut outer_left = arena[parent].children[0];
    let mut sir = arena[node].modifier;
    let mut sor = arena[node].modifier;
    let mut sil = arena[inner_left].modifier;
    let mut sol = arena[outer_left].modifier;

    while let (Some(next_l), Some(previous_r)) = (
        next_or_thread(arena, inner_left),
        previous_or_thread(arena, inner_right),
    ) {
        inner_left = next_l;
        inner_right = previous_r;
        // Vanilla dereferences these two without a null check; the loop
        // condition guarantees the corresponding left/right spines are as deep.
        outer_left = previous_or_thread(arena, outer_left)
            .expect("left outer spine keeps pace with the inner one");
        outer_right = next_or_thread(arena, outer_right)
            .expect("right outer spine keeps pace with the inner one");
        arena[outer_right].ancestor = node;

        let shift = arena[inner_left].y + sil - (arena[inner_right].y + sir) + 1.0;
        if shift > 0.0 {
            let ancestor = get_ancestor(arena, inner_left, node, default_ancestor);
            move_subtree(arena, ancestor, node, shift);
            sir += shift;
            sor += shift;
        }

        sil += arena[inner_left].modifier;
        sir += arena[inner_right].modifier;
        sol += arena[outer_left].modifier;
        sor += arena[outer_right].modifier;
    }

    if next_or_thread(arena, inner_left).is_some() && next_or_thread(arena, outer_right).is_none() {
        arena[outer_right].thread = next_or_thread(arena, inner_left);
        arena[outer_right].modifier += sil - sor;
    } else {
        if previous_or_thread(arena, inner_right).is_some()
            && previous_or_thread(arena, outer_left).is_none()
        {
            arena[outer_left].thread = previous_or_thread(arena, inner_right);
            arena[outer_left].modifier += sir - sol;
        }
        default_ancestor = node;
    }

    default_ancestor
}

fn move_subtree(arena: &mut [Node], left: usize, right: usize, shift: f32) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "vanilla computes the same difference as a float from two small ints"
    )]
    let subtrees = (arena[right].child_index - arena[left].child_index) as f32;
    if subtrees != 0.0 {
        arena[right].change -= shift / subtrees;
        arena[left].change += shift / subtrees;
    }
    arena[right].shift += shift;
    arena[right].y += shift;
    arena[right].modifier += shift;
}

/// Vanilla parity: `TreeNodePosition.getAncestor`. Vanilla's `ancestor` field
/// is never null once constructed, so only the sibling test survives.
fn get_ancestor(arena: &[Node], node: usize, other: usize, default_ancestor: usize) -> usize {
    let ancestor = arena[node].ancestor;
    let other_parent = arena[other]
        .parent
        .expect("apportion only runs on a node with a parent");
    if arena[other_parent].children.contains(&ancestor) {
        ancestor
    } else {
        default_ancestor
    }
}

fn finalize(arena: &[Node], node: usize, out: &mut Vec<(usize, f32, f32)>) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "vanilla stores the integer depth into a float x"
    )]
    let x = arena[node].x as f32;
    out.push((arena[node].advancement, x, arena[node].y));
    for &child in &arena[node].children {
        finalize(arena, child, out);
    }
}
