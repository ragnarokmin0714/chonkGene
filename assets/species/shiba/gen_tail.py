# Prints the four-legged Shiba's tail for quad.ron: `python3 gen_tail.py`,
# then paste the PIVOTS, PARENTS and PARTS sections over the tail entries
# there.
#
# The first version's fat doughnut sitting on the rump, seen side on, the
# hole showing, cream on the inside, but open: only its root, sunk into
# the rump, joins the body. The ring sits just above the back and goes 270
# degrees round; the tip stops a gap short of the root and of the back, so
# the end of the tail never melts into the body.
import math


def f(v):
    return "(" + ", ".join((f"{c:.3f}".rstrip("0").rstrip(".") if abs(c) >= 5e-4 else "0") for c in v) + ")"


R = 0.2  # the ring's radius, as the first version's torus
SWEEP = 270.0  # degrees round from the bottom, back first, over and forward
ARCS = 8
# The root: from inside the rump up to the bottom of the ring.
stalk_from = [0.06, -0.02, -0.86]
stalk_to = [0.06, 0.1, -0.86]
centre = [0.06, stalk_to[1] + R, stalk_to[2]]
# Tube radius: as fat as the first version's (0.12) all the way round,
# rounding off a little at the tip.
radii = [0.12] * ARCS + [0.105]
points = [stalk_from, stalk_to]
for k in range(1, ARCS + 1):
    t = math.radians(SWEEP * k / ARCS)
    points.append([centre[0], centre[1] - R * math.cos(t), centre[2] - R * math.sin(t)])
names = ["tail"] + [f"tail_{k}" for k in range(2, ARCS + 1)] + ["tail_tip"]
pivots, parents, parts = [], [], []
for k, name in enumerate(names):
    a, b = points[k], points[k + 1]
    to = [y - x for x, y in zip(a, b)]
    inner = [c - x for c, x in zip(centre, a)]
    pivots.append(f'        "{name}": {f(a)},')
    parents.append(f'        "{name}": "{"hips" if k == 0 else names[k - 1]}",')
    # The root blends into the rump; the ring's segments only into each
    # other, and the tip hardly at all, so it stays a gap away from the
    # root and the back.
    blend = 0.1 if k == 0 else 0.03 if k == len(names) - 1 else 0.06
    parts.append(
        f'        (name: "{name}", bone: "{name}", shape: Capsule(to: {f(to)}, r: {radii[k]}), at: {f(a)},\n'
        f'            blend: {blend}, color: "fur", zone: "bottom",\n'
        f'            marks: [(at: {f(inner)}, radii: (0.2, 0.2, 0.2), color: "light", soft: 0.5)]),'
    )
print("PIVOTS")
print("\n".join(pivots))
print("PARENTS")
print("\n".join(parents))
print("PARTS")
print("\n".join(parts))
