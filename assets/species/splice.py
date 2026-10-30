# Puts a generator's output (PIVOTS, PARENTS, PARTS, as the gen_*.py scripts
# print them) over the tail entries of a quad.ron:
#   python3 gen_tail.py | python3 ../splice.py quad.ron
import re, sys
path = sys.argv[1]
gen = sys.stdin.read().split("\n")
sec, cur = {}, None
for l in gen:
    if l in ("PIVOTS", "PARENTS", "PARTS"):
        cur = l; sec[cur] = []
    elif cur and l.strip():
        sec[cur].append(l)
src = open(path).read().split("\n")
out, placed = [], set()
i = 0
while i < len(src):
    l = src[i]
    if re.match(r'^        "tail\w*": \(', l):
        if "PIVOTS" not in placed: out += sec["PIVOTS"]; placed.add("PIVOTS")
        i += 1; continue
    if re.match(r'^        "tail\w*": "', l):
        if "PARENTS" not in placed: out += sec["PARENTS"]; placed.add("PARENTS")
        i += 1; continue
    if re.match(r'^        \(name: "tail', l):
        if "PARTS" not in placed: out += sec["PARTS"]; placed.add("PARTS")
        i += 1
        while i < len(src) and src[i].startswith("            "): i += 1
        continue
    out.append(l); i += 1
assert placed == {"PIVOTS", "PARENTS", "PARTS"}, placed
open(path, "w").write("\n".join(out))
