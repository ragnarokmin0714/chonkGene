# The fox's human form's hair as a real mesh (Roger, 2026-11-17: the hair
# made of blended round shapes looked like melted wax next to his parts
# picture, …3D示意與局部拆解圖2.jpeg). Run with Blender (headless):
#
#   blender -b --factory-startup -P hair_blend.py -- ponytail hair_ponytail.mesh
#
# Every lock of hair is a clump: a lens-shaped cross-section (full on top,
# flatter underneath, sharp at its edges) swept along a smooth path that
# hugs the head, full most of its length and tapering to a point. Clumps
# overlap, so the grooves between them show as in the picture. Blender's
# BVH gives each vertex its ambient occlusion (the head and the clumps
# shading each other); the mesh is written as text for
# `chonkgene_render::mesh::import` (`v …` and `f …` lines).
#
# Coordinates are the model's (human.ron): x to her left, y up, z forward.
import math
import sys

from mathutils import Vector
from mathutils.bvhtree import BVHTree

# ---- the head (as gen_human.py has it) ------------------------------------
HEAD_C = Vector((0.0, 0.58, 0.0))
HEAD_R = Vector((0.72, 0.61, 0.59))
# The shell the hair lies on: round the head, set back off the forehead.
SHELL_C = Vector((0.0, 0.68, -0.1))
SHELL_R = Vector((0.83, 0.69, 0.75))
# Where the ponytail is tied: high on the back of the crown.
KNOT = Vector((0.0, 1.3, -0.42))

GLOSS, FUZZ = 0.45, 0.05
# Ambient occlusion: how far a ray looks for something in the way, and
# how dark all of it in the way makes it.
AO_REACH, AO_STRENGTH = 0.15, 0.8
# How dark the grooves between clumps are (as occlusion).
GROOVE_DARK = 0.95
ALONG, AROUND = 30, 14


def ediv(a, b):
    return Vector((a.x / b.x, a.y / b.y, a.z / b.z))


def emul(a, b):
    return Vector((a.x * b.x, a.y * b.y, a.z * b.z))


def smoothstep(a, b, x):
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def shell(phi, theta, off=0.0):
    """On the shell at turn `phi` (degrees, 0 facing you, + to her left)
    and height `theta`, `off` further out."""
    p, t = math.radians(phi), math.radians(theta)
    u = Vector((math.sin(p) * math.cos(t), math.sin(t), math.cos(p) * math.cos(t)))
    return out_from(SHELL_C, SHELL_R, SHELL_C + emul(SHELL_R, u), off)


def out_from(c, r, p, off):
    n = ediv(p - c, emul(r, r)).normalized()
    return p + n * off


def face(x, y, off=0.0):
    """On the front of the head at (x, y), `off` further out."""
    k = 1.0 - ((x - HEAD_C.x) / HEAD_R.x) ** 2 - ((y - HEAD_C.y) / HEAD_R.y) ** 2
    p = Vector((x, y, HEAD_C.z + HEAD_R.z * math.sqrt(max(k, 0.0))))
    return out_from(HEAD_C, HEAD_R, p, off)


def lift(p, clear):
    """`p` pushed out of the head until at least `clear` off it."""
    q = ediv(p - HEAD_C, HEAD_R)
    r = q.length
    want = 1.0 + clear / 0.62
    return HEAD_C + emul(HEAD_R, q * (want / r)) if r < want else p


def lift_scalp(p, clear):
    """`p` pushed out of the scalp (where there is one) until at least
    `clear` off it: a curve between points on the shell cuts under it."""
    q = ediv(p - SHELL_C, SHELL_R)
    r = q.length
    phi = math.degrees(math.atan2(q.x, q.z))
    theta = math.degrees(math.asin(max(-1.0, min(1.0, q.y / r))))
    if theta < hairline(phi) - 4:
        return p
    want = 1.0 + (scalp_base(phi, theta) + 0.03 + clear) / 0.72
    if r >= want:
        return p
    return SHELL_C + emul(SHELL_R, q * (want / r))


def catmull(pts, n):
    """`n` points along a Catmull-Rom curve through `pts`."""
    ext = [pts[0] * 2 - pts[1]] + list(pts) + [pts[-1] * 2 - pts[-2]]
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


# ---- the clumps ----------------------------------------------------------
verts = []  # (pos, color a, color b, t, bones)
faces = []
clumps = []  # (first vertex, ring count) for normals


def clump(path, width, thick, *, clear=0.03, up=None, colors=("hair", "hairtip"), tip=0.75,
          bones=lambda t: [("head", 1.0)], root=0.12, point=1.6, flat=0.45, closed=False,
          edge=0.7):
    """A clump along `path` (points, at least two), `width` across and
    `thick` deep (each a number or a function of t, 0 root … 1 tip), kept
    `clear` off the head; `up` the way its top faces (out from the head by
    default). The colour runs from the first slot to the second toward the
    tip (from `tip` of the way on); it tapers from `root` to full and on to
    a point at the tip (`point`: higher keeps it full longer). Its edges
    and underside take `edge` of the second colour, so each clump stands
    out from the ones beside it as in the picture."""
    pts = catmull([Vector(p) for p in path], ALONG)
    # Roots stay buried; from a fifth of the way on, kept above the scalp.
    pts = [lift_scalp(lift(p, clear), 0.012) if i > ALONG // 5 else lift(p, clear)
           for i, p in enumerate(pts)]
    first = len(verts)
    for i, p in enumerate(pts):
        t = i / (ALONG - 1)
        tan = (pts[min(i + 1, ALONG - 1)] - pts[max(i - 1, 0)]).normalized()
        n = up(p, t) if up else ediv(p - HEAD_C, emul(HEAD_R, HEAD_R)).normalized()
        b = tan.cross(n).normalized()
        n = b.cross(tan).normalized()
        if closed:
            k = 1.0
        else:
            k = smoothstep(0.0, root, t) * 0.6 + 0.4 if root > 0 else 1.0
            k *= max(1.0 - t ** point, 0.0) ** 0.55
        w = (width(t) if callable(width) else width) * k
        h = (thick(t) if callable(thick) else thick) * max(k, 0.25 if not closed else 1.0)
        h = max(h * k ** 0.3, 0.004)
        w = max(w, 0.004)
        for j in range(AROUND):
            a = 2 * math.pi * j / AROUND
            x, y = math.cos(a), math.sin(a)
            # Full on top, flatter underneath, sharp at the edges.
            yy = y * (1.0 if y > 0 else flat)
            q = p + b * (w * x) + n * (h * yy)
            mix = smoothstep(tip, 1.0, t) if tip < 1.0 else 0.0
            groove = smoothstep(0.7, 1.0, abs(x)) * 0.9 + (0.3 if y < -0.5 else 0.0)
            verts.append((q, colors[0], colors[1], mix, bones(t), edge * min(groove, 1.0)))
    # Which way round the rings go: the faces must face out.
    mid = ALONG // 2
    a, b_ = first + mid * AROUND, first + mid * AROUND + 1
    nrm = (verts[b_][0] - verts[a][0]).cross(verts[a + AROUND][0] - verts[a][0])
    out = nrm.dot(verts[a][0] - pts[mid]) > 0
    for i in range(ALONG - 1):
        for j in range(AROUND):
            a = first + i * AROUND + j
            b_ = first + i * AROUND + (j + 1) % AROUND
            c = a + AROUND
            d = b_ + AROUND
            if out:
                faces.extend([(a, b_, d), (a, d, c)])
            else:
                faces.extend([(a, d, b_), (a, c, d)])
    # Cap the root and the tip.
    for ring, flip in ((0, out), (ALONG - 1, not out)):
        centre = len(verts)
        ring_pts = [verts[first + ring * AROUND + j] for j in range(AROUND)]
        c = sum((v[0] for v in ring_pts), Vector()) / AROUND
        v0 = ring_pts[0]
        verts.append((c, v0[1], v0[2], v0[3], v0[4], v0[5]))
        for j in range(AROUND):
            a = first + ring * AROUND + j
            b_ = first + ring * AROUND + (j + 1) % AROUND
            faces.append((centre, b_, a) if flip else (centre, a, b_))
    clumps.append(first)


def pony_bones(start):
    """On the ponytail's bone, eased in from `start` along (the knot stays
    with the head)."""
    def f(t):
        w = smoothstep(0.0, start, t) if start > 0 else 1.0
        return [("head", 1.0 - w), ("hair_b", w)] if w < 0.999 else [("hair_b", 1.0)]
    return f


# The parting, on her right (degrees round from the front).
PART = -16


def hairline(phi):
    """How far down the hair comes (shell height, degrees) at turn `phi`:
    off the forehead in front, past the ears at the sides, to the nape."""
    a = abs(phi)
    if a < 45:
        return 56.0
    if a < 110:
        return 56.0 - 60.0 * smoothstep(45, 110, a)
    return -4.0 - 30.0 * smoothstep(110, 160, a)


def scalp_base(phi, theta):
    """How far out from the shell the hair's main mass is at (phi, theta),
    before its grooves: fuller low on the sides, its edge rolled under."""
    low = hairline(phi) - 4
    u = max(0.0, (theta - low) / (89.5 - low))
    off = 0.03 + 0.05 * math.sin(math.pi * min(u * 1.6, 1.0))
    return off


def scalp(toward=None, grooves=34):
    """The hair's main mass: one sheet over the shell down to the hairline,
    fuller over the middle, its edge rolled under; with `toward` (a
    direction from the shell's middle), grooved along the way the hair is
    combed to it (a ponytail's knot), the grooves fading where they meet."""
    rows, cols = 56, 240
    first = len(verts)
    if toward is not None:
        e1 = (Vector((1, 0, 0)) - toward * toward.x).normalized()
        e2 = toward.cross(e1)
    grid = []
    for i in range(rows + 1):
        row = []
        for j in range(cols):
            phi = -180 + 360 * j / cols
            low = hairline(phi) - 4
            u = i / rows
            theta = low + (89.5 - low) * u
            pr, tr = math.radians(phi), math.radians(theta)
            q = Vector((math.sin(pr) * math.cos(tr), math.sin(tr), math.cos(pr) * math.cos(tr)))
            off, groove = scalp_base(phi, theta), 0.0
            if toward is not None:
                ang = math.acos(max(-1.0, min(1.0, q.dot(toward))))
                psi = math.atan2(q.dot(e2), q.dot(e1))
                g = (0.5 + 0.5 * math.cos(grooves * psi)) ** 0.6
                amp = 0.028 * smoothstep(0.12, 0.7, ang) * smoothstep(0.0, 0.08, u)
                off += amp * (g - 0.6)
                groove = 0.9 * (1.0 - g) ** 2 * smoothstep(0.12, 0.7, ang)
                # Gathered tight at the knot.
                off -= 0.03 * (1.0 - smoothstep(0.0, 0.35, ang))
            pos = shell(phi, theta, off)
            # Its lower edge comes in onto the head (the shell stands off
            # it at the sides), so no edge hangs in the air.
            if u < 0.3:
                hq = ediv(pos - HEAD_C, HEAD_R)
                on_head = out_from(HEAD_C, HEAD_R, HEAD_C + emul(HEAD_R, hq.normalized()), 0.025)
                pos = pos.lerp(on_head, 1.0 - smoothstep(0.0, 0.3, u))
            row.append(len(verts))
            verts.append((pos, "hair", "hairtip", 0.0, [("head", 1.0)], groove))
        grid.append(row)
    for i in range(rows):
        for j in range(cols):
            a, b_ = grid[i][j], grid[i][(j + 1) % cols]
            c, d = grid[i + 1][j], grid[i + 1][(j + 1) % cols]
            faces.extend([(a, b_, d), (a, d, c)])
    clumps.append(first)


def build_common():
    """The fringe and the locks framing the face: every hairdo has them.
    (A bob behind the ears was tried: separate clumps there hung off the
    sheet like hooks.)"""
    # The fringe, parted on her right: big clumps sweep across over her
    # left brow, smaller ones fall down her right temple; the forehead's
    # middle stays bare for the crest. Full and lifted off the brow.
    # The fringe fans out from the parting on her right near the crown:
    # over the top and down the forehead, each clump heading its own way
    # (in order round the fan, so no two cross), the big ones sweeping
    # across over her left brow, smaller ones down her right temple; the
    # forehead's middle stays bare for the crest.
    part = (PART, 82)
    fringe = [
        # (turn on the way down, height there), [(x, y, off) on the face …], width, thick
        ((-58, 48), [(-0.56, 0.9, 0.05), (-0.66, 0.72, 0.035)], 0.2, 0.05),
        ((-40, 56), [(-0.4, 1.0, 0.055), (-0.5, 0.84, 0.045), (-0.55, 0.67, 0.04)], 0.22, 0.05),
        ((-24, 60), [(-0.2, 1.06, 0.06), (-0.27, 0.93, 0.05), (-0.29, 0.79, 0.04)], 0.18, 0.045),
        ((-8, 62), [(0.0, 1.08, 0.06), (0.1, 0.97, 0.05), (0.17, 0.84, 0.04)], 0.16, 0.045),
        ((6, 62), [(0.14, 1.09, 0.065), (0.29, 0.98, 0.055), (0.41, 0.84, 0.045), (0.46, 0.7, 0.04)],
         0.23, 0.05),
        ((22, 60), [(0.32, 1.07, 0.065), (0.48, 0.94, 0.05), (0.57, 0.79, 0.04), (0.61, 0.64, 0.035)],
         0.23, 0.05),
        ((40, 56), [(0.5, 1.0, 0.055), (0.61, 0.86, 0.045), (0.68, 0.7, 0.035)], 0.21, 0.05),
    ]
    for (phi, theta), on_face, w, h in fringe:
        path = [shell(part[0], part[1], 0.0), shell((part[0] + phi) / 2, (part[1] + theta) / 2, 0.06),
                shell(phi, theta, 0.07)] + [face(x, y, off + 0.025 * (y > 0.95)) for x, y, off in on_face]
        clump(path, w * 1.1, h * 1.45, clear=0.045, tip=0.7, point=1.3, root=0.3)

    # Locks framing the face: from the temples down beside the cheeks to
    # the chin, their ends turning in; a second layer behind.
    for side in (1, -1):
        s = side
        clump([shell(56 * s, 44, -0.12), (0.6 * s, 0.96, 0.33), (0.7 * s, 0.68, 0.31),
               (0.73 * s, 0.42, 0.29), (0.69 * s, 0.18, 0.28), (0.6 * s, 0.05, 0.3)],
              0.26, 0.07, clear=0.04, tip=0.65, point=1.3,
              up=lambda p, t, s=s: Vector((s * 0.9, 0.1, 0.45)).normalized())
        clump([shell(70 * s, 40, -0.12), (0.76 * s, 0.9, 0.16), (0.85 * s, 0.6, 0.12),
               (0.86 * s, 0.32, 0.08), (0.8 * s, 0.1, 0.1)],
              0.26, 0.07, clear=0.04, tip=0.65,
              up=lambda p, t, s=s: Vector((s, 0.1, 0.1)).normalized())


def build_ponytail():
    """綁單馬尾: everything swept back from the hairline to a knot high on
    the back of her head, the ponytail hanging from it down to the nape in
    pointed clumps, a thin red ribbon tied in a bow."""
    build_common()
    # Combed back: the main mass, grooved toward the knot.
    scalp(toward=ediv(KNOT - SHELL_C, SHELL_R).normalized())
    # The knot, gathered.
    clump([KNOT + Vector((0, 0.06, 0.06)), KNOT + Vector((0, 0.0, -0.04)),
           KNOT + Vector((0, -0.05, -0.12))], 0.16, 0.12, clear=0.0, root=0.0, point=8.0,
          colors=("hair", "hair"), bones=pony_bones(0.5),
          up=lambda p, t: Vector((0, 0.5, -1)).normalized())
    # The ponytail: clumps out of the knot, over and down the back of her
    # head to the nape, fanning a little, each ending in a point.
    for k, (dx, ln, w) in enumerate(((0.0, 1.0, 0.16), (0.07, 0.92, 0.14), (-0.07, 0.95, 0.14),
                                    (0.14, 0.78, 0.12), (-0.14, 0.8, 0.12), (0.03, 0.66, 0.1))):
        path = [KNOT + Vector((dx * 0.3, 0.02, -0.1)),
                KNOT + Vector((dx * 0.8, -0.08, -0.26)),
                KNOT + Vector((dx * 1.2, -0.3 * ln, -0.42)),
                KNOT + Vector((dx * 1.5, -0.62 * ln, -0.48)),
                KNOT + Vector((dx * 1.8, -0.82 * ln, -0.46 + (0.04 if k % 2 else -0.03)))]
        clump(path, w, lambda t, w=w: w * 0.7, clear=0.02, root=0.0, point=1.4, tip=0.6,
              bones=pony_bones(0.25),
              up=lambda p, t: Vector((0, 0.2, -1)).normalized())
    ribbon(KNOT, "hair_b")


def ribbon(at, bone):
    """A thin red ribbon round the knot, tied in a little bow behind: a
    band, two loops and two ends."""
    rb = lambda t: [(bone, 1.0)]
    axis = Vector((0, -0.25, -1.0)).normalized()
    u = Vector((1, 0, 0))
    v = axis.cross(u).normalized()
    centre = at + Vector((0, -0.02, -0.07))
    band = [centre + (u * math.cos(a) + v * math.sin(a)) * 0.105
            for a in [2 * math.pi * i / 16 for i in range(17)]]
    clump(band, 0.03, 0.014, clear=0.0, root=0.0, colors=("ribbon", "ribbon"), edge=0.0, bones=rb,
          closed=True, up=lambda p, t: (p - centre - axis * (p - centre).dot(axis)).normalized())
    bow = centre + Vector((0, 0.05, -0.1))
    back = lambda p, t: Vector((0, 0.1, -1)).normalized()
    for s in (1, -1):
        loop = [bow, bow + Vector((s * 0.12, 0.09, -0.01)), bow + Vector((s * 0.26, 0.07, 0.02)),
                bow + Vector((s * 0.25, -0.05, 0.02)), bow + Vector((s * 0.1, -0.035, -0.01)), bow]
        clump(loop, 0.03, 0.011, clear=0.0, root=0.0, colors=("ribbon", "ribbon"), edge=0.0, bones=rb,
              closed=True, up=back)
        clump([bow, bow + Vector((s * 0.06, -0.14, -0.02)), bow + Vector((s * 0.12, -0.32, 0.0))],
              0.028, 0.01, clear=0.0, root=0.0, point=4.0, colors=("ribbon", "ribbon"), edge=0.0, bones=rb,
              up=back)
    clump([bow + Vector((0, 0.0, 0.02)), bow + Vector((0, 0.0, -0.03))], 0.035, 0.03, clear=0.0,
          root=0.0, point=8.0, colors=("ribbon", "ribbon"), edge=0.0, bones=rb,
          up=lambda p, t: Vector((0, 1, 0)))


# ---- normals, occlusion, out ------------------------------------------------
def normals():
    acc = [Vector() for _ in verts]
    for a, b, c in faces:
        pa, pb, pc = verts[a][0], verts[b][0], verts[c][0]
        n = (pb - pa).cross(pc - pa)
        for i in (a, b, c):
            acc[i] += n
    return [n.normalized() if n.length > 1e-12 else Vector((0, 1, 0)) for n in acc]


def head_proxy():
    """The head and the body's top, for shading only (not written)."""
    pts, polys = [], []
    n = 24
    for c, r in ((HEAD_C, HEAD_R), (Vector((0, -0.3, 0)), Vector((0.36, 0.45, 0.3)))):
        base = len(pts)
        for i in range(n + 1):
            th = math.pi * i / n
            for j in range(n):
                ph = 2 * math.pi * j / n
                u = Vector((math.sin(th) * math.cos(ph), math.cos(th), math.sin(th) * math.sin(ph)))
                pts.append(c + emul(r, u))
        for i in range(n):
            for j in range(n):
                a = base + i * n + j
                b = base + i * n + (j + 1) % n
                polys.append((a, b, b + n, a + n))
    return pts, polys


def occlusion(nrm):
    hp, hf = head_proxy()
    pts = [v[0] for v in verts] + hp
    polys = [tuple(f) for f in faces] + [tuple(i + len(verts) for i in f) for f in hf]
    tree = BVHTree.FromPolygons(pts, polys, all_triangles=False)
    dirs = []
    golden = math.pi * (3 - math.sqrt(5))
    for i in range(32):
        y = 1 - (i + 0.5) / 32 * 2
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
            loc, _, _, dist = tree.ray_cast(v[0] + n * 0.004, d, AO_REACH)
            if loc is not None:
                hit += c * (1.0 - dist / AO_REACH)
        out.append(max(0.0, 1.0 - AO_STRENGTH * hit / max(total, 1e-6)))
    return out


def write(path):
    nrm = normals()
    ao = occlusion(nrm)
    with open(path, "w") as f:
        for (p, a, b, t, bones, groove), n, o in zip(verts, nrm, ao):
            # Grooves are shaded darker in the hair's own colour (through
            # the occlusion the shader darkens by), never another colour.
            o *= 1.0 - GROOVE_DARK * groove
            bs = " ".join(f"{name} {w:.3f}" for name, w in bones)
            f.write(f"v {p.x:.4f} {p.y:.4f} {p.z:.4f} {n.x:.3f} {n.y:.3f} {n.z:.3f} "
                    f"{a} {b} {t:.3f} {GLOSS} {FUZZ} {o:.3f} {bs}\n")
        for a, b, c in faces:
            f.write(f"f {a} {b} {c}\n")
    print(f"{path}: {len(verts)} vertices, {len(faces)} triangles")


if __name__ == "__main__":
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    style = args[0] if args else "ponytail"
    out = args[1] if len(args) > 1 else f"hair_{style}.mesh"
    {"ponytail": build_ponytail}[style]()
    write(out)
