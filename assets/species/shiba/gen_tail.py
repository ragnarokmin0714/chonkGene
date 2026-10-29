# Prints the four-legged Shiba's tail for quad.ron: `python3 gen_tail.py`,
# then paste the PIVOTS, PARENTS and PARTS sections over the tail entries
# there.
#
# A doughnut that does not close: a short stalk up off the rump, then a
# ring 270 degrees round, seen side on, the hole showing, cream on
# the inside. The tip stops short of the stalk and stays clear of the back,
# so the end of the tail never melts into the body.
import math


def f(v):
    return "(" + ", ".join((f"{c:.3f}".rstrip("0").rstrip(".") if abs(c) >= 5e-4 else "0") for c in v) + ")"


R = 0.22  # the ring's radius
SWEEP = 270.0  # degrees round from the bottom, back first, over and forward
ARCS = 6
stalk_from = [0.03, 0.04, -0.85]
stalk_to = [0.03, 0.2, -0.88]
centre = [0.04, stalk_to[1] + R, stalk_to[2]]
# Tube radius from the stalk to the tip: fat at the root, thin at the end.
radii = [0.13, 0.14, 0.14, 0.135, 0.125, 0.11, 0.09]
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
    # The stalk blends into the rump; the ring's segments only into each
    # other, so the tip stays a gap away from the stalk and the back.
    blend = 0.1 if k == 0 else 0.05
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
