# Writes human.ron, the fox's human form (人型形態), then her tails into it:
#   python3 gen_human.py && python3 gen_tails.py human | python3 ../splice.py human.ron
#
# After Roger's picture of her parts (…3D示意與局部拆解圖2.jpeg, both the
# 銀白版 and the 紅髮版) and the 三視圖. Everything that has to sit on a
# surface (hair strands on the head, borders on the robe, the collar, the
# ear fluff) is placed here, so the shapes can be tuned in one place:
#
# - the head: a wide, round, mochi face taking the lower part of the head,
#   a little button nose and a mound for the mouth; the eyes, marks and
#   blush are painted (face: `chibi`, `seal`);
# - the hair: a cap set back off the forehead, big glossy clumps over the
#   back and sides that flick out at their ends, a centre-parted curtain
#   fringe sweeping out to the temples, locks framing the face down to the
#   chin; per hairdo a high ponytail, two bunches, or long hair to the
#   waist (公主頭), each with a thin red ribbon;
# - big fox ears tilted out, the colour of her hair, white fluff inside;
# - the robe (古風小長衫): an A-line from narrow shoulders to a wide flat
#   hem (frustums: a round cone would end in a ball), a thick border at the
#   hem with a gold line and a gold scroll of dots, the crossed collar's
#   lapels running down into the border down the front, trumpet sleeves
#   with bordered cuffs and a braided band round each upper arm, a wide
#   sash with a braided rope round it tied in a bow of two rope rings, a
#   bell under the knot and two tassels;
# - short legs and bare feet.
import math

OUT = "human.ron"


def f(v):
    return "(" + ", ".join(fmt(c) for c in v) + ")"


def fmt(c):
    if abs(c) < 5e-4:
        return "0"
    return f"{c:.3f}".rstrip("0").rstrip(".")


def add(a, b, k=1.0):
    return [x + y * k for x, y in zip(a, b)]


def sub(a, b):
    return [x - y for x, y in zip(a, b)]


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def norm(v):
    l = math.sqrt(dot(v, v))
    return [c / l for c in v]


def frame(d, n):
    """Euler angles (degrees, as M3::euler: turn y · pitch x · roll z) that
    take a part's +y along `d` and, as near as can be, its +z along `n`."""
    d = norm(d)
    n = norm(sub(n, [c * dot(n, d) for c in d]))
    # M e_z = (sin b cos a, -sin a, cos b cos a)
    a = math.asin(max(-1.0, min(1.0, -n[1])))
    b = math.atan2(n[0], n[2])
    # undo the turn and the pitch on d to read the roll: Rz e_y = (-sin g, cos g)
    cb, sb = math.cos(-b), math.sin(-b)
    v = [cb * d[0] + sb * d[2], d[1], -sb * d[0] + cb * d[2]]
    ca, sa = math.cos(-a), math.sin(-a)
    v = [v[0], ca * v[1] - sa * v[2], sa * v[1] + ca * v[2]]
    g = math.atan2(-v[0], v[1])
    return [math.degrees(a), math.degrees(b), math.degrees(g)]


parts = []


def part(name, bone, shape, at, color, zone, rot=None, blend=0.0, gloss=0.0, fuzz=None,
         chonk=0.0, mirror=False, style=None, marks=None):
    s = f'        (name: "{name}", bone: "{bone}", shape: {shape}, at: {f(at)},\n            '
    bits = []
    if rot is not None and any(abs(r) > 5e-4 for r in rot):
        bits.append(f"rot: {f(rot)}")
    bits.append(f"blend: {fmt(blend)}")
    bits.append(f'color: "{color}"')
    if gloss:
        bits.append(f"gloss: {fmt(gloss)}")
    if fuzz is not None:
        bits.append(f"fuzz: {fmt(fuzz)}")
    if chonk:
        bits.append(f"chonk: {fmt(chonk)}")
    if mirror:
        bits.append("mirror: true")
    if style:
        bits.append(f'style: Some("{style}")')
    bits.append(f'zone: "{zone}"')
    s += ", ".join(bits)
    if marks:
        s += ",\n            marks: [\n" + "\n".join(
            f"                (at: {f(m[0])}, radii: {f(m[1])}, color: \"{m[2]}\", soft: {fmt(m[3])}),"
            for m in marks
        ) + "\n            ]"
    s += "),"
    parts.append(s)


def comment(text):
    parts.append("        // " + text)


def ell(r):
    return f"Ellipsoid({f(r)})"


def cone(r1, r2, h, flat=1.0):
    return f"Cone(r1: {fmt(r1)}, r2: {fmt(r2)}, h: {fmt(h)}, flat: {fmt(flat)})"


def frustum(r1, r2, h, rnd):
    return f"Frustum(r1: {fmt(r1)}, r2: {fmt(r2)}, h: {fmt(h)}, round: {fmt(rnd)})"


def torus(major, minor):
    return f"Torus(major: {fmt(major)}, minor: {fmt(minor)})"


def strand(name, bone, a, b, r1, r2, out, color="hair", flat=1.8, style=None, zone="head",
           tip=True, blend=0.012, mirror=True):
    """A clump of hair from `a` to `b`, flattened against the head (its
    face turned toward `out`), the tip fading to the tip colour."""
    d = sub(b, a)
    h = math.sqrt(dot(d, d))
    marks = [([0, h, 0], [r2 * 4 + 0.1, 0.38 * h, r2 * 4 + 0.1], "hairtip", 0.8)] if tip else None
    part(name, bone, cone(r1, r2, h, flat), a, color, zone, rot=frame(d, out), blend=blend,
         gloss=0.45, fuzz=0.05, mirror=mirror, style=style, marks=marks)


def sheet(name, bone, pts, widths, thick, style=None, zone="head", mirror=True, tip=0.12):
    """A flat lock of hair through the points `pts` (each (x, y) on the
    head, how far out), as overlapping flat ellipsoids lying on the head,
    `widths` across at each point, ending in a point `tip` long."""
    if pts[0][0] == "xyz":
        ps, widths = smooth([list(p[1:]) for p in pts], widths, 2)
        placed = [(p, norm([p[0], 0.0, p[2] + 0.15])) for p in ps]
    elif pts[0][0] == "cap":
        raw = [around_cap(phi, th, off) for (_, phi, th, off) in pts]
        ps, widths = smooth([p for p, _ in raw], widths, 2)
        ns, _ = smooth([n for _, n in raw], [0.0] * len(raw), 2)
        placed = [(p, norm(n)) for p, n in zip(ps, ns)]
    else:
        xy, widths = smooth([[x, y, off] for (x, y, off) in pts], widths, 2)
        placed = [on_hair(x, y, off) for (x, y, off) in xy]
    for i in range(len(placed) - 1):
        (a, na), (b, nb) = placed[i], placed[i + 1]
        mid = add(a, sub(b, a), 0.5)
        d = sub(b, a)
        half = 0.5 * math.sqrt(dot(d, d)) * 1.7
        w = 0.5 * (widths[i] + widths[i + 1])
        n = norm(add(na, nb))
        last = i >= len(placed) - 3
        marks = [([0, half * 0.7, 0], [w * 1.6, half * 1.4, w * 1.6], "hairtip", 0.8)] if last and tip else None
        part(f"{name}{i}", bone, ell([w, half, thick]), mid, "hair", zone, rot=frame(d, n),
             blend=0.04, gloss=0.45, fuzz=0.05, mirror=mirror, style=style, marks=marks)
    if tip:
        (a, na), (b, nb) = placed[-2], placed[-1]
        d = norm(sub(b, a))
        part(f"{name}_tip", bone, cone(widths[-1] * 0.8, 0.012, tip, widths[-1] / thick * 0.6 + 1.0),
             add(b, d, -0.03), "hairtip", zone, rot=frame(d, nb), blend=0.03, gloss=0.45, fuzz=0.05,
             mirror=mirror, style=style)


def smooth(pts, radii, k):
    """`k` points between each two of `pts` along a Catmull-Rom curve, the
    radii in between eased the same way."""
    ext = [add(pts[0], sub(pts[0], pts[1]))] + pts + [add(pts[-1], sub(pts[-1], pts[-2]))]
    rx = [radii[0]] + radii + [radii[-1]]
    out, rad = [], []
    for i in range(1, len(ext) - 2):
        p0, p1, p2, p3 = ext[i - 1], ext[i], ext[i + 1], ext[i + 2]
        for j in range(k):
            t = j / k
            t2, t3 = t * t, t * t * t
            out.append([0.5 * ((2 * p1[c]) + (-p0[c] + p2[c]) * t + (2 * p0[c] - 5 * p1[c] + 4 * p2[c] - p3[c]) * t2
                               + (-p0[c] + 3 * p1[c] - 3 * p2[c] + p3[c]) * t3) for c in range(3)])
            rad.append(rx[i] + (rx[i + 1] - rx[i]) * t)
    out.append(pts[-1])
    rad.append(radii[-1])
    return out, rad


def tube(name, bone, pts, radii, style, flat=1.15, up=(0, 0, 1)):
    """A round bunch of hair along the points `pts`, `radii` thick at each:
    capsules end to end along a smoothed curve (ellipsoids left ridges where
    they met), so it bends and swells smoothly, the last part
    fading to the tip colour, ending in a point."""
    pts, radii = smooth(pts, radii, 3)
    for i in range(len(pts) - 1):
        a, b = pts[i], pts[i + 1]
        d = sub(b, a)
        half = 0.5 * math.sqrt(dot(d, d)) * 1.6
        r = 0.5 * (radii[i] + radii[i + 1])
        last = i >= len(pts) - 4
        marks = [(add(d, d, -0.5), [r * 2.2, half * 1.4, r * 2.2], "hairtip", 0.8)] if last else None
        part(f"{name}{i}", bone, f"Capsule(to: {f(d)}, r: {fmt(r)})", a, "hair", "head",
             blend=0.04, gloss=0.45, fuzz=0.05, style=style, marks=marks)
    a, b = pts[-2], pts[-1]
    d = norm(sub(b, a))
    part(f"{name}_tip", bone, cone(radii[-1], 0.012, 0.16, flat), add(b, d, -0.04), "hairtip",
         "head", rot=frame(d, up), blend=0.03, gloss=0.45, fuzz=0.05, style=style)


# ---- the head ----------------------------------------------------------------
HEAD_C = [0.0, 0.58, 0.0]
HEAD_R = [0.72, 0.61, 0.59]
CAP_C = [0.0, 0.76, -0.17]
CAP_R = [0.76, 0.62, 0.68]


def on_ellipsoid(c, r, x, y, front=True):
    """The point at (x, y) on the front (or back) of an ellipsoid."""
    k = 1.0 - ((x - c[0]) / r[0]) ** 2 - ((y - c[1]) / r[1]) ** 2
    z = r[2] * math.sqrt(max(k, 0.0))
    return [x, y, c[2] + (z if front else -z)]


def on_hair(x, y, off=0.0, front=True):
    """On the outside of the head and the hair cap at (x, y), `off` further
    out; and which way is out there."""
    p = on_ellipsoid(CAP_C, CAP_R, x, y, front)
    q = on_ellipsoid(HEAD_C, HEAD_R, x, y, front)
    best, c, r = (p, CAP_C, CAP_R) if (p[2] > q[2]) == front else (q, HEAD_C, HEAD_R)
    n = norm([(best[i] - c[i]) / (r[i] * r[i]) for i in range(3)])
    return add(best, n, off), n


def around_cap(phi, theta, off=0.0):
    """On the hair cap at turn `phi` (0 facing you) and height `theta`
    (degrees), `off` further out; and which way is out."""
    p, t = math.radians(phi), math.radians(theta)
    u = [math.sin(p) * math.cos(t), math.sin(t), math.cos(p) * math.cos(t)]
    pt = [CAP_C[i] + CAP_R[i] * u[i] for i in range(3)]
    n = norm([u[i] / CAP_R[i] for i in range(3)])
    return add(pt, n, off), n


def face_point(u, v, off=0.0):
    w = math.sqrt(max(0.0, 1.0 - u * u - v * v))
    n = [u, v, w]
    return [HEAD_C[i] + HEAD_R[i] * n[i] + off * n[i] for i in range(3)]


# ---- the robe ------------------------------------------------------------------
HEM_Y, WAIST_Y, SHOULDER_Y = -0.74, -0.31, 0.03
HEM_R, WAIST_R, SHOULDER_R = 0.47, 0.335, 0.24


def robe_r(y):
    if y <= WAIST_Y:
        return HEM_R + (WAIST_R - HEM_R) * (y - HEM_Y) / (WAIST_Y - HEM_Y)
    return WAIST_R + (SHOULDER_R - WAIST_R) * (y - WAIST_Y) / (SHOULDER_Y - WAIST_Y)


def on_robe(x, y, base_y, off=0.0):
    """A point on the robe's front at (x, y), in the frame of the robe part
    whose foot is at height `base_y`."""
    r = robe_r(y) + off
    z = math.sqrt(max(r * r - x * x, 0.0))
    return [x, y - base_y, z]


def trim_line(a, b, base_y, rad, step, color="trim", soft=0.25):
    """Round marks every `step` from (x, y) `a` to `b` on the robe's front."""
    n = max(2, int(math.dist(a, b) / step) + 1)
    out = []
    for i in range(n):
        t = i / (n - 1)
        x, y = a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t
        out.append((on_robe(x, y, base_y), [rad, rad, rad], color, soft))
    return out


comment("The robe: an A-line from narrow shoulders to a wide flat hem.")
skirt_h = WAIST_Y - HEM_Y + 0.02
skirt_marks = [
    # The border at the hem: a band, a gold line along its top, a gold
    # scroll of dots along it.
    ([0, 0.055, 0], [0.9, 0.075, 0.9], "trim", 0.12),
    ([0, 0.14, 0], [0.9, 0.011, 0.9], "gold", 0.3),
    ([0, 0.006, 0], [0.9, 0.008, 0.9], "gold", 0.3),
]
for k in range(20):
    a = 2 * math.pi * k / 20
    r = robe_r(HEM_Y + 0.06) + 0.005
    skirt_marks.append(([r * math.sin(a), 0.07 if k % 2 else 0.045, r * math.cos(a)],
                        [0.022, 0.016, 0.022], "gold", 0.35))
# The front's edge, where the robe laps over: the border down to the hem.
skirt_marks += trim_line((0.13, WAIST_Y - 0.04), (0.17, HEM_Y + 0.08), HEM_Y, 0.05, 0.035)
skirt_marks += trim_line((0.19, WAIST_Y - 0.04), (0.235, HEM_Y + 0.08), HEM_Y, 0.009, 0.02, "gold", 0.3)
part("skirt", "hips", frustum(HEM_R, WAIST_R, skirt_h, 0.05), [0, HEM_Y, 0], "robe", "back",
     fuzz=0.2, chonk=0.4, marks=skirt_marks)
chest_h = SHOULDER_Y - WAIST_Y
chest_marks = []
# The crossed collar: the left lapel over the right, a broad red band
# from each shoulder to the sash, a gold line inside it.
chest_marks += trim_line((-0.17, SHOULDER_Y - 0.01), (0.13, WAIST_Y + 0.03), WAIST_Y, 0.05, 0.03)
chest_marks += trim_line((0.17, SHOULDER_Y - 0.01), (0.0, WAIST_Y + 0.17), WAIST_Y, 0.05, 0.03)
chest_marks += trim_line((-0.12, SHOULDER_Y - 0.03), (0.17, WAIST_Y + 0.04), WAIST_Y, 0.009, 0.02, "gold", 0.3)
# The undergarment showing in the V.
part("chest", "body", frustum(WAIST_R, SHOULDER_R, chest_h, 0.07), [0, WAIST_Y, 0], "robe", "back",
     blend=0.05, fuzz=0.2, chonk=0.4, marks=chest_marks)
part("shoulders", "body", ell([0.3, 0.09, 0.22]), [0, SHOULDER_Y - 0.03, 0], "robe", "back",
     blend=0.07, fuzz=0.2, chonk=0.3)
part("neck", "body", ell([0.1, 0.1, 0.1]), [0, SHOULDER_Y + 0.05, 0.0], "skin", "chin", blend=0.04, fuzz=0)

comment("The sash, a braided rope round it, tied in a bow of two rope rings, a bell, two tassels.")
SASH_Y = -0.38
sash_marks = []
part("sash", "body", frustum(0.352, 0.345, 0.15, 0.03), [0, SASH_Y, 0], "sash", "back",
     blend=0.02, fuzz=0.15, chonk=0.4)
rope = []
for k in range(36):
    a = 2 * math.pi * k / 36
    rope.append(([0.36 * math.sin(a), 0.0, 0.36 * math.cos(a)], [0.022, 0.04, 0.022], "gold", 0.35))
part("sash_rope", "body", torus(0.362, 0.026), [0, SASH_Y + 0.075, 0], "sash", "back",
     blend=0.01, fuzz=0.1, gloss=0.3, chonk=0.4, marks=rope)
for side in (1,):
    centre = [0.115, SASH_Y + 0.085, 0.385]
    ring = []
    for k in range(16):
        a = 2 * math.pi * k / 16
        ring.append(([0.09 * math.cos(a), 0.0, 0.09 * math.sin(a)], [0.022, 0.045, 0.022], "gold", 0.35))
    part("bow", "body", torus(0.09, 0.034), centre, "sash", "back", rot=[90, 0, 18],
         blend=0.01, fuzz=0.1, gloss=0.3, mirror=True, marks=ring)
part("bow_knot", "body", ell([0.055, 0.06, 0.05]), [0, SASH_Y + 0.08, 0.4], "sash", "back",
     blend=0.02, fuzz=0.1, marks=[([0, 0, 0.04], [0.06, 0.012, 0.03], "gold", 0.3)])
part("tassel_cord", "body", f"Capsule(to: {f([0.02, -0.12, 0.01])}, r: 0.013)", [0.035, SASH_Y + 0.04, 0.4],
     "sash", "back", blend=0.01, fuzz=0.1, mirror=True)
part("tassel", "body", cone(0.022, 0.04, 0.1), [0.055, SASH_Y - 0.08, 0.41], "sash", "back",
     rot=[180, 0, 0], blend=0.015, fuzz=0.4, mirror=True,
     marks=[([0, 0.0, 0], [0.04, 0.02, 0.04], "gold", 0.3)])
part("bell", "bell", ell([0.08, 0.08, 0.08]), [0, SASH_Y - 0.03, 0.43], "gold", "back",
     blend=0.01, gloss=0.9, fuzz=0,
     marks=[([0, 0.0, 0.065], [0.08, 0.008, 0.05], "ink", 0.3),
            ([0, -0.035, 0.065], [0.009, 0.028, 0.03], "ink", 0.3),
            ([0.032, 0.038, 0.055], [0.022, 0.02, 0.02], "robe", 0.5)])

comment("Trumpet sleeves, a bordered cuff, a braided band round each upper arm, hands.")
for side, name in ((1, "l"), (-1, "r")):
    sh = [0.25 * side, SHOULDER_Y - 0.04, 0.0]
    d = norm([math.sin(math.radians(36)) * side, -math.cos(math.radians(36)), 0.12])
    rot = frame(d, [0, 0, 1])
    L = 0.42
    part(f"sleeve_{name}", f"arm_{name}", frustum(0.105, 0.2, L, 0.04), sh, "robe", "paw",
         rot=rot, blend=0.06, fuzz=0.2, chonk=0.2,
         marks=[([0, L - 0.05, 0], [0.5, 0.055, 0.5], "trim", 0.12),
                ([0, L - 0.115, 0], [0.5, 0.009, 0.5], "gold", 0.3)])
    band = add(sh, d, 0.08)
    part(f"armband_{name}", f"arm_{name}", torus(0.112, 0.024), band, "sash", "paw", rot=rot,
         blend=0.01, fuzz=0.1, marks=[([0.11, 0, 0], [0.03, 0.04, 0.12], "gold", 0.4),
                                      ([-0.11, 0, 0], [0.03, 0.04, 0.12], "gold", 0.4)])
    hand = add(sh, d, L + 0.02)
    part(f"hand_{name}", f"arm_{name}", ell([0.065, 0.075, 0.06]), hand, "skin", "paw",
         blend=0.01, fuzz=0)

comment("Short legs and bare feet (短短的腳丫).")
for side, name in ((1, "l"), (-1, "r")):
    part(f"leg_{name}", f"leg_{name}", f"Capsule(to: {f([0, -0.3, 0.01])}, r: 0.1)",
         [0.13 * side, HEM_Y + 0.04, 0], "skin", "paw", blend=0.03, fuzz=0, chonk=0.3)
    part(f"foot_{name}", f"leg_{name}", ell([0.088, 0.06, 0.13]), [0.135 * side, -1.04, 0.05],
         "skin", "paw", blend=0.05, fuzz=0)

comment("The big round head, a button nose, a mound for the mouth.")
part("head", "head", ell(HEAD_R), HEAD_C, "skin", "head", blend=0.05, fuzz=0)
part("nose", "head", ell([0.045, 0.035, 0.03]), face_point(0.0, -0.3, -0.012), "skin", "head",
     blend=0.03, fuzz=0)
part("mouth", "head", ell([0.13, 0.075, 0.045]), face_point(0.0, -0.5, -0.03), "skin", "chin",
     blend=0.05, fuzz=0)

comment("Round cheeks widening the lower face (out at the sides, where nothing is painted).")
part("cheek", "head", ell([0.22, 0.2, 0.22]), [0.46, 0.32, 0.13], "skin", "cheek", blend=0.12, fuzz=0,
     mirror=True)
comment("Hair: a cap set back off the forehead.")
part("hair", "head", ell(CAP_R), CAP_C, "hair", "head", blend=0.008, gloss=0.45, fuzz=0.05)

comment("Big clumps over the back and sides, flicking out at their ends (a bob to the chin).")
for i, phi in enumerate((70, 98, 126, 154)):
    a, n = around_cap(phi, 12, -0.07)
    b, _ = around_cap(phi + 3, -44, 0.07)
    strand(f"clump{i}", "head", a, b, 0.27, 0.07, n, flat=2.6)
a, n = around_cap(180, 12, -0.07)
b, _ = around_cap(180, -44, 0.07)
strand("clump_back", "head", a, b, 0.28, 0.08, n, mirror=False, flat=2.6)

comment("Locks framing the face, from the temples to the chin.")
for st, low in (("ponytail", 0.14), ("twintails", 0.14), ("princess", -0.16)):
    pts = [("xyz", 0.6, 0.94, 0.3), ("xyz", 0.71, 0.64, 0.23), ("xyz", 0.78, 0.38, 0.11),
           ("xyz", 0.79, low, 0.05)]
    sheet("lock" + ("" if st == "ponytail" else "_" + st[:4]), "head", pts,
          [0.14, 0.17, 0.16, 0.1], 0.05, style=st, zone="cheek")
sheet("side", "head", [("cap", 84, 20, -0.02), ("cap", 90, -10, 0.02), ("cap", 92, -36, 0.05),
                       ("cap", 92, -58, 0.09)], [0.15, 0.18, 0.17, 0.1], 0.06, zone="cheek")

comment("The fringe: two curtains parted in the middle, sweeping out over the forehead's sides to the temples.")
sheet("bang", "head", [(0.05, 1.24, 0.0), (0.22, 1.12, 0.035), (0.4, 0.96, 0.045), (0.53, 0.76, 0.05),
                       (0.6, 0.56, 0.05)], [0.09, 0.16, 0.19, 0.16, 0.09], 0.05)
comment("Strands of the fringe falling over the forehead's sides (the middle stays bare for the crest).")
for i, (x, top, low, w) in enumerate(((0.2, 1.12, 0.8, 0.07), (0.37, 1.04, 0.68, 0.075))):
    sheet(f"fringe{i}_", "head", [(x - 0.04, top, 0.02), (x, top - 0.12, 0.035), (x + 0.03, low, 0.035)],
          [w, w * 1.1, w * 0.7], 0.035, tip=0.09)

comment("Big fox ears tilted out, the colour of her hair, white fluff inside.")
EAR_H = 0.68
for side, name in ((1, "l"), (-1, "r")):
    base = [0.44 * side, 1.08, -0.1]
    d = norm([math.sin(math.radians(28)) * side, math.cos(math.radians(28)), 0.08])
    rot = frame(d, [0.15 * side, -0.1, 1])
    part(f"ear_{name}", f"ear_{name}", cone(0.34, 0.06, EAR_H, 2.4), base, "hair", "head", rot=rot,
         blend=0.06, fuzz=0.5,
         marks=[([0, 0.26, 0.09], [0.19, 0.3, 0.06], "blush", 0.45),
                ([0, EAR_H + 0.04, 0], [0.16, 0.16, 0.16], "hairtip", 0.7)])
    # Fluff: soft cones up from the inside of the ear's foot.
    x_axis = norm(cross(d, [0.15 * side, -0.1, 1]))
    z_axis = norm(cross(x_axis, d))
    for j, (u, v, ln, r) in enumerate(((0.0, 0.04, 0.42, 0.11), (0.13, 0.0, 0.33, 0.09),
                                       (-0.13, 0.0, 0.33, 0.09), (0.06, 0.2, 0.28, 0.075),
                                       (-0.07, 0.16, 0.26, 0.07))):
        at = add(add(add(base, x_axis, u * side), d, v), z_axis, 0.12)
        dd = norm(add(d, x_axis, -0.35 * u / 0.09 * side if u else 0.0))
        part(f"fluff{j}_{name}", f"ear_{name}", cone(r, 0.012, ln, 1.6), at, "fluff", "head",
             rot=frame(dd, z_axis), blend=0.03, fuzz=0.9)

comment("綁單馬尾: one ponytail tied high on the crown, a thin red ribbon.")
knot = [0.0, 1.3, -0.3]
part("pony_knot", "hair_b", ell([0.19, 0.16, 0.18]), knot, "hair", "head", blend=0.06, gloss=0.45,
     fuzz=0.05, style="ponytail")
tube("pony", "hair_b", [[0, 1.36, -0.36], [0, 1.4, -0.6], [0, 1.24, -0.84], [0, 0.96, -0.97],
                       [0, 0.66, -1.0]], [0.14, 0.18, 0.18, 0.13, 0.06], "ponytail", up=(1, 0, 0))
part("pony_ribbon", "hair_b", ell([0.085, 0.045, 0.03]), [0.08, 1.33, -0.14], "ribbon", "head",
     rot=[-30, 0, 25], blend=0.01, fuzz=0, mirror=True, style="ponytail")
part("pony_ribbon_end", "hair_b", f"Capsule(to: {f([0.03, -0.14, 0.02])}, r: 0.016)", [0.03, 1.3, -0.12],
     "ribbon", "head", blend=0.01, fuzz=0, mirror=True, style="ponytail")

comment("綁雙馬尾: two bunches high on the sides, each in an S, a ribbon on each.")
for side, name in ((1, "l"), (-1, "r")):
    k = [0.6 * side, 1.1, -0.14]
    part(f"twin_knot_{name}", f"hair_{name}", ell([0.15, 0.15, 0.14]), k, "hair", "head", blend=0.06,
         gloss=0.45, fuzz=0.05, style="twintails")
    tube(f"twin_{name}", f"hair_{name}",
         [[0.68 * side, 1.1, -0.15], [0.9 * side, 1.0, -0.18], [0.99 * side, 0.76, -0.2],
          [0.94 * side, 0.48, -0.18], [0.84 * side, 0.22, -0.14]],
         [0.12, 0.16, 0.17, 0.13, 0.06], "twintails", up=(0, 0, 1))
    part(f"twin_ribbon_{name}", f"hair_{name}", ell([0.08, 0.045, 0.03]), [0.6 * side, 1.25, -0.04],
         "ribbon", "head", rot=[-20, 0, 40 * side], blend=0.01, fuzz=0, style="twintails")
    part(f"twin_ribbon2_{name}", f"hair_{name}", ell([0.08, 0.045, 0.03]), [0.72 * side, 1.18, -0.04],
         "ribbon", "head", rot=[-20, 0, -30 * side], blend=0.01, fuzz=0, style="twintails")

comment("公主頭: long hair down her back to the waist, half tied up behind, a ribbon.")
for i, phi in enumerate((112, 140, 166)):
    a, n = around_cap(phi, 0, -0.03)
    b = [math.sin(math.radians(phi)) * 0.5, -0.28, math.cos(math.radians(phi)) * 0.42 - 0.12]
    strand(f"mane{i}", "hair_b", a, b, 0.2, 0.07, n, style="princess")
a, n = around_cap(180, 0, -0.03)
strand("mane_back", "hair_b", a, [0, -0.3, -0.55], 0.22, 0.08, n, style="princess", mirror=False)
part("mane_knot", "hair_b", ell([0.15, 0.1, 0.09]), [0, 0.86, -0.82], "hair", "head", blend=0.05,
     gloss=0.45, fuzz=0.05, style="princess")
part("mane_ribbon", "hair_b", ell([0.08, 0.045, 0.03]), [0.09, 0.9, -0.88], "ribbon", "head",
     rot=[0, 0, 20], blend=0.01, fuzz=0, mirror=True, style="princess")

comment("The nine tails grow from a round puff at the small of her back (gen_tails.py human).")
part("tail1", "tail1", ell([0.1, 0.1, 0.1]), [0, -0.3, -0.3], "fur", "tail", blend=0.1)
part("rootpuff", "hips", ell([0.2, 0.2, 0.17]), [0, -0.3, -0.36], "fur", "tail", blend=0.1)

HEADER = '''// 九尾狐's human form (人型形態). Written by gen_human.py (the shapes and
// where everything sits) and gen_tails.py (the tails): change them there.
//   python3 gen_human.py && python3 gen_tails.py human | python3 ../splice.py human.ron
//
// Her coat (the beast form's, by the same names) gives the tails, the
// marks, the eyes, the trim, the sash and the bell; the hair's colour is
// chosen on its own (`hairs`, white by default), and so is the hairdo
// (parts with a `style`: 綁單馬尾, 綁雙馬尾, 公主頭).
(
    species: "fox",
    bake_cell: 0.025,
    canvas: Some((4.8, 3.4)),
    // Sixteen at most: the shaders hold sixteen.
    slots: ["skin", "robe", "trim", "gold", "sash", "hair", "hairtip", "ribbon", "fur", "tailmid", "tailtip", "ink", "blush", "mark", "iris", "fluff"],
    swatch: Some(("fur", "mark")),
    coats: [
        // 紅髮版's colours: a red border with gold, a red sash and a red and
        // gold rope (`gold` is the braid and the bell), red marks, red eyes;
        // tails white through gold to orange.
        (name: "white", colors: {
            "skin": "#FFE6DC", "robe": "#FFFDF8", "trim": "#D2232F", "gold": "#F0BC42", "sash": "#D2232F",
            "hair": "#E9EDF5", "hairtip": "#CCD6E6", "ribbon": "#D2232F", "fluff": "#FFFFFF",
            "fur": "#FFFAF4", "tailmid": "#F9CF6A", "tailtip": "#F06A2E",
            "ink": "#3A2224", "blush": "#FF9AAA", "mark": "#D8202E", "iris": "#E02E30",
        }),
        // 銀白版's: a silver-grey border, a white rope with a silver bell,
        // ice-blue marks and eyes, tails tipped pale blue.
        (name: "snow", colors: {
            "skin": "#FFE6DC", "robe": "#FFFEFC", "trim": "#AEB9CB", "gold": "#C9D1DD", "sash": "#EEF1F6",
            "hair": "#E9EDF5", "hairtip": "#CCD6E6", "ribbon": "#D2232F", "fluff": "#FFFFFF",
            "fur": "#FBFCFF", "tailmid": "#DCEEFB", "tailtip": "#8CCBF2",
            "ink": "#22283A", "blush": "#FFB0C0", "mark": "#62B6EA", "iris": "#4A8FDB",
        }),
        (name: "gold", colors: {
            "skin": "#FFE6DC", "robe": "#FFFDF8", "trim": "#D2232F", "gold": "#F0BC42", "sash": "#D2232F",
            "hair": "#E9EDF5", "hairtip": "#CCD6E6", "ribbon": "#D2232F", "fluff": "#FFF8EE",
            "fur": "#F0A050", "tailmid": "#FBD9B0", "tailtip": "#FFFFFF",
            "ink": "#3A2224", "blush": "#FF8C94", "mark": "#E8454F", "iris": "#B8742A",
        }),
        (name: "night", colors: {
            "skin": "#FFE6DC", "robe": "#FBFAFF", "trim": "#5E6AB8", "gold": "#C8D0E0", "sash": "#4A4060",
            "hair": "#E9EDF5", "hairtip": "#CCD6E6", "ribbon": "#D2232F", "fluff": "#EDE7F5",
            "fur": "#4A4060", "tailmid": "#7A88C8", "tailtip": "#9FE3FF",
            "ink": "#231C2A", "blush": "#FF8CA8", "mark": "#7FD4FF", "iris": "#58B8F0",
        }),
    ],
    // The hair's colour, chosen apart from the coat (Roger: white by
    // default). 紅髮 is red fading to gold at the ends, as in his picture.
    hairs: [
        (name: "white", colors: {"hair": "#E9EDF5", "hairtip": "#CCD6E6"}),
        (name: "red", colors: {"hair": "#D8322A", "hairtip": "#F9B84E"}),
        (name: "gold", colors: {"hair": "#EFA04C", "hairtip": "#FFE3B5"}),
        (name: "night", colors: {"hair": "#4B4262", "hairtip": "#8A96D6"}),
    ],
    pivots: {
        "body": (0, -0.55, 0),
        "hips": (0, -0.45, 0),
        "head": (0, 0.05, 0),
        "arm_l": (0.25, -0.01, 0),
        "arm_r": (-0.25, -0.01, 0),
        "leg_l": (0.13, -0.68, 0),
        "leg_r": (-0.13, -0.68, 0),
        "bell": (0, -0.33, 0.4),
        "ear_l": (0.44, 1.08, -0.1),
        "ear_r": (-0.44, 1.08, -0.1),
        "hair_b": (0, 1.3, -0.3),
        "hair_l": (0.6, 1.1, -0.14),
        "hair_r": (-0.6, 1.1, -0.14),
        "tail1": (0, -0.3, -0.3),
    },
    // The legs hang from the whole pet, so a planted foot stays put as the
    // body sways; the rest from the body.
    parents: {
        "hips": "body",
        "head": "body",
        "arm_l": "body",
        "arm_r": "body",
        "bell": "body",
        "ear_l": "head",
        "ear_r": "head",
        "hair_b": "head",
        "hair_l": "head",
        "hair_r": "head",
        "tail1": "hips",
    },
    jiggles: [
        (name: "belly", parent: "body", at: (0, -0.3, 0.3), radius: 0.3, share: 0.5),
        (name: "chin", parent: "head", at: (0, 0.3, 0.45), radius: 0.34, share: 0.6),
    ],
    parts: [
'''

FOOTER = '''    ],
    gait: (swing: 24, lift: 0.08, roll: 4, twist: 6, nod: 3, diagonal: false, neck: 1.0),
    face: (
        on: "head",
        mouth_on: Some("mouth"),
        nose: (0, -0.18, 0.56),
        ink: "ink",
        // Big chibi eyes low on her face, her coat's colour in them.
        eye: (at: (0.34, -0.09), r: (0.205, 0.25), iris: Some(("iris", 0.55))),
        chibi: true,
        forehead: Some("mark"),
        seal: true,
        blush: (at: (0.5, -0.4), r: (0.2, 0.12), color: "blush"),
    ),
)
'''

with open(OUT, "w") as out:
    out.write(HEADER + "\n".join(parts) + "\n" + FOOTER)
