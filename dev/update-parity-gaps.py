import io
import os
import re

# The repository this script belongs to, rather than a fixed path: it also
# runs from a git worktree, and a hard-coded root made a worktree read the
# main checkout's generated file and write the main checkout's ledger --
# silently producing a ledger for the wrong tree.
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__))) + "/"
GEN = ROOT + "foton-core/src/behavior/generated/parity.rs"
LEDGER = ROOT + "dev/parity-gaps.txt"

source = io.open(GEN, encoding="utf-8").read()


def names(const):
    block = re.search(
        r"pub const " + const + r": &\[&str\] = &\[(.*?)\];", source, re.S
    )
    assert block, const
    return re.findall(r'"([^"]+)"', block.group(1))


blocks = names("UNCLAIMED_BLOCK_CLASSES")
items = names("UNCLAIMED_ITEM_CLASSES")
entities = names("UNCLAIMED_ENTITY_CLASSES")

HEADER = """# Vanilla classes Foton has no behavior for.
#
# Generated from `classes.json` at build time and checked by
# `parity_ledger_matches_the_generated_gaps`. A line that disappears is a piece
# of vanilla that now works; a line that appears is one that stopped, or a new
# class the extractor found. Either way it shows up in a diff and has to be
# looked at.
#
# Not every line is work to do. Vanilla's plain `Block` and `Item` need no
# behavior at all -- `DefaultBlockBehavior` covers them -- and they are listed
# here only because the ledger is mechanical and does not guess.
#
# Sections are `blocks`, `items`, `entities`. Sorted, one class per line.

"""

with io.open(LEDGER, "w", encoding="utf-8", newline="\n") as handle:
    handle.write(HEADER)
    sections = (("blocks", blocks), ("items", items), ("entities", entities))
    for index, (section, values) in enumerate(sections):
        handle.write("[%s] %d\n" % (section, len(values)))
        for value in values:
            handle.write("%s\n" % value)
        # One trailing newline at the end of the file: the repository's
        # end-of-file hook trims a second one, and the next run would put
        # it back, so the two would fight over every commit.
        if index + 1 < len(sections):
            handle.write("\n")

print(
    "ledger written: %d blocks, %d items, %d entities"
    % (len(blocks), len(items), len(entities))
)
