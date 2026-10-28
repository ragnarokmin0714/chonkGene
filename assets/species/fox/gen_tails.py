# Prints the fox's nine tails for quad.ron: `python3 gen_tails.py`, then
# paste the PIVOTS, PARENTS and PARTS sections over the tail entries there.
#
# Nine plump teardrops opening behind like the petals of a flower, as chibi
# kitsune are drawn: each thin at the root, fat in the middle, coming to a
# point that curls up and out. White at the root, through pink, to the tip
# colour. Packed close, the nine read as one big fluffy bloom.
import math


def norm(v):
    l = math.sqrt(sum(c * c for c in v))
    return [c / l for c in v]


def add(a, b, k=1.0):
    return [x + y * k for x, y in zip(a, b)]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def f(v):
    return "(" + ", ".join((f"{c:.3f}".rstrip("0").rstrip(".") if abs(c) >= 5e-4 else "0") for c in v) + ")"


base = [0.0, 0.1, -0.88]
axis = norm([0.0, 0.9, -0.42])
u = norm(cross(axis, [1, 0, 0]))  # in the y-z plane, perpendicular to axis
v = [1.0, 0.0, 0.0]
HALF = math.radians(66)
LEN = [0.22, 0.34, 0.32, 0.36]
# Thin root, fat middle: a teardrop, not a finger.
R = [0.11, 0.24, 0.22]
# How each segment bends from the last: along `out` (away from the
# axis) and up; the tip curls up and out.
BEND = [None, (0.15, 0.0), (0.3, 0.1), (0.55, 0.35)]
SEG = ["", "_b", "_c", "_tip"]
pivots, parents, parts = [], [], []
for i in range(9):
    phi = math.radians(90 + i * 40)  # i = 0 leans out to the pet's left (+x)
    # No two quite alike, so the bloom looks soft rather than stamped out.
    size = 1.0 + 0.1 * math.sin(i * 2.3)
    out = add([c * math.cos(phi) for c in u], [c * math.sin(phi) for c in v])
    d = norm(add([c * math.cos(HALF) for c in axis], [c * math.sin(HALF) for c in out]))
    p = base[:]
    names = [f"tail{i + 1}{s}" for s in SEG]
    for k in range(4):
        if BEND[k]:
            d = norm(add(add(d, out, BEND[k][0]), [0, 1, 0], BEND[k][1]))
        to = [x * LEN[k] * size for x in d]
        pivots.append(f'        "{names[k]}": {f(p)},')
        parents.append(f'        "{names[k]}": "{"hips" if k == 0 else names[k - 1]}",')
        if k < 3:
            parts.append(
                f'        (name: "{names[k]}", bone: "{names[k]}", shape: Capsule(to: {f(to)}, r: {R[k]}), at: {f(p)},\n'
                f'            blend: 0.08, color: "fur", zone: "bottom"),'
            )
        else:
            # A cone along +y, turned onto d: roll about z, then pitch about x.
            g = -math.degrees(math.asin(max(-1, min(1, d[0]))))
            a = math.degrees(math.atan2(d[2], d[1]))
            h = LEN[k] * size
            parts.append(
                f'        (name: "{names[k]}", bone: "{names[k]}", shape: Cone(r1: 0.21, r2: 0.025, h: {h:.3f}), at: {f(p)},\n'
                f'            rot: {f([a, 0, g])}, blend: 0.08, color: "fur", zone: "bottom",\n'
                f'            marks: [(at: (0, {h * 0.55:.3f}, 0), radii: (0.22, 0.2, 0.22), color: "tailmid", soft: 0.8),\n'
                f'                (at: (0, {h:.3f}, 0), radii: (0.2, 0.22, 0.2), color: "tailtip", soft: 0.6)]),'
            )
        p = add(p, to)
print("PIVOTS")
print("\n".join(pivots))
print("PARENTS")
print("\n".join(parents))
print("PARTS")
print("\n".join(parts))
