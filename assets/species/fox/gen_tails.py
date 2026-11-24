# Prints the fox's nine tails for quad.ron: `python3 gen_tails.py | python3
# ../splice.py quad.ron`; for her human form, `python3 gen_tails.py human |
# python3 ../splice.py human.ron` (the same plumes a little smaller, from
# the small of her back, the fan a little narrower so the outer ones stay
# off the floor).
#
# Nine fat plumes opening behind her like the petals of a lotus, as in
# Roger's picture of her parts (PLAN §8, twenty-second pass, item 3,
# 「3D示意與局部拆解圖」: 九尾巴（分件）): each a plump teardrop of fluff,
# narrow where it leaves the round puff at her tail root, fullest a little
# under half way (near the width of her head) and drawn to a point that
# flicks up like a flame's; the top ones upright, the outer ones lying out
# sideways, all leaning back, packed close so they overlap as petals do;
# flattened a little, as a brush of fur is; the centre line an S.
#
# The centre line is sampled into round cones whose radii meet and which
# carry on in the same direction, so a plume is one smooth surface; they
# blend only a hair, widely only at the root. Five bones a tail, three
# cones each (MAX_BONES is 64: ten for the body, 45 for the tails), so a
# wave can run down it with some follow-through.
# White at the root, through the middle colour, to the tip colour.
import math


def norm(v):
    l = math.sqrt(sum(c * c for c in v))
    return [c / l for c in v]


def add(a, b, k=1.0):
    return [x + y * k for x, y in zip(a, b)]


def f(v):
    return "(" + ", ".join((f"{c:.3f}".rstrip("0").rstrip(".") if abs(c) >= 5e-4 else "0") for c in v) + ")"


import sys

# The fan for each form: where the nine meet, how long and fat the plumes
# are against the beast form's, how wide the fan opens either side of
# upright (degrees). figure_blend.py builds her whole-figure tails along
# the same lines, from her own root (`line`, `radius`).
PRESETS = {
    "quad": dict(base=(0.0, 0.1, -0.88), length=1.9, wide=1.0, spread=112.0),
    "human": dict(base=(0.0, -0.3, -0.3), length=1.55, wide=0.8, spread=96.0),
}
BONES = 5
PER = 3  # cones to a bone
CONES = BONES * PER
SEG = ["", "_b", "_c", "_d", "_tip"]
FLAT = 1.2


def radius(s, wide=1.0):
    """A plume's half-width a fraction `s` of the way out: a narrow root
    swelling quickly to its fullest at 0.42, then easing in to a point."""
    if s < 0.42:
        return wide * (0.1 + 0.33 * math.sin(0.5 * math.pi * s / 0.42) ** 0.9)
    return wide * (0.03 + 0.4 * max(0.0, math.cos(0.5 * math.pi * (s - 0.42) / 0.58)) ** 1.25)


def bend(s):
    """How the centre line turns toward upright (+) or away (-), and up,
    per unit length, a fraction `s` of the way out: the S, out and down
    from the root, back up through the middle, the tip flicking up."""
    if s < 0.3:
        return -0.8, 0.1
    if s < 0.72:
        return 0.6, 0.0
    return 2.2, 0.3


def line(i, base, length, spread, steps=CONES, lean=1.0):
    """Tail `i`'s centre line (0 out to her right … 8 her left) in `steps`
    equal steps, root to point: (start, direction, step length, s at the
    start, s at the end) for each step. `lean` > 1 sets the plumes off
    further back from her."""
    th = math.radians(-spread + i * 2 * spread / 8)
    # Upright ones the longest, the outer a little shorter; no two alike.
    size = (1.0 - 0.06 * abs(math.sin(th))) * (1.0 + 0.05 * math.sin(i * 2.3))
    # The outer ones droop no lower than her knees: lower, they would count
    # as standing on the floor and spread her shadow (mesh.rs, `reach`).
    d = norm([1.15 * math.sin(th), 0.45 + 0.9 * math.cos(th), -lean])
    # The way it curls: across itself in the fan, toward upright (up for
    # the outer ones, in for the top ones; turning toward its own way out
    # would not turn it at all), and a little back.
    sgn = 1.0 if th >= 0.0 else -1.0
    out = norm([-sgn * math.cos(th), abs(math.sin(th)), -0.3])
    p = list(base)
    step = length * size / steps
    for k in range(steps):
        s0, s1 = k / steps, (k + 1) / steps
        # Turn half before and half after the step (midpoint rule), so the
        # cones meet at the same angle on both sides of each joint.
        o, up = bend((s0 + s1) / 2)
        d = norm(add(add(d, out, o * step * 0.5), [0, 1, 0], up * step * 0.5))
        yield p, d, step, s0, s1
        p = add(p, [x * step for x in d])
        d = norm(add(add(d, out, o * step * 0.5), [0, 1, 0], up * step * 0.5))


def ron(base, length, wide, spread):
    """The nine plumes as round cones for a model file, and their lowest
    point."""
    pivots, parents, parts = [], [], []
    lowest = 0.0
    for i in range(9):
        names = [f"tail{i + 1}{s}" for s in SEG]
        for k, (p, d, step, s0, s1) in enumerate(line(i, base, length, spread)):
            bone = names[k // PER]
            if k % PER == 0:
                pivots.append(f'        "{bone}": {f(p)},')
                parents.append(f'        "{bone}": "{"hips" if k == 0 else names[k // PER - 1]}",')
            g = -math.degrees(math.asin(max(-1, min(1, d[0]))))
            a = math.degrees(math.atan2(d[2], d[1]))
            # One soft gradient up the plume: the middle colour from past half
            # way, the tip colour over the last part (big soft spots, no bands).
            marks = ""
            if k == CONES // 2 + 1:
                marks = f',\n            marks: [(at: (0, {step:.3f}, 0), radii: (0.5, 0.45, 0.5), color: "tailmid", soft: 0.9)]'
            if k == CONES - 3:
                marks = f',\n            marks: [(at: (0, 0, 0), radii: (0.45, 0.4, 0.45), color: "tailmid", soft: 0.9)]'
            if k == CONES - 1:
                marks = (
                    f',\n            marks: [(at: (0, 0, 0), radii: (0.35, 0.3, 0.35), color: "tailmid", soft: 0.9),'
                    f'\n                (at: (0, {step:.3f}, 0), radii: (0.3, {step * 1.6:.3f}, 0.3), color: "tailtip", soft: 0.7)]'
                )
            name = bone if k % PER == 0 else f"{bone}_{k % PER + 1}"
            parts.append(
                f'        (name: "{name}", bone: "{bone}", shape: Cone(r1: {radius(s0, wide):.3f}, r2: {radius(s1, wide):.3f}, h: {step:.3f}, flat: {FLAT}), at: {f(p)},\n'
                f'            rot: {f([a, 0, g])}, blend: {0.1 if k == 0 else 0.02}, color: "fur", zone: "bottom"{marks}),'
            )
            end = add(p, [x * step for x in d])
            lowest = min(lowest, end[1] - radius(s1, wide))
    return pivots, parents, parts, lowest


if __name__ == "__main__":
    preset = PRESETS["human" if sys.argv[1:] == ["human"] else "quad"]
    pivots, parents, parts, lowest = ron(**preset)
    print(f"lowest point of the tails: y {lowest:.3f}", file=sys.stderr)
    print("PIVOTS")
    print("\n".join(pivots))
    print("PARENTS")
    print("\n".join(parents))
    print("PARTS")
    print("\n".join(parts))
