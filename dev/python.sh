# Resolve a Python interpreter that actually runs code. Source it; it sets $PY.
#
# Two traps this exists to close, both of which fail silently:
#
#   - `python3` on Windows is usually the Microsoft Store stub. It prints an
#     advertisement, executes nothing, and exits 0 -- so a check that shells
#     out to it reports success without having run. Conversely most Linux
#     images ship `python3` and no `python`, so neither name is portable on
#     its own.
#   - without PYTHONUTF8 a script writing accented output dies on a cp1252
#     encode *after* it has opened its output file, which is how
#     CONFIGURATION.md once ended up empty.

PY=""
for _foton_py in python3 python py; do
  if [ "$("$_foton_py" -c 'print(3)' 2>/dev/null)" = "3" ]; then
    PY="$_foton_py"
    break
  fi
done
unset _foton_py

if [ -z "$PY" ]; then
  echo "no working Python interpreter (tried python3, python, py)" >&2
  return 1 2>/dev/null || exit 1
fi

export PYTHONUTF8=1
