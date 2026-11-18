# The fox's human form as a whole figure made in Blender (Roger,
# 2026-11-17: 「明天用這份檔案使用 blender 進行開發看看」, after his sheet
# inbox/Gemini_Generated_Image_九尾人型(小)-3D示意與局部拆解圖.jpeg). Run
# headless:
#
#   blender -b --factory-startup -P figure_blend.py -- <out dir>
#
# It writes two files into <out dir>:
#
# - figure.mesh: everything but the head, as text for
#   `chonkgene_render::mesh::import` (bare legs, feet and hands, the robe
#   with its borders, the sleeves, the sash with its rope bow, bell and
#   tassels, the ears), each vertex with its bones and touch zone;
# - human_figure.ron: human.ron's coats, hair colours and face, with this
#   figure's skeleton and only the painted head (the face is painted on an
#   ellipsoid by our own shaders, so the head stays a baked part).
#
# Proportions are measured off the sheet's front and side views (her
# outline traced in pixels below, in the sheet's 正面 / 側面 at 2752 px
# wide, cropped at (28, 234)): crown of the hair at y 62, chin 300, sole
# 735, her middle at x 245 in front and 635 from the side. One pixel is K
# model units; the sole lands on FLOOR.
#
# Coordinates are the model's: x to her left, y up, z forward.
import math
import os
import sys

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree

K = 0.0034
FLOOR = -1.1
SOLE_PX = 735


def Y(py):
    return FLOOR + (SOLE_PX - py) * K


def X(px):
    """Front view: pixels right of her middle are to her left."""
    return (px - 245) * K


def Z(px):
    """Side view (she faces left): pixels left of her middle are forward."""
    return (635 - px) * K


def smoothstep(a, b, x):
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def lerp(a, b, t):
    return a + (b - a) * t


# ---- the skeleton -----------------------------------------------------------
HEAD_C = Vector((0.0, Y(212), -0.01))
HEAD_R = Vector((0.34, 0.33, 0.31))
NECK_Y = Y(318)
SHOULDER = Vector((X(305), Y(345), -0.01))  # her left shoulder joint
CUFF = Vector((-X(95), Y(468), 0.03))  # the middle of her left cuff
HIP = Vector((X(290), Y(560), 0.0))
ANKLE_Y = Y(705)
SASH_TOP, SASH_BOT = Y(437), Y(489)
HEM_Y = Y(615)

PIVOTS = {
    "body": Vector((0, Y(560), 0)),
    "hips": Vector((0, Y(540), 0)),
    "head": Vector((0, NECK_Y, 0)),
    "arm_l": SHOULDER,
    "arm_r": Vector((-SHOULDER.x, SHOULDER.y, SHOULDER.z)),
    "leg_l": HIP,
    "leg_r": Vector((-HIP.x, HIP.y, HIP.z)),
    "bell": Vector((0, Y(470), 0.2)),
    "ear_l": Vector((0.25, HEAD_C.y + 0.26, -0.06)),
    "ear_r": Vector((-0.25, HEAD_C.y + 0.26, -0.06)),
    "hair_b": Vector((0, HEAD_C.y + 0.3, -0.2)),
    "hair_l": Vector((0.28, HEAD_C.y + 0.1, 0.0)),
    "hair_r": Vector((-0.28, HEAD_C.y + 0.1, 0.0)),
}
PARENTS = {
    "hips": "body", "head": "body", "arm_l": "body", "arm_r": "body", "bell": "body",
    "ear_l": "head", "ear_r": "head", "hair_b": "head", "hair_l": "head", "hair_r": "head",
}

# ---- what is written ----------------------------------------------------------
# A vertex: position, colour slots a and b mixed t, shine, fuzz, bones,
# touch zone.
verts = []
faces = []


def vert(p, a, b="", t=0.0, gloss=0.15, fuzz=0.0, bones=(("body", 1.0),), zone="back"):
    verts.append((Vector(p), a, b or a, t, gloss, fuzz, tuple(bones), zone))
    return len(verts) - 1


def grid(rows, close_around, color, close_top=None, close_bottom=None):
    """Triangles over `rows` (lists of vertex indices, same length each);
    around each row closed if `close_around`. Optional caps: a vertex
    index fanned to the first or last row."""
    n = len(rows[0])
    m = n if close_around else n - 1
    for r in range(len(rows) - 1):
        a, b = rows[r], rows[r + 1]
        for j in range(m):
            j2 = (j + 1) % n
            faces.append((a[j], b[j], b[j2]))
            faces.append((a[j], b[j2], a[j2]))
    for cap, row, flip in ((close_top, rows[0], True), (close_bottom, rows[-1], False)):
        if cap is None:
            continue
        for j in range(m):
            j2 = (j + 1) % n
            faces.append((cap, row[j2], row[j]) if flip else (cap, row[j], row[j2]))


# ---- the skin: legs, feet, hands, neck (one metaball, so they are smooth) ----
def skin():
    mb = bpy.data.metaballs.new("skin")
    mb.resolution = 0.016
    mb.render_resolution = 0.016
    mb.threshold = 0.6
    obj = bpy.data.objects.new("skin", mb)
    bpy.context.scene.collection.objects.link(obj)

    # (A capsule's size_x is its half length in units, not scaled.)
    # Blender's metaball shows a lone ball at 0.575 of its radius (at
    # this threshold and stiffness, measured): sizes here are as seen.
    SEEN = 0.575

    def ball(at, r, kind="BALL", size=(1, 1, 1), rot=None):
        e = mb.elements.new()
        e.type = kind
        e.co = at
        e.radius = r / SEEN
        e.stiffness = 2.0
        if kind in ("ELLIPSOID", "CAPSULE"):
            e.size_x, e.size_y, e.size_z = size
        if rot is not None:
            e.rotation = rot

    from mathutils import Quaternion
    up = Quaternion((0, 0, 1), math.radians(90))  # capsule's x along y
    for s in (1, -1):
        # A chubby straight leg (thigh under the skirt, a round calf), a
        # little foot with a round heel and its toes.
        x = s * HIP.x
        top, bot = HIP.y - 0.02, ANKLE_Y + 0.05
        ball(Vector((x, (top + bot) / 2, 0.0)), 0.085, "CAPSULE",
             ((top - bot) / 2, 1, 1), up)
        ball(Vector((x, Y(655), 0.0)), 0.07, "ELLIPSOID", (1.0, 1.5, 1.0))
        ball(Vector((x * 1.02, FLOOR + 0.05, 0.035)), 0.075, "ELLIPSOID", (1.05, 0.68, 1.55))
        ball(Vector((x * 1.03, FLOOR + 0.04, 0.115)), 0.05, "ELLIPSOID", (1.35, 0.75, 0.9))
        # A hand out of the cuff, fingers together, the thumb forward,
        # pointing down and out along the arm.
        axis = (CUFF - SHOULDER).normalized()
        ax = Vector((s * axis.x, axis.y, axis.z))
        along = Quaternion((0, 0, 1), math.atan2(ax.y, ax.x))
        cuff = Vector((s * CUFF.x, CUFF.y, CUFF.z))
        ball(cuff + ax * 0.0, 0.04, "CAPSULE", (0.03, 1, 1), along)
        ball(cuff + ax * 0.08, 0.05, "ELLIPSOID", (1.25, 1.0, 0.62), along)
        ball(cuff + ax * 0.14, 0.04, "ELLIPSOID", (1.2, 1.05, 0.55), along)
        ball(cuff + ax * 0.075 + Vector((0, 0, 0.04)), 0.022)
    # The neck, into the head.
    ball(Vector((0, NECK_Y + 0.03, -0.01)), 0.095, "CAPSULE", (0.035, 1, 1), up)

    dg = bpy.context.evaluated_depsgraph_get()
    me = obj.evaluated_get(dg).to_mesh()
    idx = []
    for v in me.vertices:
        p = v.co.copy()
        side = "l" if p.x > 0 else "r"
        if p.y > NECK_Y - 0.12:
            w = smoothstep(NECK_Y - 0.06, NECK_Y + 0.06, p.y)
            bones, zone = (("head", w), ("body", 1 - w)), "chin"
        elif abs(p.x) > 0.3:
            bones, zone = ((f"arm_{side}", 1.0),), "paw"
        else:
            w = smoothstep(HIP.y - 0.12, HIP.y + 0.02, p.y)
            bones, zone = ((f"leg_{side}", 1 - w), ("hips", w)), "paw"
        idx.append(vert(p, "skin", gloss=0.12, bones=bones, zone=zone))
    for poly in me.polygons:
        vs = [idx[i] for i in poly.vertices]
        for k in range(1, len(vs) - 1):
            faces.append((vs[0], vs[k], vs[k + 1]))


# ---- the robe -------------------------------------------------------------------
# Her outline, (y px, half width px in front, forward px and back px from
# the side), neck to hem.
ROBE = [
    (316, 30, 26, 26),
    (326, 48, 36, 34),
    (338, 62, 42, 40),
    (360, 66, 46, 44),
    (400, 67, 48, 45),
    (440, 69, 50, 46),
    (485, 72, 50, 48),
    (520, 84, 58, 58),
    (560, 100, 72, 76),
    (595, 113, 84, 96),
    (614, 119, 88, 104),
]
AROUND = 168


def robe_at(py):
    """Half width, forward and back reach (model units) at sheet height py."""
    for (y0, *a), (y1, *b) in zip(ROBE, ROBE[1:]):
        if y0 <= py <= y1:
            t = (py - y0) / (y1 - y0)
            t = t * t * (3 - 2 * t) * 0.5 + t * 0.5
            return [lerp(u, v, t) * K for u, v in zip(a, b)]
    return [c * K for c in (ROBE[0][1:] if py < ROBE[0][0] else ROBE[-1][1:])]


def section(py, th, out=0.0):
    """On the robe at height py, turned th (0 facing you, + to her left)."""
    a, f, b = robe_at(py)
    c, s = math.cos(th), math.sin(th)
    # A soft box: fuller at the sides than an ellipse.
    e = 2.5
    sx = math.copysign(abs(s) ** (2 / e), s)
    cz = math.copysign(abs(c) ** (2 / e), c)
    depth = f if c > 0 else b
    p = Vector(((a + out) * sx, Y(py), (depth + out) * cz))
    return p


def front_point(x, y):
    """The point on the robe's front at x, model height y."""
    py = SOLE_PX - (y - FLOOR) / K
    a, f, _ = robe_at(py)
    th = math.asin(max(-1.0, min(1.0, x / max(a, 1e-3)))) if a else 0.0
    # invert the soft box roughly: search th
    best, bt = 1e9, 0.0
    for i in range(-60, 61):
        t = i / 60 * 1.4
        d = abs(section(py, t).x - x)
        if d < best:
            best, bt = d, t
    return section(py, bt)


# The crossed collar's edge (her right panel over her left, as in the
# sheet: from beside her neck on her left, across to her right at the
# sash), and below the sash the overlap's edge down her right to the hem.
OUTER_LAPEL = [(0.075, Y(322)), (0.02, Y(360)), (-0.05, Y(400)), (-0.12, Y(445))]
SKIRT_EDGE = [(-0.15, Y(485)), (-0.16, Y(540)), (-0.175, Y(600))]
INNER_LAPEL = [(-0.075, Y(322)), (-0.03, Y(350)), (0.0, Y(368))]
BAND = 0.062
HEM_BAND = (Y(612), Y(574))


def seg_dist(p, a, b):
    ab = b - a
    t = max(0.0, min(1.0, (p - a).dot(ab) / max(ab.dot(ab), 1e-9)))
    return (p - (a + ab * t)).length


def poly_dist(p, line):
    pts = [front_point(x, y) for x, y in line]
    return min(seg_dist(p, a, b) for a, b in zip(pts, pts[1:]))


LAPELS = None


def trim_amount(p, front):
    """How much of the border the robe's surface at p is in (0 to 1)."""
    global LAPELS
    if LAPELS is None:
        LAPELS = [[front_point(x, y) for x, y in line]
                  for line in (OUTER_LAPEL, SKIRT_EDGE, INNER_LAPEL)]
    t = 0.0
    if front:
        for pts in LAPELS:
            d = min(seg_dist(p, a, b) for a, b in zip(pts, pts[1:]))
            t = max(t, 1 - smoothstep(BAND * 0.5 - 0.006, BAND * 0.5 + 0.006, d))
    # The collar's band round the back of the neck.
    t = max(t, smoothstep(Y(334), Y(328), p.y))
    # The hem's border.
    t = max(t, smoothstep(HEM_BAND[1] - 0.008, HEM_BAND[1] + 0.006, -p.y + 2 * HEM_BAND[1])
            if False else (1 - smoothstep(HEM_BAND[1] - 0.006, HEM_BAND[1] + 0.006, p.y)))
    return t


def robe_bones(p):
    w = smoothstep(SASH_BOT, SASH_BOT - 0.2, p.y)
    bones = [("body", 1 - w), ("hips", w)] if w > 0 else [("body", 1.0)]
    side = "l" if p.x > 0 else "r"
    arm = 0.6 * smoothstep(0.17, 0.25, abs(p.x)) * smoothstep(Y(380), Y(340), p.y)
    if arm > 0:
        bones = [(n, w * (1 - arm)) for n, w in bones] + [(f"arm_{side}", arm)]
    return bones


def robe():
    rows = []
    py_rows = []
    y = ROBE[0][0]
    while y < ROBE[-1][0]:
        py_rows.append(y)
        y += 2.0 if y > 560 else 3.0
    py_rows.append(ROBE[-1][0])
    for py in py_rows:
        row = []
        for j in range(AROUND):
            th = 2 * math.pi * j / AROUND
            p = section(py, th)
            front = math.cos(th) > 0.2
            t = trim_amount(p, front)
            # The border stands a little proud of the cloth.
            p = section(py, th, out=0.008 * t)
            row.append(vert(p, "robe", "trim", t, gloss=0.18, fuzz=0.05, bones=robe_bones(p)))
        rows.append(row)
    # Closed round the neck; the hem turns in, the cloth's thickness.
    top = vert(Vector((0, Y(ROBE[0][0]) + 0.005, -0.01)), "robe", "trim", 1.0,
               bones=[("body", 1.0)])
    hem = []
    for j in range(AROUND):
        th = 2 * math.pi * j / AROUND
        p = section(ROBE[-1][0] + 1.5, th, out=-0.012)
        hem.append(vert(p, "trim", bones=robe_bones(p)))
    inner = []
    for j in range(AROUND):
        th = 2 * math.pi * j / AROUND
        p = section(ROBE[-1][0] - 8, th, out=-0.03)
        inner.append(vert(p, "robe", bones=robe_bones(p)))
    grid(rows + [hem, inner], True, "robe", close_top=top)


# ---- the sleeves: wide trumpet sleeves, a border at the cuff ------------------
def sleeve(s):
    sh = Vector((s * SHOULDER.x * 0.75, SHOULDER.y + 0.01, SHOULDER.z))
    cf = Vector((s * CUFF.x, CUFF.y, CUFF.z))
    axis = (cf - sh).normalized()
    side = Vector((0, 0, 1)).cross(axis).normalized()  # in the front plane
    fwd = axis.cross(side).normalized()
    length = (cf - sh).length
    rows = []
    n_al, n_ar = 40, 64
    bone = f"arm_{'l' if s > 0 else 'r'}"
    for i in range(n_al + 1):
        t = i / n_al
        r = lerp(0.085, 0.175, t ** 1.6)
        # A sleeve hangs: fuller underneath towards the cuff.
        sag = 0.035 * t * t
        c = sh + axis * (length * t)
        row = []
        for j in range(n_ar):
            a = 2 * math.pi * j / n_ar
            u = math.cos(a)  # + towards the top of the arm (side x s)
            v = math.sin(a)
            hang = sag * max(0.0, -u)
            d = side * (u * r * (1.0 + 0.25 * max(0.0, -u))) * (1 if s > 0 else -1) + fwd * (v * r * 0.92)
            p = c + d + Vector((0, -hang, 0))
            trim = smoothstep(0.80, 0.82, t)
            p += d.normalized() * 0.007 * trim
            w = smoothstep(0.0, 0.3, t)
            row.append(vert(p, "robe", "trim", trim, gloss=0.18, fuzz=0.05,
                            bones=((bone, w), ("body", 1 - w)) if w < 1 else ((bone, 1.0),)))
        rows.append(row)
    # The cuff's edge turned in, so it has a thickness.
    for k, (dl, dr) in enumerate(((0.008, -0.012), (-0.03, -0.022))):
        row = []
        t = 1.0
        c = sh + axis * (length + dl)
        r = 0.175 + dr
        for j in range(n_ar):
            a = 2 * math.pi * j / n_ar
            u, v = math.cos(a), math.sin(a)
            hang = 0.035 * max(0.0, -u)
            d = side * (u * r * (1.0 + 0.25 * max(0.0, -u))) * (1 if s > 0 else -1) + fwd * (v * r * 0.92)
            row.append(vert(c + d + Vector((0, -hang, 0)), "trim" if k == 0 else "robe",
                            bones=((bone, 1.0),)))
        rows.append(row)
    if s < 0:
        rows = [list(reversed(r)) for r in rows]
    grid(rows, True, "robe")


# ---- tubes: the sash, the rope, the bow, cords --------------------------------
def tube(path, radius, color, bones, zone="back", closed=False, sides=12, gloss=0.25,
         twist=0.0, b_color=None, caps=True):
    """A round tube along `path` (Vectors), radius(t) or a number."""
    rf = radius if callable(radius) else (lambda t, r=radius: r)
    n = len(path)
    rows = []
    prev_side = None
    for i, p in enumerate(path):
        if closed:
            d = path[(i + 1) % n] - path[i - 1]
        else:
            d = path[min(i + 1, n - 1)] - path[max(i - 1, 0)]
        d.normalize()
        ref = Vector((0, 1, 0)) if abs(d.y) < 0.9 else Vector((1, 0, 0))
        side = d.cross(ref).normalized() if prev_side is None else (prev_side - d * prev_side.dot(d)).normalized()
        prev_side = side
        up = side.cross(d).normalized()
        t = i / (n - (0 if closed else 1))
        r = rf(t)
        row = []
        for j in range(sides):
            a = 2 * math.pi * j / sides + twist * t
            q = p + (side * math.cos(a) + up * math.sin(a)) * r
            row.append(vert(q, color, b_color or color, 0.0, gloss=gloss,
                            bones=bones(q) if callable(bones) else bones, zone=zone))
        rows.append(row)
    if closed:
        rows.append(rows[0])
        grid(rows, True, color)
    else:
        a = vert(path[0] - (path[1] - path[0]).normalized() * rf(0) * 0.5, color,
                 bones=bones(path[0]) if callable(bones) else bones, zone=zone)
        b = vert(path[-1] + (path[-1] - path[-2]).normalized() * rf(1) * 0.5, color,
                 bones=bones(path[-1]) if callable(bones) else bones, zone=zone)
        grid(rows, True, color, close_top=a if caps else None, close_bottom=b if caps else None)


def rope(path, r, color, bones, closed=False):
    """Three strands twisted round each other: a braided cord."""
    n = len(path)
    for k in range(3):
        pts = []
        prev = None
        for i, p in enumerate(path):
            d = (path[(i + 1) % n] - path[i - 1]) if closed else \
                (path[min(i + 1, n - 1)] - path[max(i - 1, 0)])
            d.normalize()
            ref = Vector((0, 1, 0)) if abs(d.y) < 0.9 else Vector((1, 0, 0))
            side = d.cross(ref).normalized() if prev is None else (prev - d * prev.dot(d)).normalized()
            prev = side
            up = side.cross(d).normalized()
            arc = sum((path[m] - path[m - 1]).length for m in range(1, i + 1))
            a = arc / (r * 3.2) * 2 * math.pi + k * 2 * math.pi / 3
            pts.append(p + (side * math.cos(a) + up * math.sin(a)) * r * 0.55)
        tube(pts, r * 0.62, color, bones, closed=closed, sides=8, gloss=0.35)


def ring_path(y, out, n=96):
    py = SOLE_PX - (y - FLOOR) / K
    return [section(py, 2 * math.pi * j / n, out=out) for j in range(n)]


def sash():
    # The wide sash: a band round her waist over the robe.
    rows = []
    for i in range(9):
        y = lerp(SASH_TOP, SASH_BOT, i / 8)
        edge = min(i, 8 - i)
        out = 0.012 + (0.004 if edge > 0 else 0.0)
        rows.append([vert(p, "sash", gloss=0.2, fuzz=0.04) for p in ring_path(y, out)])
    grid(rows, True, "sash")
    # The rope round it, tied at the front in a bow of two rope loops,
    # a bell hung under the knot, two tassels.
    yr = lerp(SASH_TOP, SASH_BOT, 0.42)
    rope(ring_path(yr, 0.034), 0.02, "gold", (("body", 1.0),), closed=True)
    front = ring_path(yr, 0.034)[0]
    knot = front + Vector((0, 0, 0.03))
    for s in (1, -1):
        loop = []
        for j in range(33):
            a = 2 * math.pi * j / 32
            # A loop out to the side and back: a teardrop round from the knot.
            x = 0.12 * (1 - math.cos(a))
            yy = 0.07 * math.sin(a) * (0.55 + 0.45 * (1 - math.cos(a)) / 2)
            # Round the body's front as it goes out to the side.
            loop.append(knot + Vector((s * x, yy, -0.06 * ((1 - math.cos(a)) / 2) ** 1.5)))
        rope(loop, 0.02, "gold", (("body", 1.0),), closed=True)
    tube([knot + Vector((0, 0.035, 0)), knot + Vector((0, -0.035, 0.006))],
         lambda t: 0.036, "gold", (("body", 1.0),), sides=16, gloss=0.35)
    # Tassels: a cord down from the knot, a little cap, a skirt of threads.
    for s in (1, -1):
        a = knot + Vector((s * 0.03, -0.03, 0.0))
        b = a + Vector((s * 0.05, -0.13, -0.005))
        rope([a.lerp(b, i / 8) for i in range(9)], 0.011, "gold", (("body", 1.0),))
        tube([b, b + Vector((0, -0.025, 0))], 0.018, "gold", (("body", 1.0),), sides=10)
        tube([b + Vector((0, -0.02, 0)), b + Vector((s * 0.004, -0.1, 0))],
             lambda t: lerp(0.019, 0.032, t ** 0.7), "gold", (("body", 1.0),), sides=12)
    # The bell: a ball with a slot, hung under the knot.
    bc = knot + Vector((0, -0.105, 0.03))
    sphere(bc, Vector((0.06, 0.056, 0.06)), "gold", (("bell", 1.0),), gloss=0.7,
           slot=True)


def sphere(c, r, color, bones, gloss=0.3, slot=False, zone="back", n=20):
    rows = []
    for i in range(1, n):
        th = math.pi * i / n
        row = []
        for j in range(2 * n):
            ph = 2 * math.pi * j / (2 * n)
            u = Vector((math.sin(th) * math.sin(ph), math.cos(th), math.sin(th) * math.cos(ph)))
            # The slot across the bell's lower half: a dark band facing out.
            dark = 0.0
            if slot and u.y < -0.1 and abs(u.x) < 0.1:
                dark = 1 - smoothstep(0.06, 0.1, abs(u.x))
            p = c + Vector((r.x * u.x, r.y * u.y, r.z * u.z)) * (1 - 0.15 * dark)
            row.append(vert(p, color, "ink", 0.85 * dark, gloss=gloss, bones=bones, zone=zone))
        rows.append(row)
    top = vert(c + Vector((0, r.y, 0)), color, bones=bones, zone=zone)
    bot = vert(c - Vector((0, r.y, 0)), color, bones=bones, zone=zone)
    grid([list(reversed(r_)) for r_ in rows], True, color, close_top=top, close_bottom=bot)


# ---- ears: tall fox ears tilted out, white fluff inside -----------------------
def ear(s):
    base = Vector((s * 0.22, HEAD_C.y + 0.24, -0.07))
    tip = Vector((s * 0.39, HEAD_C.y + 0.6, -0.08))
    bone = f"ear_{'l' if s > 0 else 'r'}"
    axis = (tip - base).normalized()
    across = Vector((1, 0, 0)) - axis * axis.x
    across.normalize()
    fwd = axis.cross(across).normalized()
    if fwd.z < 0:
        fwd = -fwd
    length = (tip - base).length
    for layer, (color, scale, shift) in enumerate((("hair", 1.0, 0.0), ("fluff", 0.6, 0.03))):
        rows = []
        for i in range(25):
            t = i / 24
            w = 0.2 * scale * (1 - t) ** 0.75 * (1 + 0.3 * math.sin(math.pi * t))
            d = 0.06 * scale * (1 - t) ** 0.7 + 0.004
            c = base + axis * (length * (t * (0.96 if layer else 1.0) + (0.05 if layer else 0)))
            c += fwd * (shift * (1 - t))
            row = []
            for j in range(24):
                a = 2 * math.pi * j / 24
                ca, sa = math.cos(a), math.sin(a)
                # Lens-shaped: hollow at the front for the outer ear.
                hollow = -1.1 * d * max(0.0, sa) * (1 - abs(ca)) ** 0.7 if layer == 0 else 0.0
                p = c + across * (w * ca) + fwd * (d * sa + hollow)
                row.append(vert(p, color, gloss=0.3, fuzz=0.2, bones=((bone, 1.0),), zone="head"))
            rows.append(row)
        top = vert(tip + fwd * 0.003 * layer, color, bones=((bone, 1.0),), zone="head")
        grid(rows if s > 0 else [list(reversed(r)) for r in rows], True, color, close_bottom=top)


# ---- normals, occlusion, out ---------------------------------------------------
def piece(build, *args):
    """Build a piece, then turn its triangles outward if they face in (by
    its signed volume: each piece is near enough closed)."""
    first = len(faces)
    build(*args)
    vol = 0.0
    for a, b, c in faces[first:]:
        vol += verts[a][0].dot(verts[b][0].cross(verts[c][0]))
    if vol < 0:
        faces[first:] = [(a, c, b) for a, b, c in faces[first:]]
    print(f"{build.__name__}{args}: {len(faces) - first} triangles"
          f"{' (turned)' if vol < 0 else ''}")


def normals():
    acc = [Vector() for _ in verts]
    for a, b, c in faces:
        pa, pb, pc = verts[a][0], verts[b][0], verts[c][0]
        n = (pb - pa).cross(pc - pa)
        for i in (a, b, c):
            acc[i] += n
    return [n.normalized() if n.length > 1e-12 else Vector((0, 1, 0)) for n in acc]


def head_proxy():
    pts, polys = [], []
    n = 24
    for i in range(n + 1):
        th = math.pi * i / n
        for j in range(n):
            ph = 2 * math.pi * j / n
            u = Vector((math.sin(th) * math.cos(ph), math.cos(th), math.sin(th) * math.sin(ph)))
            pts.append(HEAD_C + Vector((HEAD_R.x * u.x, HEAD_R.y * u.y, HEAD_R.z * u.z)))
    for i in range(n):
        for j in range(n):
            a = i * n + j
            b = i * n + (j + 1) % n
            polys.append((a, b, b + n, a + n))
    return pts, polys


AO_REACH, AO_STRENGTH = 0.12, 0.75


def occlusion(nrm):
    hp, hf = head_proxy()
    pts = [v[0] for v in verts] + hp
    polys = [tuple(f) for f in faces] + [tuple(i + len(verts) for i in f) for f in hf]
    tree = BVHTree.FromPolygons(pts, polys, all_triangles=False)
    dirs = []
    golden = math.pi * (3 - math.sqrt(5))
    for i in range(24):
        y = 1 - (i + 0.5) / 24 * 2
        r = math.sqrt(1 - y * y)
        dirs.append(Vector((math.cos(golden * i) * r, y, math.sin(golden * i) * r)))
    out = []
    for v, n in zip(verts, nrm):
        hit, total = 0.0, 0.0
        for d in dirs:
            c = d.dot(n)
            if c <= 0.05:
                continue
            total += c
            loc, _, _, dist = tree.ray_cast(v[0] + n * 0.003, d, AO_REACH)
            if loc is not None:
                hit += c * (1.0 - dist / AO_REACH)
        out.append(max(0.0, 1.0 - AO_STRENGTH * hit / max(total, 1e-6)))
    return out


def write_mesh(path):
    nrm = normals()
    ao = occlusion(nrm)
    zone = None
    with open(path, "w") as f:
        for (p, a, b, t, gloss, fuzz, bones, z), n, o in zip(verts, nrm, ao):
            if z != zone:
                f.write(f"z {z}\n")
                zone = z
            total = sum(w for _, w in bones) or 1.0
            bs = " ".join(f"{name} {w / total:.3f}" for name, w in bones if w > 1e-4)
            f.write(f"v {p.x:.4f} {p.y:.4f} {p.z:.4f} {n.x:.3f} {n.y:.3f} {n.z:.3f} "
                    f"{a} {b} {t:.3f} {gloss} {fuzz} {o:.3f} {bs}\n")
        for a, b, c in faces:
            f.write(f"f {a} {b} {c}\n")
    print(f"{path}: {len(verts)} vertices, {len(faces)} triangles")


def fmt(c):
    return "0" if abs(c) < 5e-4 else f"{c:.3f}".rstrip("0").rstrip(".")


def v3(v):
    return "(" + ", ".join(fmt(c) for c in v) + ")"


def write_ron(path):
    """human.ron's coats, hair colours and face, with this figure's
    skeleton and only the painted head."""
    here = os.path.dirname(os.path.abspath(__file__))
    src = open(os.path.join(here, "human.ron"), encoding="utf-8").read()
    head_part = src[:src.index("    pivots: {")]
    tail = src[src.index("    gait: ("):]
    hc, hr = HEAD_C, HEAD_R
    nose = Vector((0, hc.y - 0.1, hc.z + 0.3))
    mouth = Vector((0, hc.y - 0.18, hc.z + 0.262))
    cheek = Vector((0.17, hc.y - 0.17, hc.z + 0.12))
    # The face: eyes wide apart and low, as on the sheet (measured: the
    # eyes' middles 46 px either side, 38 px under the head's middle).
    tail = tail.replace("nose: (0, -0.18, 0.56)", f"nose: {v3(nose - hc)}")
    tail = tail.replace("eye: (at: (0.34, -0.09), r: (0.205, 0.25)",
                        "eye: (at: (0.42, -0.13), r: (0.2, 0.23)")
    tail = tail.replace("blush: (at: (0.5, -0.4), r: (0.2, 0.12)",
                        "blush: (at: (0.58, -0.45), r: (0.15, 0.09)")
    head_part = head_part.replace("bake_cell: 0.025", "bake_cell: 0.012")
    out = [head_part, "    pivots: {\n"]
    out += [f'        "{k}": {v3(v)},\n' for k, v in PIVOTS.items()]
    out.append("    },\n    parents: {\n")
    out += [f'        "{k}": "{v}",\n' for k, v in PARENTS.items()]
    out.append("    },\n    jiggles: [\n")
    out.append(f'        (name: "chin", parent: "head", at: {v3(Vector((0, hc.y - 0.2, 0.2)))}, '
               f'radius: 0.17, share: 0.6),\n')
    out.append("    ],\n    parts: [\n")
    out.append(f'        (name: "head", bone: "head", shape: Ellipsoid({v3(hr)}), at: {v3(hc)},\n'
               f'            blend: 0.03, color: "skin", fuzz: 0, zone: "head"),\n')
    out.append(f'        (name: "nose", bone: "head", shape: Ellipsoid((0.014, 0.01, 0.007)), at: {v3(nose)},\n'
               f'            blend: 0.015, color: "skin", fuzz: 0, zone: "head"),\n')
    out.append(f'        (name: "mouth", bone: "head", shape: Ellipsoid((0.045, 0.02, 0.01)), at: {v3(mouth)},\n'
               f'            blend: 0.025, color: "skin", fuzz: 0, zone: "chin"),\n')
    out.append(f'        (name: "cheek", bone: "head", shape: Ellipsoid((0.14, 0.12, 0.13)), at: {v3(cheek)},\n'
               f'            blend: 0.07, color: "skin", fuzz: 0, mirror: true, zone: "cheek"),\n')
    out.append("    ],\n")
    with open(path, "w", encoding="utf-8") as f:
        f.write("".join(out) + tail)
    print(f"{path}: written")


if __name__ == "__main__":
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = args[0] if args else "."
    os.makedirs(out, exist_ok=True)
    piece(skin)
    piece(robe)
    piece(sleeve, 1)
    piece(sleeve, -1)
    piece(sash)
    piece(ear, 1)
    piece(ear, -1)
    write_mesh(os.path.join(out, "figure.mesh"))
    write_ron(os.path.join(out, "human_figure.ron"))
