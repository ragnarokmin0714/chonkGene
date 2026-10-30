# Shared by the species' gen_tail.py: a tail as a chain of round-cone bones
# along a path, so it is one smooth surface that bends at every joint (the
# fox's tails and the Shiba's ring are made the same way).
import math


def f(v):
    return "(" + ", ".join((f"{c:.3f}".rstrip("0").rstrip(".") if abs(c) >= 5e-4 else "0") for c in v) + ")"


def turn_path(start, heading, bend, length, n):
    """Points of a path of `n` equal segments: from `start`, first heading
    `heading` degrees (0 = up +y, 90 = forward +z, -90 = back), turning
    `bend` degrees in all, total length `length`. In the y-z plane at x."""
    pts = [list(start)]
    step = length / n
    for k in range(n):
        a = math.radians(heading + bend * (k + 0.5) / n)
        p = pts[-1]
        pts.append([p[0], p[1] + step * math.cos(a), p[2] + step * math.sin(a)])
    return pts


def emit(points, radii, names, parent="hips", blend=0.1, color="fur", mark=None, zone="bottom"):
    """Print the PIVOTS, PARENTS and PARTS for a chain through `points`
    (one more than there are `names`), the bone k thickening from
    `radii[k]` to `radii[k + 1]` along its segment. `mark(k, to)` may
    return the text of a `marks: [...]` list for a part."""
    pivots, parents, parts = [], [], []
    for k, name in enumerate(names):
        a, b = points[k], points[k + 1]
        d = [y - x for x, y in zip(a, b)]
        h = math.sqrt(sum(c * c for c in d))
        u = [c / h for c in d]
        # A cone along +y, turned onto u: roll about z, then pitch about x.
        g = -math.degrees(math.asin(max(-1, min(1, u[0]))))
        p = math.degrees(math.atan2(u[2], u[1]))
        pivots.append(f'        "{name}": {f(a)},')
        parents.append(f'        "{name}": "{parent if k == 0 else names[k - 1]}",')
        m = mark(k, h) if mark else ""
        parts.append(
            f'        (name: "{name}", bone: "{name}", shape: Cone(r1: {radii[k]}, r2: {radii[k + 1]}, h: {h:.3f}), at: {f(a)},\n'
            f'            rot: {f([p, 0, g])}, blend: {blend}, color: "{color}", zone: "{zone}"{"," + chr(10) + "            " + m if m else ""}),'
        )
    print("PIVOTS")
    print("\n".join(pivots))
    print("PARENTS")
    print("\n".join(parents))
    print("PARTS")
    print("\n".join(parts))
