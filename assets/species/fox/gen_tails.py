# Prints the fox's nine tails for quad.ron: `python3 gen_tails.py | python3
# ../splice.py quad.ron`.
#
# Nine little flames opening behind like the petals of a flower, as chibi
# kitsune are drawn: each a soft, round-edged flame, thin at the root,
# fullest a third of the way up, drawn out to a rounded tip that flicks back
# in and up. The centre line is a smooth curve, sampled into eight round
# cones whose radii meet and which carry on in the same direction, so the
# flame is one smooth surface with no lumps or corners; they blend only a
# hair (a wide smooth union swells where two cones overlap, which shows as
# rings), widely only at the root. The fox has few
# bones to spare (MAX_BONES), so a tail is four bones, two cones each.
# White at the root, through pink, to the tip colour.
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
LENGTH = 2.05
CONES = 16  # four to a bone
SEG = ["", "_b", "_c", "_tip"]


def radius(s):
    """The flame's half-width a fraction `s` of the way up: a thin root
    swelling to a plump belly a third of the way up, then an ogive easing
    in to a rounded tip (never a needle): a teardrop, as a flame is drawn."""
    if s < 0.32:
        return 0.09 + 0.21 * math.sin(0.5 * math.pi * s / 0.32)
    return 0.07 + 0.23 * math.cos(0.5 * math.pi * (s - 0.32) / 0.68) ** 0.9


def bend(s):
    """How the flame turns away from its axis (+) or back toward it (-),
    and up, per unit length, a fraction `s` of the way up: out through
    the middle, then a flick back in and up at the tip."""
    out = 0.5 if s < 0.55 else -1.1
    up = 0.3 if s < 0.55 else 1.0
    return out, up


pivots, parents, parts = [], [], []
for i in range(9):
    phi = math.radians(90 + i * 40)  # i = 0 leans out to the pet's left (+x)
    # No two quite alike, so the bloom looks soft rather than stamped out.
    size = 1.0 + 0.1 * math.sin(i * 2.3)
    out = add([c * math.cos(phi) for c in u], [c * math.sin(phi) for c in v])
    d = norm(add([c * math.cos(HALF) for c in axis], [c * math.sin(HALF) for c in out]))
    p = base[:]
    names = [f"tail{i + 1}{s}" for s in SEG]
    step = LENGTH * size / CONES
    for k in range(CONES):
        s0, s1 = k / CONES, (k + 1) / CONES
        # Turn half before and half after the step (midpoint rule), so the
        # cones meet at the same angle on both sides of each joint.
        o, up = bend((s0 + s1) / 2)
        d = norm(add(add(d, out, o * step * 0.5), [0, 1, 0], up * step * 0.5))
        to = [x * step for x in d]
        bone = names[k // (CONES // 4)]
        if k % (CONES // 4) == 0:
            pivots.append(f'        "{bone}": {f(p)},')
            parents.append(f'        "{bone}": "{"hips" if k == 0 else names[k // (CONES // 4) - 1]}",')
        g = -math.degrees(math.asin(max(-1, min(1, d[0]))))
        a = math.degrees(math.atan2(d[2], d[1]))
        # One soft gradient up the flame: pink from halfway, the tip colour
        # at the end (big soft spots, so no bands).
        marks = ""
        if k == CONES // 2:
            marks = f',\n            marks: [(at: (0, {step:.3f}, 0), radii: (0.45, 0.4, 0.45), color: "tailmid", soft: 0.9)]'
        if k == CONES - 2:
            marks = f',\n            marks: [(at: (0, 0, 0), radii: (0.45, 0.4, 0.45), color: "tailmid", soft: 0.9)]'
        if k == CONES - 1:
            marks = (
                f',\n            marks: [(at: (0, 0, 0), radii: (0.4, 0.3, 0.4), color: "tailmid", soft: 0.9),'
                f'\n                (at: (0, {step:.3f}, 0), radii: (0.3, {step * 1.3:.3f}, 0.3), color: "tailtip", soft: 0.7)]'
            )
        name = bone if k % (CONES // 4) == 0 else f"{bone}_{k % (CONES // 4) + 1}"
        parts.append(
            f'        (name: "{name}", bone: "{bone}", shape: Cone(r1: {radius(s0):.3f}, r2: {radius(s1):.3f}, h: {step:.3f}), at: {f(p)},\n'
            f'            rot: {f([a, 0, g])}, blend: {0.1 if k == 0 else 0.02}, color: "fur", zone: "bottom"{marks}),'
        )
        d = norm(add(add(d, out, o * step * 0.5), [0, 1, 0], up * step * 0.5))
        p = add(p, to)
print("PIVOTS")
print("\n".join(pivots))
print("PARENTS")
print("\n".join(parents))
print("PARTS")
print("\n".join(parts))
