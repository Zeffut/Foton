"""Strips any [server.rcon] section from a config.toml.

The server generates one itself now that it speaks RCON. A test rig that
appends its own on top leaves two, and a duplicate key is something the server
refuses to start on.

Usage: strip-rcon-section.py <config.toml>
"""

import io
import re
import sys

path = sys.argv[1]
text = io.open(path, encoding="utf-8").read()
text = re.sub(r"^\[server\.rcon\]\n(?:(?!^\[).*\n)*", "", text, flags=re.M)
io.open(path, "w", encoding="utf-8", newline="\n").write(text)
