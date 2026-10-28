# Prints the fox's nine tails for quad.ron: `python3 gen_tails.py`, then
# paste the PIVOTS, PARENTS and PARTS sections over the tail entries there.
#
# Nine wisps in a bouquet: each leaves the rump along a cone opening up and
# back, then bends out, in, out again (an S, like a flame), thinning to a
# fine tip. White at the root, through pink, to red at the tip.
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


base = [0.0, -0.02, -0.95]
axis = norm([0.0, 0.95, -0.32])
u = norm(cross(axis, [1, 0, 0]))  # in the y-z plane, perpendicular to axis
v = [1.0, 0.0, 0.0]
HALF = math.radians(60)
LEN = [0.27, 0.32, 0.32, 0.3, 0.33]
# Thick in the middle: a brush, not a finger.
R = [0.13, 0.19, 0.18, 0.13]
# How each segment bends from the last: along `out` (away from the axis)
# and up. Out, in, out: an S.
BEND = [None, (0.5, 0.1), (-0.75, 0.2), (0.75, 0.1), (0.45, 0.25)]
SEG = ["", "_b", "_c", "_d", "_tip"]
pivots, parents, parts = [], [], []
for i in range(9):
    phi = math.radians(90 + i * 40)  # i = 0 leans out to the pet's left (+x)
    # No two alike: each a little longer or shorter, its S a little deeper
    # or shallower, so the nine read as flames rather than fingers.
    size = 1.0 + 0.14 * math.sin(i * 2.3)
    deep = 1.0 + 0.3 * math.sin(i * 1.7 + 1.0)
    out =add([c * math.cos(phi) for c in u], [c * math.sin(phi) for c in v])
    d = norm(add([c * math.cos(HALF) for c in axis], [c * math.sin(HALF) for c in out]))
    p = base[:]
    names = [f"tail{i + 1}{s}" for s in SEG]
    for k in range(5):
        if BEND[k]:
            d = norm(add(add(d, out, BEND[k][0] * deep), [0, 1, 0], BEND[k][1]))
        to = [x * LEN[k] * size for x in d]
        pivots.append(f'        "{names[k]}": {f(p)},')
        parents.append(f'        "{names[k]}": "{"hips" if k == 0 else names[k - 1]}",')
        if k < 4:
            marks = {
                2: f'\n            marks: [(at: {f(to)}, radii: (0.22, 0.22, 0.22), color: "tailmid", soft: 0.85)]',
                3: f'\n            marks: [(at: {f([c * 0.3 for c in to])}, radii: (0.24, 0.24, 0.24), color: "tailmid", soft: 0.6),\n'
                f'                (at: {f(to)}, radii: (0.2, 0.2, 0.2), color: "tailtip", soft: 0.8)]',
            }.get(k, "")
            parts.append(
                f'        (name: "{names[k]}", bone: "{names[k]}", shape: Capsule(to: {f(to)}, r: {R[k]}), at: {f(p)},\n'
                f'            blend: 0.07, color: "fur", zone: "bottom"{"," if marks else ""}{marks}),'
            )
        else:
            # A cone along +y, turned onto d: roll about z, then pitch about x.
            g = -math.degrees(math.asin(max(-1, min(1, d[0]))))
            a = math.degrees(math.atan2(d[2], d[1]))
            parts.append(
                f'        (name: "{names[k]}", bone: "{names[k]}", shape: Cone(r1: 0.13, r2: 0.02, h: {LEN[k] * size:.3f}), at: {f(p)},\n'
                f'            rot: {f([a, 0, g])}, blend: 0.06, color: "tailtip", zone: "bottom"),'
            )
        p = add(p, to)
print("PIVOTS")
print("\n".join(pivots))
print("PARENTS")
print("\n".join(parents))
print("PARTS")
print("\n".join(parts))
