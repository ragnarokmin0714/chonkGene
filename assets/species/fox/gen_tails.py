# Prints the fox's nine tails for quad.ron: `python3 gen_tails.py`, then
# paste the PIVOTS, PARENTS and PARTS sections over the tail entries there.
#
# Nine little flames opening behind like the petals of a flower, as chibi
# kitsune are drawn: each thin at the root, fullest a third of the way up,
# then drawn out to a point that flicks back in and up. Four round cones
# a tail, each starting at the radius the one before ends on and carrying
# on in the same direction, so the flame is one smooth surface with no
# lumps at its joints. White at the root, through pink, to the tip colour.
# The fox has few bones to spare (MAX_BONES), so a tail is four bones.
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
HALF = math.radians(60)
LEN = [0.4, 0.6, 0.6, 0.64]
# The radius each segment starts on; the last ends on a point.
R = [0.1, 0.21, 0.24, 0.15, 0.025]
# How each segment bends from the last: along `out` (away from the axis)
# and up. Out, then back in and up at the end: a flame's flick.
BEND = [None, (0.2, 0.1), (0.18, 0.2), (-0.35, 0.5)]
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
        h = LEN[k] * size
        to = [x * h for x in d]
        pivots.append(f'        "{names[k]}": {f(p)},')
        parents.append(f'        "{names[k]}": "{"hips" if k == 0 else names[k - 1]}",')
        # A cone along +y, turned onto d: roll about z, then pitch about x.
        g = -math.degrees(math.asin(max(-1, min(1, d[0]))))
        a = math.degrees(math.atan2(d[2], d[1]))
        marks = ""
        if k == 2:
            marks = f'\n            marks: [(at: (0, {h:.3f}, 0), radii: (0.3, 0.25, 0.3), color: "tailmid", soft: 0.8)]'
        if k == 3:
            marks = (
                f'\n            marks: [(at: (0, {h * 0.3:.3f}, 0), radii: (0.3, 0.25, 0.3), color: "tailmid", soft: 0.8),'
                f'\n                (at: (0, {h:.3f}, 0), radii: (0.2, 0.3, 0.2), color: "tailtip", soft: 0.6)]'
            )
        parts.append(
            f'        (name: "{names[k]}", bone: "{names[k]}", shape: Cone(r1: {R[k]}, r2: {R[k + 1]}, h: {h:.3f}), at: {f(p)},\n'
            f'            rot: {f([a, 0, g])}, blend: 0.1, color: "fur", zone: "bottom"{"," if marks else ""}{marks}),'
        )
        p = add(p, to)
print("PIVOTS")
print("\n".join(pivots))
print("PARENTS")
print("\n".join(parents))
print("PARTS")
print("\n".join(parts))
