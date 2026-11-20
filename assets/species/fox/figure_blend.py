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
# - armbands.mesh: 紅髮版's rope rings round the upper sleeves, for the
#   coats that wear them (the 銀白版 has none);
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
HEAD_C = Vector((0.0, Y(212), -0.02))
HEAD_R = Vector((0.34, 0.33, 0.28))
# Her lower face, a part of its own on the painted head: the sheet's side
# view has the cheeks, mouth and chin well forward of the brow (a baby's
# face, fullest low down), the chin's underside at y 0.35.
JAW_C = Vector((0.0, 0.46, 0.10))
JAW_R = Vector((0.24, 0.13, 0.178))
NECK_Y = Y(318)
SHOULDER = Vector((X(305), Y(345), -0.04))  # her left shoulder joint
CUFF = Vector((-X(95), Y(468), -0.095))  # the middle of her left cuff
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


def face_out(first):
    """Turn the triangles from `first` on outward if they face in (by their
    signed volume: a closed part's does not depend on where it is), and
    mark them done, so the piece they are in leaves them be."""
    vol = sum(verts[a][0].dot(verts[b][0].cross(verts[c][0])) for a, b, c in faces[first:])
    if vol < 0:
        faces[first:] = [(a, c, b) for a, b, c in faces[first:]]
    ORIENTED.update(range(first, len(faces)))


ORIENTED = set()


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


# ---- the skin: legs, feet, hands, neck --------------------------------------
# Metaballs, so each part is one smooth surface. Each part is a family of
# its own (Blender joins metaballs only by name), at its own resolution:
# the fingers and toes need a fine one, the legs do not.
#
# A metaball's field is stiffness × (1 − d²/R²)³, shown where it reaches
# the threshold, so a lone ball shows at R·√(1 − (threshold/stiffness)^⅓)
# (0.575 R at stiffness 2: measured the same). A stiff ball's field falls
# away fast outside what shows: stiff fingers and toes side by side keep
# the grooves between them (at stiffness 8 two touching ones bridge only
# 6 % past their own size; at 2, 19 %).
THRESHOLD = 0.6


def seen(stiff):
    return math.sqrt(1 - (THRESHOLD / stiff) ** (1 / 3))


class Balls:
    def __init__(self, name, res):
        self.mb = bpy.data.metaballs.new(name)
        self.mb.resolution = res
        self.mb.render_resolution = res
        self.mb.threshold = THRESHOLD
        self.obj = bpy.data.objects.new(name, self.mb)
        bpy.context.scene.collection.objects.link(self.obj)

    def ball(self, at, r, kind="BALL", size=(1, 1, 1), rot=None, stiff=2.0):
        """A ball showing `r` round (a capsule's size_x is its half length
        in units, unscaled; an ellipsoid's sizes scale `r`; a cube shows
        as a box `size` from its middle each way plus `r`, rounded `r`)."""
        e = self.mb.elements.new()
        e.type = kind
        e.co = at
        e.radius = r / seen(stiff)
        e.stiffness = stiff
        if kind in ("ELLIPSOID", "CAPSULE", "CUBE"):
            e.size_x, e.size_y, e.size_z = size
        if rot is not None:
            e.rotation = rot

    def capsule(self, a, b, r, stiff=2.0):
        """A capsule from a to b, `r` round."""
        d = b - a
        self.ball((a + b) / 2, r, "CAPSULE", (d.length / 2, 1, 1),
                  Vector((1, 0, 0)).rotation_difference(d.normalized()), stiff)

    def oriented(self, at, r, size, frame, stiff=2.0):
        """An ellipsoid with its axes along `frame` (three vectors)."""
        from mathutils import Matrix
        m = Matrix((frame[0], frame[1], frame[2])).transposed()
        self.ball(at, r, "ELLIPSOID", size, m.to_quaternion(), stiff)

    def mesh(self, bones, zone, gloss=0.12, keep=None):
        """Into the figure; `keep` thins it to that share of its triangles
        (Blender's Decimate: the fine resolution the toes and fingers need
        would be wasted on the rest)."""
        dg = bpy.context.evaluated_depsgraph_get()
        me = self.obj.evaluated_get(dg).to_mesh()
        if keep is not None:
            solid = bpy.data.meshes.new_from_object(self.obj.evaluated_get(dg))
            thin = bpy.data.objects.new(self.obj.name + "_thin", solid)
            bpy.context.scene.collection.objects.link(thin)
            mod = thin.modifiers.new("thin", "DECIMATE")
            mod.ratio = keep
            dg = bpy.context.evaluated_depsgraph_get()
            me = thin.evaluated_get(dg).to_mesh()
        idx = [vert(v.co.copy(), "skin", gloss=gloss, bones=bones(v.co), zone=zone) for v in me.vertices]
        for poly in me.polygons:
            vs = [idx[i] for i in poly.vertices]
            for k in range(1, len(vs) - 1):
                faces.append((vs[0], vs[k], vs[k + 1]))


def legs():
    """The legs and the feet as one surface: a close chain of balls down
    each leg (a full thigh, a round calf, an ankle hardly narrower), plump
    feet as wide as the legs (the sheet's front view), and along the front
    of each a row of five toes flush with it, the big toe on the inside,
    the grooves between them showing (stiff toes keep them); the toes reach
    a little past the shin (its side view)."""
    b = Balls("legs", 0.0045)
    for s in (1, -1):
        x = s * HIP.x
        top, ankle = HIP.y - 0.02, FLOOR + 0.075
        steps = 22
        for i in range(steps + 1):
            t = i / steps
            r = 0.088 + 0.006 * math.sin(math.pi * min(1.0, t / 0.75))
            # Front to back (the sheet's side view): as deep as wide at the
            # top, the calf bulging behind, the ankle slimmer; the leg's
            # middle 0.04 to 0.06 behind hers.
            deep = 1.0 - 0.22 * smoothstep(0.35, 1.0, t)
            z = -0.035 - 0.025 * smoothstep(0.0, 0.5, t)
            b.ball(Vector((x, lerp(top, ankle, t), z)), r, "ELLIPSOID", (1.0, 1.0, deep))
        # The foot is hardly more than the leg's end (the sheet's side view:
        # the toes a little past the shin, no heel behind the calf): a flat
        # sole as wide as the leg, and along its front, low, a row of five
        # toes taller than they are wide, the big toe on the inside, the
        # grooves between them showing.
        fx = x * 1.02
        # A rounded box, so the front is flat across like the sheet's and
        # all five toes show along it.
        b.ball(Vector((fx, FLOOR + 0.04, -0.022)), 0.03, "CUBE", (0.058, 0.008, 0.095))
        for dx, r, sx in ((-0.066, 0.027, 0.72), (-0.032, 0.025, 0.7), (0.0, 0.0245, 0.7),
                          (0.031, 0.0235, 0.7), (0.061, 0.022, 0.72)):
            b.ball(Vector((fx + s * dx, FLOOR + 0.025, 0.104 - 0.15 * abs(dx + 0.01) ** 1.5)),
                   r, "ELLIPSOID", (sx, 1.0, 0.82), stiff=6.0)

    def bones(p):
        side = "l" if p.x > 0 else "r"
        w = smoothstep(HIP.y - 0.12, HIP.y + 0.02, p.y)
        return ((f"leg_{side}", 1 - w), ("hips", w))
    b.mesh(bones, "paw", keep=0.3)


def hands():
    """Big chubby hands out of the cuffs, as the sheet draws them: from
    the side the back of the hand, four fingers side by side front to back
    with the thumb in front; from the front its edge. They point out and
    down at 45°, the palm turned to the body."""
    b = Balls("hands", 0.0045)
    axis = (CUFF - SHOULDER).normalized()
    for s in (1, -1):
        ax = Vector((s * axis.x, axis.y, axis.z))
        cuff = Vector((s * CUFF.x, CUFF.y, CUFF.z))
        hd = Vector((s * math.cos(math.radians(45)), -math.sin(math.radians(45)), 0.0))
        z = Vector((0, 0, 1))
        w = z
        n = hd.cross(w)
        if n.x * s > 0:  # the palm faces the body
            w = -z
            n = hd.cross(w)
        # The wrist at the cuff's opening (its underside reaches further),
        # the whole hand out of it, 0.23 to the fingertips.
        wrist = cuff + ax * 0.07
        b.capsule(cuff - ax * 0.05, wrist, 0.045)
        b.oriented(wrist + hd * 0.085 + n * 0.004, 0.075, (1.1, 1.15, 0.5), (hd, w, n))
        for k, (off, ln) in enumerate(((-0.069, 0.04), (-0.023, 0.05), (0.023, 0.052), (0.069, 0.042))):
            base = wrist + hd * 0.14 + z * off
            tip = base + (hd + n * 0.18).normalized() * ln
            b.capsule(base, tip, 0.023, stiff=8.0)
        tb = wrist + hd * 0.07 + z * 0.088 + n * 0.03
        b.capsule(tb, tb + (hd * 0.6 + z * 0.5 + n * 0.4).normalized() * 0.055, 0.024, stiff=6.0)
    b.mesh(lambda p: ((f"arm_{'l' if p.x > 0 else 'r'}", 1.0),), "paw", keep=0.4)


def neck():
    b = Balls("neck", 0.0085)
    b.capsule(Vector((0, NECK_Y - 0.005, -0.03)), Vector((0, NECK_Y + 0.065, -0.03)), 0.075)

    def bones(p):
        w = smoothstep(NECK_Y - 0.06, NECK_Y + 0.06, p.y)
        return (("head", w), ("body", 1 - w))
    b.mesh(bones, "chin")


# ---- the robe -------------------------------------------------------------------
# Her outline, (y px, half width px in front, forward px and back px from
# the side), neck to hem.
ROBE = [
    (316, 30, 16, 26),
    (326, 48, 22, 34),
    (338, 62, 28, 40),
    (360, 66, 41, 44),
    (400, 67, 60, 45),
    (440, 69, 62, 46),
    (485, 72, 61, 52),
    (520, 84, 70, 79),
    (560, 100, 80, 88),
    (595, 113, 86, 98),
    (614, 119, 89, 103),
    # (the back hangs lower than the front: past the hem's front)
    (660, 133, 99, 120),
]
AROUND = 168
FOLDS = 9
HEM_FRONT, HEM_BACK = 614, 625


def hem(th):
    """Where the hem is at turn th: the back hangs longer than the front
    (the sheet's side view)."""
    return HEM_FRONT + (HEM_BACK - HEM_FRONT) * (1 - math.cos(th)) / 2


def robe_at(py):
    """Half width, forward and back reach (model units) at sheet height py."""
    for (y0, *a), (y1, *b) in zip(ROBE, ROBE[1:]):
        if y0 <= py <= y1:
            t = (py - y0) / (y1 - y0)
            t = t * t * (3 - 2 * t) * 0.5 + t * 0.5
            return [lerp(u, v, t) * K for u, v in zip(a, b)]
    return [c * K for c in (ROBE[0][1:] if py < ROBE[0][0] else ROBE[-1][1:])]


def fold(py, th):
    """Soft folds falling from the sash, deeper toward the hem; a few
    creases over the chest."""
    k = smoothstep(492, 612, py)
    f = k * (0.010 * math.sin(FOLDS * th + 0.6) + 0.004 * math.sin(23 * th + 1.3))
    chest = smoothstep(340, 380, py) * (1 - smoothstep(420, 437, py))
    return f + chest * 0.002 * math.sin(14 * th + 0.4)


def section(py, th, out=0.0):
    """On the robe at height py, turned th (0 facing you, + to her left),
    `out` off the cloth."""
    a, f, b = robe_at(py)
    c, s = math.cos(th), math.sin(th)
    # Round, a little fuller at the sides than an ellipse (the sheet's
    # cloth curves all the way round: its hem and sash show as arcs).
    e = 2.15
    sx = math.copysign(abs(s) ** (2 / e), s)
    cz = math.copysign(abs(c) ** (2 / e), c)
    depth = f if c > 0 else b
    o = out + fold(py, th)
    return Vector(((a + o) * sx, Y(py), (depth + o) * cz))


def robe_normal(py, th):
    dpy = section(py + 0.5, th) - section(py - 0.5, th)
    dth = section(py, th + 0.003) - section(py, th - 0.003)
    return dpy.cross(dth).normalized()


def front_at(x, y):
    """(py, th) of the point on the robe's front at x, model height y."""
    py = SOLE_PX - (y - FLOOR) / K
    lo, hi = -1.5, 1.5
    for _ in range(40):
        mid = (lo + hi) / 2
        if section(py, mid).x < x:
            lo = mid
        else:
            hi = mid
    return py, (lo + hi) / 2


def catmull(pts, per=12):
    """A smooth curve through 2D points."""
    out = []
    ext = [pts[0]] + pts + [pts[-1]]
    for i in range(1, len(ext) - 2):
        p0, p1, p2, p3 = ext[i - 1], ext[i], ext[i + 1], ext[i + 2]
        for k in range(per):
            t = k / per
            out.append(tuple(
                0.5 * (2 * p1[j] + (-p0[j] + p2[j]) * t + (2 * p0[j] - 5 * p1[j] + 4 * p2[j] - p3[j]) * t * t
                       + (-p0[j] + 3 * p1[j] - 3 * p2[j] + p3[j]) * t ** 3) for j in range(2)))
    out.append(pts[-1])
    return out


# The crossed collar (her right panel over her left, as in the sheet): the
# overlap's edge up from the hem on her right, under the sash, across her
# chest to beside her neck on her left, round the back of the neck, and
# down the inner panel on her right into the V.
SKIRT_EDGE = [(-0.175, Y(606)), (-0.165, Y(560)), (-0.15, Y(500)), (-0.13, Y(455))]
OUTER_LAPEL = [(-0.11, Y(430)), (-0.05, Y(400)), (0.015, Y(362)), (0.058, Y(326))]
INNER_LAPEL = [(-0.058, Y(326)), (-0.03, Y(350)), (0.0, Y(372))]
# High round the neck, so no cloth shows above the collar.
NECK_PY = 321
BAND = 0.066
# The hem's border runs down to the hem itself, as drawn (its middle this
# far above the hem).
HEM_UP, HEM_W = 17.0, 0.12


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
    while y < HEM_FRONT:
        py_rows.append(y)
        y += 2.0 if y > 560 else 3.0
    py_rows.append(HEM_FRONT)
    top_py = ROBE[0][0]
    for py in py_rows:
        row = []
        for j in range(AROUND):
            th = 2 * math.pi * j / AROUND
            # The rows stretch down to wherever the hem is at this turn.
            at = top_py + (py - top_py) * (hem(th) - top_py) / (HEM_FRONT - top_py)
            p = section(at, th)
            row.append(vert(p, "robe", gloss=0.18, fuzz=0.05, bones=robe_bones(p)))
        rows.append(row)
    # Closed round the neck; the hem turns in, the cloth's thickness.
    top = vert(Vector((0, Y(ROBE[0][0]) + 0.005, -0.01)), "robe", bones=[("body", 1.0)])
    lip, inner = [], []
    for j in range(AROUND):
        th = 2 * math.pi * j / AROUND
        p = section(hem(th) + 1.5, th, out=-0.012)
        lip.append(vert(p, "gold", gloss=0.4, bones=robe_bones(p)))
        p = section(hem(th) - 8, th, out=-0.03)
        inner.append(vert(p, "robe", bones=robe_bones(p)))
    grid(rows + [lip, inner], True, "robe", close_top=top)


# ---- borders: woven bands laid over the cloth ---------------------------------
# A woven border's vertex: where it is on its band, in half-widths (`x`
# along, `y` across), the band's half width, and the pattern's colour slot.
# Written as `w` lines; mesh.wgsl `woven` draws the pattern from them.
WOVEN = {}
WEAVE = 2.6  # the scroll's period, in half-widths (mesh.wgsl `woven`)
BANDED = []


def band(path_pts, normals, width, bones_fn, closed=False, a="trim", b="gold", lift=0.006,
         across=25, zone="back"):
    """A border strip `width` wide along a line of points on a surface
    (with the surface's normals there): a raised gold cord along each edge,
    standing `lift` proud of the cloth with its edges tucked down into it;
    the scroll in its middle is woven by the shader."""
    half = width / 2
    step = 0.004
    cum = [0.0]
    for p, q in zip(path_pts, path_pts[1:]):
        cum.append(cum[-1] + (q - p).length)
    total = cum[-1]
    n = max(2, int(total / step))
    # A closed band holds a whole number of the pattern, so it meets itself.
    stretch = 1.0
    if closed:
        reps = max(1, round(total / half / WEAVE))
        stretch = reps * WEAVE * half / total
    rows = []
    j = 0
    for i in range(n + 1):
        u = total * i / n
        while j < len(cum) - 2 and cum[j + 1] < u:
            j += 1
        t = (u - cum[j]) / max(cum[j + 1] - cum[j], 1e-9)
        p = path_pts[j].lerp(path_pts[j + 1], t)
        nm = normals[j].lerp(normals[j + 1], t).normalized()
        tan = (path_pts[j + 1] - path_pts[j]).normalized()
        acr = nm.cross(tan).normalized()
        row = []
        for k in range(across):
            v = -1 + 2 * k / (across - 1)
            av = abs(v)
            # The cord: a half-round along each edge.
            cord = math.sqrt(max(0.0, 1 - ((av - 0.9) / 0.1) ** 2))
            h = lift * (1 - smoothstep(0.96, 1.0, av)) + 0.0045 * cord \
                - 0.003 * smoothstep(0.97, 1.0, av)
            q = p + acr * (v * half) + nm * h
            row.append(vert(q, a, gloss=0.35, fuzz=0.02, bones=bones_fn(q), zone=zone))
            WOVEN[row[-1]] = (u * stretch / half, v, half, b)
        rows.append(row)
    first = len(faces)
    grid(rows, False, a)
    # An open strip has no inside to judge by: face it along the cloth's
    # normal.
    fa, fb, fc = faces[first]
    if (verts[fb][0] - verts[fa][0]).cross(verts[fc][0] - verts[fa][0]).dot(normals[0]) < 0:
        faces[first:] = [(x, z, y) for x, y, z in faces[first:]]
    BANDED.extend(range(first, len(faces)))


def borders():
    # The hem's border all round.
    ths = [2 * math.pi * j / 720 for j in range(721)]
    path = [section(hem(th) - HEM_UP, th) for th in ths]
    nrm = [robe_normal(hem(th) - HEM_UP, th) for th in ths]
    band(path, nrm, HEM_W, robe_bones, closed=True)
    # The collar: one band from the hem up the overlap's edge, across her
    # chest, round the back of her neck and down into the V.
    params = [front_at(x, y) for x, y in catmull(SKIRT_EDGE + OUTER_LAPEL, 16)]
    th0 = params[-1][1]
    _, th1 = front_at(*INNER_LAPEL[0])
    ring = []
    for k in range(1, 60):
        t = k / 60
        ring.append((lerp(params[-1][0], NECK_PY, smoothstep(0, 0.15, t)) if t < 0.5 else
                     lerp(NECK_PY, front_at(*INNER_LAPEL[0])[0], smoothstep(0.85, 1, t)),
                     lerp(th0, th1 + 2 * math.pi, t)))
    params += ring + [front_at(x, y) for x, y in catmull(INNER_LAPEL, 16)]
    band([section(py, th) for py, th in params], [robe_normal(py, th) for py, th in params],
         BAND, robe_bones)


# ---- the sleeves: wide trumpet sleeves, a border at the cuff ------------------
def sleeve_frame(s):
    sh = Vector((s * SHOULDER.x * 0.75, SHOULDER.y + 0.01, SHOULDER.z))
    cf = Vector((s * CUFF.x, CUFF.y, CUFF.z))
    axis = (cf - sh).normalized()
    side = Vector((0, 0, 1)).cross(axis).normalized() * (1 if s > 0 else -1)
    fwd = axis.cross(side).normalized()
    if fwd.z < 0:
        fwd = -fwd
    return sh, axis, side, fwd, (cf - sh).length


def sleeve_at(s, t, a, out=0.0, ext=0.0):
    """On sleeve s at t (0 shoulder … 1 cuff) and turn a (0 along the top
    of the arm)."""
    sh, axis, side, fwd, length = sleeve_frame(s)
    tt = min(t, 1.0)
    # A soft bell: it swells a little on the way out, then opens wide.
    r = 0.085 + 0.088 * tt ** 1.4 + 0.012 * math.sin(math.pi * tt) + out
    # Folds round the sleeve, deeper toward the cuff.
    r += 0.006 * t * math.sin(7 * a + 0.8)
    u, v = math.cos(a), math.sin(a)
    # The cloth hangs: fuller underneath, and its underside reaches further
    # out than its top, so the opening tips down (the sheet's cuffs).
    under = max(0.0, -u)
    hang = 0.06 * tt * tt * under
    reach = 0.045 * tt ** 3 * under
    d = side * (u * r * (1.0 + 0.25 * under)) + fwd * (v * r * 0.92)
    return sh + axis * (length * t + ext + reach) + d - Vector((0, hang, 0))


def sleeve_normal(s, t, a):
    dt = sleeve_at(s, t + 0.003, a) - sleeve_at(s, t - 0.003, a)
    da = sleeve_at(s, t, a + 0.003) - sleeve_at(s, t, a - 0.003)
    n = da.cross(dt).normalized()
    c = sleeve_at(s, t, a) - sleeve_at(s, t, a, out=-0.05)
    return n if n.dot(c) > 0 else -n


def sleeve(s):
    bone = f"arm_{'l' if s > 0 else 'r'}"
    n_al, n_ar = 40, 72
    rows = []
    for i in range(n_al + 1):
        t = i / n_al
        w = smoothstep(0.0, 0.3, t)
        bones = ((bone, w), ("body", 1 - w)) if w < 1 else ((bone, 1.0),)
        rows.append([vert(sleeve_at(s, t, 2 * math.pi * j / n_ar), "robe", gloss=0.18, fuzz=0.05,
                          bones=bones) for j in range(n_ar)])
    # The cuff's edge turned in, so it has a thickness.
    for color, out, ext in (("gold", -0.012, 0.008), ("robe", -0.022, -0.03)):
        rows.append([vert(sleeve_at(s, 1.0, 2 * math.pi * j / n_ar, out, ext), color,
                          bones=((bone, 1.0),)) for j in range(n_ar)])
    grid(rows, True, "robe")


def cuff(s):
    bone = f"arm_{'l' if s > 0 else 'r'}"
    t = 0.905
    path = [sleeve_at(s, t, 2 * math.pi * j / 400) for j in range(401)]
    nrm = [sleeve_normal(s, t, 2 * math.pi * j / 400) for j in range(401)]
    band(path, nrm, 0.085, lambda q: ((bone, 1.0),), closed=True)


def armband(s):
    """Up at the shoulder, on 紅髮版's robe only (the 銀白版 has none): a
    fat rope ring in the sash's colour round the sleeve, a gold braid
    beside it. Written to a file of its own (armbands.mesh), for the coats
    that wear it."""
    bone = f"arm_{'l' if s > 0 else 'r'}"
    arm = lambda q: ((bone, 0.8), ("body", 0.2))
    rope([sleeve_at(s, 0.16, 2 * math.pi * j / 72, out=0.016) for j in range(72)], 0.017, "sash",
         arm, closed=True)
    rope([sleeve_at(s, 0.225, 2 * math.pi * j / 72, out=0.01) for j in range(72)], 0.009, "gold",
         arm, closed=True)


# ---- tubes: the sash, the rope, the bow, cords --------------------------------
def tube(path, radius, color, bones, zone="back", closed=False, sides=12, gloss=0.25,
         twist=0.0, b_color=None, caps=True):
    """A round tube along `path` (Vectors), radius(t) or a number."""
    rf = radius if callable(radius) else (lambda t, r=radius: r)
    n = len(path)
    rows = []
    first = len(faces)
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
    face_out(first)


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
    body = (("body", 1.0),)
    # The wide sash: a band round her waist over the robe.
    rows = []
    for i in range(9):
        y = lerp(SASH_TOP, SASH_BOT, i / 8)
        edge = min(i, 8 - i)
        out = 0.012 + (0.004 if edge > 0 else 0.0)
        rows.append([vert(p, "sash", gloss=0.2, fuzz=0.04) for p in ring_path(y, out)])
    grid(rows, True, "sash")
    # A gold braided cord round the waist (it shows through the bow's
    # loops); at the front, a fat rope bow in the sash's colour (the
    # sheet's 紅金編織繩: red rope over gold), its knot, its two ends hanging
    # in tassels with gold caps; the bell hung under the knot.
    yr = lerp(SASH_TOP, SASH_BOT, 0.45)
    rope(ring_path(yr, 0.03), 0.014, "gold", body, closed=True)
    front = ring_path(yr, 0.03)[0]
    knot = front + Vector((0, 0.004, 0.04))
    for s in (1, -1):
        loop = []
        for j in range(49):
            a = 2 * math.pi * j / 48
            # Out to the side and back round: a fat teardrop from the knot.
            x = 0.108 * (1 - math.cos(a))
            yy = 0.088 * math.sin(a) * (0.5 + 0.5 * (1 - math.cos(a)) / 2)
            # Round the body's front as it goes out to the side, the loop
            # standing out from the sash in the middle.
            puff = 0.025 * math.sin(a / 2) ** 2
            loop.append(knot + Vector((s * x, yy, puff - 0.065 * ((1 - math.cos(a)) / 2) ** 1.5)))
        rope(loop, 0.028, "sash", body, closed=True)
    # The knot: the rope wound round and round a core, as tied.
    sphere(knot, Vector((0.046, 0.05, 0.036)), "sash", body, gloss=0.25)
    for k, dy in enumerate((-0.03, 0.0, 0.03)):
        ry = math.sqrt(max(0.0, 1 - (dy / 0.058) ** 2))
        rope([knot + Vector((0.05 * ry * math.sin(2 * math.pi * j / 36), dy,
                             0.04 * ry * math.cos(2 * math.pi * j / 36))) for j in range(36)],
             0.0135, "sash", body, closed=True)
    for s in (1, -1):
        a = knot + Vector((s * 0.03, -0.035, 0.0))
        b = a + Vector((s * 0.05, -0.085, -0.004))
        rope([a.lerp(b, i / 10) for i in range(11)], 0.017, "sash", body)
        # The gold cap, then the tassel's threads flaring out below it.
        tube([b + Vector((0, 0.012, 0)), b + Vector((0, -0.022, 0))], 0.024, "gold", body,
             sides=14, gloss=0.5)
        tube([b + Vector((0, -0.018, 0)), b + Vector((s * 0.004, -0.085, 0))],
             lambda t: lerp(0.019, 0.036, t ** 0.8) * (1 - 0.4 * smoothstep(0.85, 1.0, t)), "sash",
             body, sides=18, gloss=0.15)
    # The bell: round, a raised band round its middle, a loop on top, and
    # a slot from a round hole up its lower half; hung close under the knot.
    bc = knot + Vector((0, -0.098, 0.03))
    bell(bc, 0.063)


def bell(c, r, bones=(("bell", 1.0),)):
    sphere(c, Vector((r, r * 0.94, r)), "gold", bones, gloss=0.75, slot=True)
    tube([c + Vector((r * 1.005 * math.sin(2 * math.pi * j / 64), -0.08 * r,
                      r * 1.005 * math.cos(2 * math.pi * j / 64))) for j in range(64)],
         0.11 * r, "gold", bones, closed=True, sides=8, gloss=0.75)
    tube([c + Vector((0, r * 0.94 + 0.28 * r * (1 - math.cos(2 * math.pi * j / 32)) * 0.5
                      + 0.0, 0.0)) + Vector((0.3 * r * math.sin(2 * math.pi * j / 32), 0.3 * r * (1 - math.cos(2 * math.pi * j / 32)), 0))
          for j in range(32)], 0.08 * r, "gold", bones, closed=True, sides=8, gloss=0.75)


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
            if slot and u.z > 0:
                # The slot up from the bottom, a round hole at its top.
                d = min(max(abs(u.x) - 0.07, u.y + 0.3),
                        math.hypot(u.x, (u.y + 0.32) * 1.0) - 0.14)
                dark = 1 - smoothstep(-0.02, 0.02, d)
                # ...and the slot runs on under the bell's bottom.
            if slot and u.y < -0.3:
                dark = max(dark, 1 - smoothstep(0.05, 0.08, abs(u.x)))
            p = c + Vector((r.x * u.x, r.y * u.y, r.z * u.z)) * (1 - 0.15 * dark)
            row.append(vert(p, color, "ink", 0.85 * dark, gloss=gloss, bones=bones, zone=zone))
        rows.append(row)
    top = vert(c + Vector((0, r.y, 0)), color, bones=bones, zone=zone)
    bot = vert(c - Vector((0, r.y, 0)), color, bones=bones, zone=zone)
    first = len(faces)
    grid([list(reversed(r_)) for r_ in rows], True, color, close_top=top, close_bottom=bot)
    face_out(first)


# ---- ears: big fox ears tilted out, a thick rim, a hollow lined with fluff ----
def ear(s):
    bone = f"ear_{'l' if s > 0 else 'r'}"
    bones = ((bone, 1.0),)
    # The root sinks well into the head (the hair hides the rest), so
    # nothing shows between them.
    base = Vector((s * 0.19, HEAD_C.y + 0.19, -0.06))
    tip = Vector((s * 0.4, HEAD_C.y + 0.62, -0.075))
    axis = (tip - base).normalized()
    across = Vector((1, 0, 0)) - axis * axis.x
    across.normalize()
    fwd = axis.cross(across).normalized()
    if fwd.z < 0:
        fwd = -fwd
    # Turned a little out to the side, as the sheet's ears face.
    turn = math.radians(32) * s
    fwd, across = (fwd * math.cos(turn) + across * math.sin(turn) * -1,
                   across * math.cos(turn) + fwd * math.sin(turn))
    length = (tip - base).length
    n_al, n_ar = 30, 40
    rows = []
    for i in range(n_al + 1):
        t = i / n_al
        w = 0.2 * (1 - t) ** 0.72 * (1 + 0.32 * math.sin(math.pi * t)) + 0.004
        d = 0.085 * (1 - t) ** 0.7 + 0.006
        cup = 0.75 * d * smoothstep(0.1, 0.25, t) * (1 - smoothstep(0.72, 0.95, t))
        # The outer edge bows out, the inner edge is straighter.
        bow = 0.035 * math.sin(math.pi * t) * (1 - t)
        c = base + axis * (length * t)
        row = []
        for j in range(n_ar):
            a = 2 * math.pi * j / n_ar
            ca, sa = math.cos(a), math.sin(a)
            if sa < 0:
                v = d * sa
            else:
                # A rim round the hollow: thick at the edges, sunk between.
                v = d * 0.55 * sa - cup * max(0.0, 1 - ca * ca) ** 1.5
            x = w * ca + (bow * max(0.0, ca * s) if s else 0.0)
            p = c + across * x + fwd * v
            inner = (sa > 0) * (1 - smoothstep(0.62, 0.8, abs(ca))) * smoothstep(0.06, 0.16, t) \
                * (1 - smoothstep(0.8, 0.92, t))
            row.append(vert(p, "hair", "fluff", inner, gloss=0.3, fuzz=0.15, bones=bones, zone="head"))
        rows.append(row)
    top = vert(tip, "hair", bones=bones, zone="head")
    grid(rows, True, "hair", close_bottom=top)
    # Fluff lining the hollow: soft flat locks overlapping from its foot,
    # fanned up and out, the tips curling forward over the rim.
    for ca, t0, ln, wd in ((-0.45, 0.14, 0.13, 0.05), (-0.18, 0.1, 0.17, 0.06),
                           (0.12, 0.12, 0.16, 0.055), (0.42, 0.16, 0.12, 0.05),
                           (-0.3, 0.26, 0.1, 0.04), (0.25, 0.27, 0.1, 0.04)):
        w = 0.2 * (1 - t0) ** 0.72
        root = base + axis * (length * t0) + across * (w * ca)
        spread = across * (0.9 * ca)
        pts, nrm = [], []
        for i in range(13):
            u = i / 12
            pts.append(root + axis * (ln * u) + spread * (0.08 * u)
                       + fwd * (-0.012 + 0.03 * u * u))
            nrm.append((fwd - axis * 0.4 * u).normalized())
        lock(pts, nrm, lambda u, wd=wd: wd * (1 - u) ** 0.8 * min(1.0, 0.5 + 3 * u) + 0.002,
             0.011, "fluff", bones, zone="head")


def lock(pts, nrm, width, thick, color, bones, zone="head", around=12):
    """A flat lock along `pts`, lens-shaped across (`width(u)` wide,
    `thick` deep along `nrm`), coming to a point."""
    rows = []
    n = len(pts)
    for i, (p, nm) in enumerate(zip(pts, nrm)):
        d = (pts[min(i + 1, n - 1)] - pts[max(i - 1, 0)]).normalized()
        acr = d.cross(nm).normalized()
        up = acr.cross(d).normalized()
        u = i / (n - 1)
        w = width(u)
        th = thick * (1 - u) ** 0.6 + 0.0015
        row = []
        for j in range(around):
            a = 2 * math.pi * j / around
            row.append(vert(p + acr * (w * math.cos(a)) + up * (th * math.sin(a)), color,
                            gloss=0.15, fuzz=0.3, bones=bones, zone=zone))
        rows.append(row)
    first = len(faces)
    cap = vert(pts[-1] + (pts[-1] - pts[-2]).normalized() * 0.004, color, bones=bones, zone=zone)
    grid(rows, True, color, close_bottom=cap)
    # Each lock faces out on its own (the ear's own test can't judge them).
    face_out(first)


# ---- normals, occlusion, out ---------------------------------------------------
def piece(build, *args):
    """Build a piece, then turn its triangles outward if they face in (by
    its signed volume: each piece is near enough closed)."""
    first = len(faces)
    build(*args)
    vol = 0.0
    banded = set(BANDED)
    if all(i in banded for i in range(first, len(faces))):
        print(f"{build.__name__}{args}: {len(faces) - first} triangles (bands)")
        return
    # Bands, tubes, balls and locks have faced themselves already.
    done = ORIENTED | banded
    own = [i for i in range(first, len(faces)) if i not in done]
    for i in own:
        a, b, c = faces[i]
        vol += verts[a][0].dot(verts[b][0].cross(verts[c][0]))
    if vol < 0:
        for i in own:
            a, b, c = faces[i]
            faces[i] = (a, c, b)
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


# ---- hair: long and straight, three hairdos ---------------------------------
# Measured off the sheet's turnaround (its hair is let down): the crown at
# y 65, the sides 0.41 out at the cheeks, the back of the head 0.47 behind
# her middle, the fringe bulging to 0.35 forward over the brow; the long
# hair hangs behind her arms and shoulders as a curtain (0.44 out at the
# sides, 0.47 to 0.40 behind), the tips at the sash (y 495). The hairdos
# (三款直髮造型) share the fringe and the locks framing the face.
SHELL_C = Vector((0.0, 0.76, -0.06))
SHELL_R = Vector((0.41, 0.42, 0.41))
HAIR_TIPS = Y(497)
HAIR_GLOSS, HAIR_FUZZ = 0.45, 0.05
# A little of the tips' colour all over, so the white hair has some depth.
HAIR_SHADE = 0.18
# How dark the grooves between clumps are (as occlusion).
GROOVE_DARK = 0.6
GROOVES = {}
ALONG, AROUND = 32, 14
# The parting, on her left (the sheet's front view), degrees round from
# the front.
PART = 22


def ediv(a, b):
    return Vector((a.x / b.x, a.y / b.y, a.z / b.z))


def emul(a, b):
    return Vector((a.x * b.x, a.y * b.y, a.z * b.z))


def on_shell(phi, theta, off=0.0):
    """On the hair's shell at turn `phi` (degrees, 0 facing you, + to her
    left) and height `theta`, `off` further out."""
    p, t = math.radians(phi), math.radians(theta)
    u = Vector((math.sin(p) * math.cos(t), math.sin(t), math.cos(p) * math.cos(t)))
    q = SHELL_C + emul(SHELL_R, u)
    n = ediv(q - SHELL_C, emul(SHELL_R, SHELL_R)).normalized()
    return q + n * off


def off_head(p, clear):
    """`p` pushed out of the head (and her lower face) until `clear` off it."""
    for c, r in ((HEAD_C, HEAD_R), (JAW_C, JAW_R)):
        q = ediv(p - c, r)
        want = 1.0 + clear / min(r)
        if q.length < want:
            p = c + emul(r, q * (want / q.length))
    return p


def curve(pts, n):
    """`n` points along a Catmull-Rom curve through `pts` (3D)."""
    pts = [Vector(p) for p in pts]
    ext = [pts[0] * 2 - pts[1]] + pts + [pts[-1] * 2 - pts[-2]]
    segs = len(pts) - 1
    out = []
    for i in range(n):
        u = i / (n - 1) * segs
        k = min(int(u), segs - 1)
        t = u - k
        p0, p1, p2, p3 = ext[k], ext[k + 1], ext[k + 2], ext[k + 3]
        out.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t
                          + (-p0 + 3 * p1 - 3 * p2 + p3) * t * t * t))
    return out


def clump(path, width, thick, *, clear=0.02, up=None, colors=("hair", "hairtip"), tip=0.72,
          bones=lambda t: (("head", 1.0),), root=0.15, point=1.6, flat=0.45, closed=False,
          edge=0.7, gloss=HAIR_GLOSS):
    """A lock of hair along `path`: a lens-shaped section (full on top,
    flatter underneath, sharp at its edges) `width` across and `thick` deep
    (numbers, or functions of t: 0 root … 1 tip), kept `clear` off the
    head; `up` the way its top faces (out from the head by default). Its
    colour turns to the tip's from `tip` of the way on; it swells from
    `root` to full and tapers to a point (`point`: higher keeps it full
    longer). Its edges and underside are shaded as grooves (`edge`), so
    each lock stands out from the ones beside it."""
    pts = [off_head(p, clear) for p in curve(path, ALONG)]
    first, first_face = len(verts), len(faces)
    rings = []
    for i, p in enumerate(pts):
        t = i / (ALONG - 1)
        tan = (pts[min(i + 1, ALONG - 1)] - pts[max(i - 1, 0)]).normalized()
        n = up(p, t) if up else ediv(p - HEAD_C, emul(HEAD_R, HEAD_R)).normalized()
        b = tan.cross(n).normalized()
        n = b.cross(tan).normalized()
        if closed:
            k = 1.0
        else:
            k = (smoothstep(0.0, root, t) * 0.6 + 0.4) if root > 0 else 1.0
            k *= max(1.0 - t ** point, 0.0) ** 0.55
        w = max((width(t) if callable(width) else width) * k, 0.003)
        h = (thick(t) if callable(thick) else thick) * (1.0 if closed else max(k, 0.25))
        h = max(h * (k ** 0.3 if not closed else 1.0), 0.003)
        ring = []
        for j in range(AROUND):
            a = 2 * math.pi * j / AROUND
            x, y = math.cos(a), math.sin(a)
            q = p + b * (w * x) + n * (h * y * (1.0 if y > 0 else flat))
            mix = smoothstep(tip, 1.0, t) if tip < 1.0 else 0.0
            if colors[0] == "hair":
                mix = HAIR_SHADE + (1.0 - HAIR_SHADE) * mix
            i_v = vert(q, colors[0], colors[1], mix, gloss, HAIR_FUZZ, bones(t), "head")
            GROOVES[i_v] = edge * min(smoothstep(0.7, 1.0, abs(x)) * 0.9 + (0.3 if y < -0.5 else 0.0), 1.0)
            ring.append(i_v)
        rings.append(ring)
    grid(rings, True, None)
    for ring, top in ((rings[0], True), (rings[-1], False)):
        c = sum((verts[i][0] for i in ring), Vector()) / AROUND
        v0 = verts[ring[0]]
        centre = vert(c, v0[1], v0[2], v0[3], gloss, HAIR_FUZZ, v0[6], "head")
        for j in range(AROUND):
            a, b_ = ring[j], ring[(j + 1) % AROUND]
            faces.append((centre, b_, a) if top else (centre, a, b_))
    face_out(first_face)


def hairline(phi):
    """How far down the hair's main mass comes (shell height, degrees) at
    turn `phi`: above the brow in front (the fringe covers the rest),
    down past the temples at the sides, to the nape behind."""
    a = abs(phi)
    if a < 40:
        return 42.0
    if a < 100:
        return 42.0 - 50.0 * smoothstep(40, 100, a)
    # Behind, it ends under the top of the long hair (which carries on
    # down from it).
    return -8.0 + 13.0 * smoothstep(100, 140, a)


def scalp(knot=None, grooves=9):
    """The hair's main mass: one sheet over the shell down to the hairline,
    its lower edge coming in onto the head; with `knot` (a point on the
    shell), grooved along the way the hair is combed to it, gathered tight
    there."""
    rows, cols = 48, 220
    if knot is not None:
        toward = ediv(knot - SHELL_C, SHELL_R).normalized()
        e1 = (Vector((1, 0, 0)) - toward * toward.x).normalized()
        e2 = toward.cross(e1)
    first_face = len(faces)
    rows_v = []
    for i in range(rows + 1):
        row = []
        for j in range(cols):
            phi = -180 + 360 * j / cols
            low = hairline(phi)
            u = i / rows
            theta = low + (89.5 - low) * u
            pr, tr = math.radians(phi), math.radians(theta)
            q = Vector((math.sin(pr) * math.cos(tr), math.sin(tr), math.cos(pr) * math.cos(tr)))
            off, groove = 0.0, 0.0
            if knot is not None:
                ang = math.acos(max(-1.0, min(1.0, q.dot(toward))))
                psi = math.atan2(q.dot(e2), q.dot(e1))
                g = (0.5 + 0.5 * math.cos(grooves * psi)) ** 0.6
                amp = 0.01 * smoothstep(0.15, 0.7, ang) * smoothstep(0.0, 0.1, u)
                off += amp * (g - 0.6)
                groove = 0.35 * (1.0 - g) ** 2 * smoothstep(0.15, 0.7, ang)
                off -= 0.02 * (1.0 - smoothstep(0.0, 0.3, ang))
            else:
                # Combed straight down from the crown.
                g = (0.5 + 0.5 * math.cos(grooves * pr)) ** 0.6
                off += 0.012 * (g - 0.6) * smoothstep(0.0, 0.1, u)
                groove = 0.7 * (1.0 - g) ** 2 * (1 - smoothstep(0.8, 1.0, u))
            pos = on_shell(phi, theta, off)
            # The lower edge comes in onto the head, so no edge hangs in
            # the air.
            tuck = 1.0 - smoothstep(95, 125, abs(phi))
            if u < 0.25 and tuck > 0:
                on_head = HEAD_C + emul(HEAD_R, ediv(pos - HEAD_C, HEAD_R).normalized())
                pos = pos.lerp(off_head(on_head, 0.01), tuck * (1.0 - smoothstep(0.0, 0.25, u)))
            i_v = vert(pos, "hair", "hairtip", HAIR_SHADE, HAIR_GLOSS, HAIR_FUZZ, (("head", 1.0),), "head")
            GROOVES[i_v] = groove
            row.append(i_v)
        rows_v.append(row)
    grid(rows_v, True, None)
    # Out from the head.
    a, b, c = faces[first_face + cols * rows]
    nrm = (verts[b][0] - verts[a][0]).cross(verts[c][0] - verts[a][0])
    if nrm.dot(verts[a][0] - SHELL_C) < 0:
        faces[first_face:] = [(x, z, y) for x, y, z in faces[first_face:]]
    ORIENTED.update(range(first_face, len(faces)))


def face_point(x, y, out):
    """In front of her face at (x, y), `out` off it."""
    k = 1.0 - (x / HEAD_R.x) ** 2 - ((y - HEAD_C.y) / HEAD_R.y) ** 2
    p = Vector((x, y, HEAD_C.z + HEAD_R.z * math.sqrt(max(k, 0.0))))
    n = ediv(p - HEAD_C, emul(HEAD_R, HEAD_R)).normalized()
    return p + n * out


def fringe_and_locks():
    """The fringe, parted on her left: big locks sweep from the parting
    across her brow to her right, ending above the brows (higher in the
    middle, where the crest shows), smaller ones fall over her left
    temple; full, bulging out over the brow (0.35 forward at y 0.94). Then
    the locks framing her face: from the temples down beside the cheeks,
    turning in under the jaw (y 0.36)."""
    # (turn and height on the way down, then points in front of the face:
    # x, y, out), width, thickness. Broad locks that overlap, so the
    # fringe reads as a few big shapes, as the sheet draws it: the big
    # sweep over to her right brow on top, the shorter ones under it.
    fringe = [
        # her right temple, under the big sweep
        ((-62, 44), [(-0.33, 0.86, 0.05), (-0.37, 0.72, 0.02)], 0.16, 0.045),
        # the big sweep across to her right brow
        ((-30, 58), [(-0.08, 1.0, 0.1), (-0.2, 0.9, 0.08), (-0.28, 0.8, 0.05), (-0.3, 0.74, 0.03)],
         0.27, 0.075),
        # under it, ending higher (the crest shows in the middle)
        ((-10, 62), [(0.02, 1.0, 0.1), (-0.07, 0.9, 0.08), (-0.12, 0.82, 0.05)], 0.22, 0.07),
        ((8, 64), [(0.1, 0.99, 0.1), (0.07, 0.9, 0.075), (0.04, 0.84, 0.05)], 0.19, 0.065),
        # her left, falling over the temple
        ((30, 58), [(0.2, 0.96, 0.09), (0.21, 0.86, 0.065), (0.2, 0.79, 0.04)], 0.2, 0.065),
        ((52, 48), [(0.3, 0.9, 0.065), (0.33, 0.8, 0.045), (0.33, 0.71, 0.025)], 0.17, 0.045),
    ]
    for (phi, theta), front, w, h in fringe:
        # Down the front of the hair's shell (it stands 0.15 to 0.2 off the
        # brow: the sheet's fringe bulges 0.35 forward at y 0.94), then in
        # to just off the face at its end.
        x_end, y_end, _ = front[-1]
        phi_end = math.degrees(math.asin(max(-0.95, min(0.95, x_end / 0.4))))
        # (its middle set in from the shell by about its thickness, so its
        # outside lies where the sheet's fringe does)
        path = [on_shell(PART + (phi - PART) * 0.35, 66, -0.04), on_shell((PART + phi) / 2, 52, -0.03),
                on_shell(phi, 34, -0.035),
                on_shell(phi_end, 14, -0.05), face_point(x_end, y_end, 0.02)]
        clump(path, w, h, clear=0.03, tip=0.75, point=1.6, root=0.3, edge=0.4)
    # Framing the face: from the temples down beside the cheeks to the
    # jaw, the end turning in.
    for s in (1, -1):
        def locks(t, s=s):
            w = smoothstep(0.15, 0.5, t)
            side = "l" if s > 0 else "r"
            return (("head", 1.0 - w), (f"hair_{side}", w))
        clump([on_shell(62 * s, 30, -0.05), Vector((0.39 * s, 0.68, 0.12)), Vector((0.4 * s, 0.52, 0.11)),
               Vector((0.38 * s, 0.4, 0.1)), Vector((0.33 * s, 0.34, 0.14))],
              0.16, 0.05, clear=0.03, tip=0.65, point=1.15, bones=locks, edge=0.4,
              up=lambda p, t, s=s: Vector((s * 0.9, 0.0, 0.45)).normalized())
# The curtain's outline round her back, from the sheet's back and side
# views: its outside's half width, how far behind her middle at the back,
# and at the sides, at each height.
# (Over the shoulders it is as wide as they are, so from the front it shows
# either side of her, outside the sleeves, as the sheet's front view has
# it; its sides are hidden behind her arms from the side.)
CURTAIN = [(0.5, 0.46, -0.46, -0.04), (0.28, 0.5, -0.47, -0.12), (0.02, 0.5, -0.44, -0.16),
           (-0.16, 0.48, -0.41, -0.18)]


def curtain_at(u, y):
    """On the curtain's outside at `u` round her back (−1 her right side,
    0 the middle of her back, 1 her left side) and height `y`."""
    rows = CURTAIN
    if y >= rows[0][0]:
        r = rows[0]
    elif y <= rows[-1][0]:
        r = rows[-1]
    else:
        for r0, r1 in zip(rows, rows[1:]):
            if r1[0] <= y <= r0[0]:
                t = smoothstep(r0[0], r1[0], y)
                r = tuple(lerp(a, b, t) for a, b in zip(r0, r1))
                break
    _, half, back, side = r
    a = u * math.pi / 2
    return Vector((half * math.sin(a), y, side + (back - side) * math.cos(a)))


def curtain(grooves=14):
    """The long hair let down behind her arms and shoulders to the sash: one
    slab of hair round her back from one side to the other, hanging from
    under the shell, with `grooves` grooves down it; low down each strip
    between them ends in a point (the sheet: one smooth mass, splitting
    into pointed locks at the ends)."""
    def bones(v):
        w = smoothstep(0.1, 0.5, v)
        return (("head", 1.0 - w), ("hair_b", 0.6 * w), ("body", 0.4 * w))
    cols, rows = 15 * grooves, 44
    first_face = len(faces)
    outer, inner = [], []
    for i in range(rows + 1):
        v = i / rows
        o_row, i_row = [], []
        for j in range(cols + 1):
            u = -1.0 + 2.0 * j / cols
            # Each strip between grooves ends in a point, the middle of the
            # strip lowest; a strip's length varies a little.
            k = (u + 1.0) / 2.0 * grooves
            f = k - math.floor(k)
            strip = int(min(k, grooves - 1))
            tip = HAIR_TIPS + 0.025 * math.cos(2.3 * strip) - 0.07 * (1 - abs(2 * f - 1))
            # From under the shell round the back and sides, then down.
            phi = math.copysign(180 - abs(u) * 98, u) if u else 180.0
            first = curtain_at(u, 0.5)
            if v < 0.12:
                # From under the shell's lower edge, down its outside (so it
                # carries on from the hair over her head, no ledge), into
                # the curtain.
                w_ = v / 0.12
                on = on_shell(phi, lerp(18.0, -26.0, w_), lerp(-0.04, 0.015, smoothstep(0.0, 0.3, w_)))
                p = on.lerp(first, smoothstep(0.55, 1.0, w_))
            else:
                y = lerp(0.5, tip + 0.07 * (1 - abs(2 * f - 1)), (v - 0.12) / 0.88)
                y = min(y, 0.5)
                # The last stretch runs down to the strip's point.
                y = lerp(y, tip, smoothstep(0.86, 1.0, v))
                p = curtain_at(u, y)
                # Ends turned in a little.
                p += Vector((-p.x, 0, -(p.z + 0.2))).normalized() * 0.03 * smoothstep(0.8, 1.0, v)
            out = Vector((p.x, 0.0, p.z + 0.2)).normalized()
            g = (0.5 + 0.5 * math.cos(2 * math.pi * k)) ** 0.5
            groove = (1.0 - g) ** 2
            p = p + out * (0.014 * (g - 0.7) * smoothstep(0.08, 0.2, v))
            # Thinning to the points; the side edges rounded off.
            th = 0.065 * (1.0 - smoothstep(0.82, 1.0, v)) + 0.004
            th *= 0.4 + 0.6 * math.sqrt(max(0.0, 1.0 - abs(u) ** 6))
            mix = HAIR_SHADE + (1.0 - HAIR_SHADE) * smoothstep(0.55, 1.0, v)
            a = vert(p, "hair", "hairtip", mix, HAIR_GLOSS, HAIR_FUZZ, bones(v), "head")
            b = vert(p - out * th, "hair", "hairtip", mix, HAIR_GLOSS, HAIR_FUZZ, bones(v), "head")
            GROOVES[a] = 0.8 * groove * smoothstep(0.08, 0.2, v)
            GROOVES[b] = 0.6
            o_row.append(a)
            i_row.append(b)
        outer.append(o_row)
        inner.append(i_row)
    grid(outer, False, None)
    grid([list(reversed(r)) for r in inner], False, None)
    # Close the edges: the sides, the top and the points.
    for side in (0, cols):
        col_o = [r[side] for r in outer]
        col_i = [r[side] for r in inner]
        grid([col_o, col_i] if side else [col_i, col_o], False, None)
    grid([outer[0], inner[0]], False, None)
    grid([inner[-1], outer[-1]], False, None)
    face_out(first_face)


def ribbon(at, axis, bone, k=1.8):
    """A thin red cord round a knot at `at` (the hair gathered along
    `axis`), tied in a bow: a band, two loops and two long ends; `k` times
    the size first drawn (the sheet's bow is as wide as the gathered
    hair)."""
    rb = lambda t: ((bone, 1.0),)
    V = lambda x, y, z: Vector((x, y, z)) * k
    axis = axis.normalized()
    u = Vector((1, 0, 0))
    v = axis.cross(u).normalized()
    band = [at + (u * math.cos(a) + v * math.sin(a)) * 0.045 for a in [2 * math.pi * i / 16 for i in range(17)]]
    clump(band, 0.012, 0.008, clear=0.0, root=0.0, colors=("ribbon", "ribbon"), edge=0.0, bones=rb,
          closed=True, gloss=0.3, up=lambda p, t: (p - at - axis * (p - at).dot(axis)).normalized())
    back = lambda p, t: (-axis + Vector((0, 0.3, 0))).normalized()
    bow = at - axis * 0.03
    for s in (1, -1):
        loop = [bow, bow + V(s * 0.05, 0.04, 0.0), bow + V(s * 0.11, 0.035, -0.005),
                bow + V(s * 0.115, -0.02, -0.005), bow + V(s * 0.045, -0.015, 0.0), bow]
        clump(loop, 0.012 * k, 0.007 * k, clear=0.0, root=0.0, colors=("ribbon", "ribbon"), edge=0.0, bones=rb,
              closed=True, gloss=0.3, up=back)
        clump([bow, bow + V(s * 0.025, -0.07, -0.01), bow + V(s * 0.05, -0.15, -0.02),
               bow + V(s * 0.06, -0.21, -0.015)],
              0.012 * k, 0.006 * k, clear=0.0, root=0.0, point=6.0, colors=("ribbon", "ribbon"), edge=0.0,
              bones=rb, gloss=0.3, up=back)
    clump([bow + axis * 0.012, bow - axis * 0.012], 0.02 * k, 0.016 * k, clear=0.0, root=0.0, point=8.0,
          colors=("ribbon", "ribbon"), edge=0.0, bones=rb, gloss=0.3, up=lambda p, t: Vector((0, 1, 0)))


def hair_ponytail():
    """綁單馬尾 (the sheet's hairdos, front and back): the hair let down,
    and its top layer gathered high on the back of her head, tied with a
    thin red cord in a bow, the gathered lock hanging down the middle of
    her back over the rest, swinging a little to one side."""
    knot = on_shell(180, 30, -0.01)
    fringe_and_locks()
    curtain()
    scalp(knot=knot)
    def tail_bones(t):
        w = smoothstep(0.0, 0.35, t)
        return (("head", 1.0 - w), ("hair_b", w))
    out = (knot - SHELL_C).normalized()
    # The gathered lock: out of the knot and down the middle of her back,
    # lying on the curtain, swinging a little to her left and back.
    def on_curtain(u, y, off):
        return curtain_at(u, y) + Vector((0, 0, -off))
    for k, (du, w, ln) in enumerate(((0.02, 0.1, 0.0), (-0.04, 0.08, 0.1))):
        clump([knot + Vector((du * 0.2, 0.0, 0.0)),
               knot + Vector((du * 0.4, -0.1, -0.08)),
               on_curtain(du + 0.02, 0.5, 0.08),
               on_curtain(du + 0.12, 0.27, 0.07),
               on_curtain(du - 0.02, 0.06 + ln, 0.06),
               on_curtain(du + 0.1, -0.08 + ln, 0.05)],
              w, 0.085, clear=0.02, root=0.0, point=1.6, tip=0.6, bones=tail_bones, edge=0.4,
              flat=0.8, up=lambda p, t: Vector((0, 0.15, -1)).normalized())
    ribbon(knot + out * 0.035, out, "hair_b")


HAIRDOS = {"ponytail": hair_ponytail}


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


def occlusion(nrm, others=None):
    """Each vertex's ambient occlusion; `others` (points, triangles) shade
    it too without being written (the figure, under its armbands)."""
    hp, hf = head_proxy()
    pts = [v[0] for v in verts] + hp
    polys = [tuple(f) for f in faces] + [tuple(i + len(verts) for i in f) for f in hf]
    if others:
        base = len(pts)
        pts += others[0]
        polys += [tuple(i + base for i in f) for f in others[1]]
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


def write_mesh(path, others=None):
    nrm = normals()
    ao = occlusion(nrm, others)
    zone = None
    with open(path, "w") as f:
        for i, ((p, a, b, t, gloss, fuzz, bones, z), n, o) in enumerate(zip(verts, nrm, ao)):
            if z != zone:
                f.write(f"z {z}\n")
                zone = z
            o *= 1.0 - GROOVE_DARK * GROOVES.get(i, 0.0)
            total = sum(w for _, w in bones) or 1.0
            bs = " ".join(f"{name} {w / total:.3f}" for name, w in bones if w > 1e-4)
            f.write(f"v {p.x:.4f} {p.y:.4f} {p.z:.4f} {n.x:.3f} {n.y:.3f} {n.z:.3f} "
                    f"{a} {b} {t:.3f} {gloss} {fuzz} {o:.3f} {bs}\n")
            if i in WOVEN:
                x, y, half, slot = WOVEN[i]
                f.write(f"w {x:.4f} {y:.3f} {half:.4f} {slot}\n")
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
    # On the head's surface: a little nose, a mouth only just raised.
    def front(dy):
        return hc.z + hr.z * math.sqrt(max(0.0, 1 - (dy / hr.y) ** 2))
    def jaw_front(y):
        d = (y - JAW_C.y) / JAW_R.y
        return JAW_C.z + JAW_R.z * math.sqrt(max(0.0, 1 - d * d))
    # (measured off the side view: the nose's tip at y 0.53, 0.296
    # forward, hardly past the cheeks; the mouth 0.12 wide, on the lower
    # face, only just raised)
    nose = Vector((0, 0.532, 0.282))
    mouth = Vector((0, hc.y - 0.205, jaw_front(hc.y - 0.205) - 0.06 + 0.009))
    cheek = Vector((0.16, hc.y - 0.15, hc.z + 0.1))
    # The face, measured off the sheet's front view: the eyes' middles
    # 51 px either side of the face's (238.5), at y 228, 47 px round; set
    # 0.02 higher, as the app's camera looks down 9° and the face, in
    # front, shows lower than the crown.
    tail = tail.replace("nose: (0, -0.18, 0.56)", f"nose: {v3(nose - hc)}")
    tail = tail.replace("eye: (at: (0.34, -0.09), r: (0.205, 0.25)",
                        "eye: (at: (0.513, -0.105), r: (0.25, 0.25)")
    tail = tail.replace("blush: (at: (0.5, -0.4), r: (0.2, 0.12)",
                        "blush: (at: (0.5, -0.46), r: (0.15, 0.085)")
    head_part = head_part.replace("bake_cell: 0.025", "bake_cell: 0.008")
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
    out.append(f'        (name: "jaw", bone: "head", shape: Ellipsoid({v3(JAW_R)}), at: {v3(JAW_C)},\n'
               f'            blend: 0.06, color: "skin", fuzz: 0, zone: "chin"),\n')
    out.append(f'        (name: "nose", bone: "head", shape: Ellipsoid((0.024, 0.017, 0.014)), at: {v3(nose)},\n'
               f'            blend: 0.015, color: "skin", fuzz: 0, zone: "head"),\n')
    out.append(f'        (name: "mouth", bone: "head", shape: Ellipsoid((0.13, 0.07, 0.06)), at: {v3(mouth)},\n'
               f'            blend: 0.03, color: "skin", fuzz: 0, zone: "chin"),\n')
    out.append(f'        (name: "cheek", bone: "head", shape: Ellipsoid((0.12, 0.1, 0.11)), at: {v3(cheek)},\n'
               f'            blend: 0.07, color: "skin", fuzz: 0, mirror: true, zone: "cheek"),\n')
    out.append("    ],\n")
    with open(path, "w", encoding="utf-8") as f:
        f.write("".join(out) + tail)
    print(f"{path}: written")


if __name__ == "__main__":
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = args[0] if args else "."
    os.makedirs(out, exist_ok=True)
    piece(legs)
    piece(hands)
    piece(neck)
    piece(robe)
    piece(borders)
    piece(sleeve, 1)
    piece(sleeve, -1)
    piece(cuff, 1)
    piece(cuff, -1)
    piece(sash)
    piece(ear, 1)
    piece(ear, -1)
    write_mesh(os.path.join(out, "figure.mesh"))
    # 紅髮版's armbands, in a file of their own, shaded by the figure.
    figure = ([v[0] for v in verts], list(faces))
    for store in (verts, faces, BANDED):
        store.clear()
    WOVEN.clear()
    ORIENTED.clear()
    piece(armband, 1)
    piece(armband, -1)
    write_mesh(os.path.join(out, "armbands.mesh"), figure)
    # Each hairdo in a file of its own, shaded by the figure and the head.
    for style, build in HAIRDOS.items():
        for store in (verts, faces, BANDED):
            store.clear()
        WOVEN.clear()
        ORIENTED.clear()
        GROOVES.clear()
        build()
        write_mesh(os.path.join(out, f"hair_{style}.mesh"), figure)
    write_ron(os.path.join(out, "human_figure.ron"))
