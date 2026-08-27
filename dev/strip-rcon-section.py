"""Retire toute section [server.rcon] d'un config.toml.

Le serveur en genere une depuis que RCON existe ; le script de test en ajoute
une seconde, et deux sections identiques sont un `duplicate key` sur lequel le
serveur refuse de demarrer.
"""
import io
import re
import sys

path = sys.argv[1]
s = io.open(path, encoding="utf-8").read()
s = re.sub(r"^\[server\.rcon\]\n(?:(?!^\[).*\n)*", "", s, flags=re.M)
io.open(path, "w", encoding="utf-8", newline="\n").write(s)
