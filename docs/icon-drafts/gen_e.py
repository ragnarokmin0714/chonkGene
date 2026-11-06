# Direction E: a mochi lazing on a desk edge, plus one variant per species.
import math, sys
out = sys.argv[1]
INK = "#4B3528"; WOOD = "#C9915F"; WOOD_HI = "#DDA872"

SP = {
 "base":     dict(fur="#FFB37A", light="#FFF3E2", limb="paw"),
 "shiba":    dict(fur="#E8955A", light="#FFF3E2", ear="#FFD2B5", limb="paw"),
 "munchkin": dict(fur="#D9B48F", light="#FFF6EA", ear="#F6C3B8", stripe="#B88E66", limb="paw"),
 "fold":     dict(fur="#B8BED6", light="#EEF0F8", stripe="#8F96B8", limb="paw"),
 "owl":      dict(fur="#A9785A", light="#F3E3CF", wing="#8F6246", scallop="#D9BFA3", disc="#F6E7D2", ring="#F2C94C", beak="#F2B24C", limb="wing"),
 "chick":    dict(fur="#FFD95E", light="#FFF0B8", wing="#F5C842", tuft="#F5C842", beak="#F29B38", limb="wing"),
 "duck":     dict(fur="#FFFFFF", light="#F6F2EC", wing="#F0EAE1", beak="#F29B38", limb="wing"),
 "fox":      dict(fur="#FFF9F3", light="#FFFFFF", ear="#F9C9D4", mark="#E3354B", tip="#E04460", rope="#D8323F", bell="#F2C14E", limb="paw"),
}
BODY = "M30 188 C20 188 18 168 26 150 C40 92 84 56 128 56 C172 56 216 92 230 150 C238 168 236 188 226 188 Z"
BODY_EDGE = "M30 188 C20 188 18 168 26 150 C40 92 84 56 128 56 C172 56 216 92 230 150 C238 168 236 188 226 188"

def mirror(path_pts):
    return [(256 - x, y) for x, y in path_pts]

def poly(pts):
    return "M" + " L".join(f"{x} {y}" for x, y in pts) + " Z"

def master(sp):
    c = SP[sp]; fur, light = c["fur"], c["light"]
    o = []; defs = []
    st = f'stroke="{INK}" stroke-width="10" stroke-linejoin="round"'
    # behind the body: tails, ears, tufts
    if sp == "fox":
        for i, a in enumerate([-66, -46, 46, 66]):
            r = math.radians(a); cx = 128 + 86 * math.sin(r); cy = 150 - 86 * math.cos(r)
            t = f'rotate({a} {cx:.1f} {cy:.1f})'
            defs.append(f'<clipPath id="t{i}"><ellipse cx="{cx:.1f}" cy="{cy:.1f}" rx="18" ry="42" transform="{t}"/></clipPath>')
            tx = 128 + 124 * math.sin(r); ty = 150 - 124 * math.cos(r)
            o.append(f'<ellipse cx="{cx:.1f}" cy="{cy:.1f}" rx="18" ry="42" transform="{t}" fill="{fur}"/>')
            o.append(f'<g clip-path="url(#t{i})"><ellipse cx="{tx:.1f}" cy="{ty:.1f}" rx="26" ry="16" transform="rotate({a} {tx:.1f} {ty:.1f})" fill="{c["tip"]}"/></g>')
            o.append(f'<ellipse cx="{cx:.1f}" cy="{cy:.1f}" rx="18" ry="42" transform="{t}" fill="none" {st}/>')
    ears = {
        "shiba": ([(40, 124), (58, 34), (114, 72)], [(58, 104), (66, 58), (98, 80)]),
        "munchkin": ([(44, 118), (56, 30), (110, 70)], [(60, 100), (66, 54), (94, 78)]),
        "fox": ([(42, 120), (50, 14), (112, 68)], [(58, 102), (62, 40), (96, 76)]),
        "owl": ([(72, 82), (50, 36), (94, 70)], None),
    }
    if sp in ears:
        outer, inner = ears[sp]
        for pts in (outer, mirror(outer)):
            o.append(f'<path d="{poly(pts)}" fill="{fur}" {st}/>')
        if inner:
            for pts in (inner, mirror(inner)):
                o.append(f'<path d="{poly(pts)}" fill="{c["ear"]}" stroke="{c["ear"]}" stroke-width="6" stroke-linejoin="round"/>')
    # the desk edge
    o.append(f'<rect x="14" y="180" width="228" height="44" rx="14" fill="{WOOD}" {st}/>')
    o.append(f'<path d="M30 194 L226 194" stroke="{WOOD_HI}" stroke-width="6" stroke-linecap="round"/>')
    # body, with marks clipped to it
    defs.append(f'<clipPath id="b"><path d="{BODY}"/></clipPath>')
    o.append(f'<path d="{BODY}" fill="{fur}"/>')
    marks = []
    if sp == "shiba":
        marks += [f'<ellipse cx="128" cy="176" rx="84" ry="40" fill="{light}"/>',
                  f'<ellipse cx="72" cy="158" rx="34" ry="28" fill="{light}"/>',
                  f'<ellipse cx="184" cy="158" rx="34" ry="28" fill="{light}"/>']
    if sp in ("munchkin", "fold"):
        marks += [f'<path d="M116 62 L118 84 M128 60 L128 86 M140 62 L138 84" stroke="{c["stripe"]}" stroke-width="8" stroke-linecap="round"/>']
    if sp == "fox":
        marks += [f'<path d="M116 80 Q128 96 140 80" fill="none" stroke="{c["mark"]}" stroke-width="7" stroke-linecap="round"/>',
                  f'<ellipse cx="128" cy="72" rx="6" ry="8" fill="{c["mark"]}"/>']
    if sp == "owl":
        marks += [f'<ellipse cx="128" cy="184" rx="70" ry="36" fill="{light}"/>']
    if marks:
        o.append('<g clip-path="url(#b)">' + "".join(marks) + '</g>')
    if sp == "shiba":
        o += [f'<ellipse cx="98" cy="104" rx="10" ry="7" fill="{light}"/>', f'<ellipse cx="158" cy="104" rx="10" ry="7" fill="{light}"/>']
    o.append(f'<path d="{BODY_EDGE}" fill="none" {st}/>')
    # folded ears and tufts sit on top of the head
    if sp == "fold":
        for d in ("M58 98 C56 76 70 64 92 66 C90 76 86 86 80 92 Z", "M198 98 C200 76 186 64 164 66 C166 76 170 86 176 92 Z"):
            o.append(f'<path d="{d}" fill="{c["stripe"]}" {st}/>')
    if sp == "chick":
        o.append(f'<path d="M124 58 C116 40 122 26 134 24 M132 58 C136 42 148 38 158 42" fill="none" stroke="{INK}" stroke-width="16" stroke-linecap="round"/>')
        o.append(f'<path d="M124 58 C116 40 122 26 134 24 M132 58 C136 42 148 38 158 42" fill="none" stroke="{c["tuft"]}" stroke-width="6" stroke-linecap="round"/>')
    if sp == "duck":
        o.append(f'<path d="M128 58 C124 42 132 32 144 34" fill="none" stroke="{INK}" stroke-width="16" stroke-linecap="round"/>')
        o.append(f'<path d="M128 58 C124 42 132 32 144 34" fill="none" stroke="{c["fur"]}" stroke-width="6" stroke-linecap="round"/>')
    # belly spilling over the edge
    o.append(f'<path d="M70 186 C72 208 98 216 128 216 C158 216 184 208 186 186" fill="{light}" {st}/>')
    if sp == "owl":
        o.append(f'<path d="M100 198 q8 8 16 0 M124 204 q8 8 16 0 M148 198 q8 8 16 0" fill="none" stroke="{c["scallop"]}" stroke-width="5" stroke-linecap="round"/>')
    # paws or wings hanging over the edge
    if c["limb"] == "paw":
        for x in (48, 208):
            o.append(f'<ellipse cx="{x}" cy="204" rx="18" ry="20" fill="{fur}" {st}/>')
            o.append(f'<path d="M{x-6} 214 L{x-6} 220 M{x+6} 214 L{x+6} 220" stroke="{INK}" stroke-width="5" stroke-linecap="round"/>')
    else:
        for x, a in ((44, 18), (212, -18)):
            o.append(f'<ellipse cx="{x}" cy="198" rx="16" ry="26" transform="rotate({a} {x} 198)" fill="{c["wing"]}" {st}/>')
    # face
    ey = 128
    if sp == "owl":
        for x in (98, 158):
            o.append(f'<circle cx="{x}" cy="{ey}" r="28" fill="{c["disc"]}"/>')
            o.append(f'<circle cx="{x}" cy="{ey}" r="14" fill="{c["ring"]}"/>')
    for x in (98, 158):
        o.append(f'<ellipse cx="{x}" cy="{ey}" rx="10" ry="12" fill="{INK}"/>')
        o.append(f'<circle cx="{x+3.5}" cy="{ey-4.5}" r="4" fill="#fff"/>')
    blush = "#FF8C94"
    for x in (68, 188):
        o.append(f'<ellipse cx="{x}" cy="152" rx="16" ry="9" fill="{blush}" opacity=".7"/>')
    if sp in ("owl", "chick"):
        o.append(f'<path d="M116 144 L140 144 L128 162 Z" fill="{c["beak"]}" stroke="{INK}" stroke-width="6" stroke-linejoin="round"/>')
    elif sp == "duck":
        o.append(f'<path d="M104 146 C104 138 152 138 152 146 C152 160 104 160 104 146 Z" fill="{c["beak"]}" stroke="{INK}" stroke-width="6" stroke-linejoin="round"/>')
    else:
        if sp in ("shiba", "fox", "munchkin", "fold"):
            o.append(f'<ellipse cx="128" cy="146" rx="9" ry="6.5" fill="{INK}"/>')
            o.append(f'<path d="M114 156 Q121 165 128 157 Q135 165 142 156" fill="none" stroke="{INK}" stroke-width="5.5" stroke-linecap="round" stroke-linejoin="round"/>')
        else:
            o.append(f'<path d="M114 148 Q121 157 128 149 Q135 157 142 148" fill="none" stroke="{INK}" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>')
    if sp == "fox":
        o.append(f'<path d="M90 176 Q128 192 166 176" fill="none" stroke="{c["rope"]}" stroke-width="7" stroke-linecap="round"/>')
        o.append(f'<circle cx="128" cy="190" r="13" fill="{c["bell"]}" stroke="{INK}" stroke-width="6"/>')
        o.append(f'<path d="M122 192 L134 192" stroke="{INK}" stroke-width="4" stroke-linecap="round"/>')
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><defs>' + "".join(defs) +
            '</defs><g transform="translate(0 -6)">' + "\n".join(o) + '</g></svg>')

def px(x, y, w, h, col):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{col}"/>'

def small(sp):
    c = SP[sp]; fur, light = c["fur"], c["light"]
    st = f'stroke="{INK}" stroke-width="1" stroke-linejoin="round"'
    o = []
    if sp == "fox":
        o.append(f'<path d="M1.5 9.5 L0.5 5.5 L2.5 6.5 Z M14.5 9.5 L15.5 5.5 L13.5 6.5 Z" fill="{c["tip"]}" {st}/>')
    ears = {"shiba": "M2 8 L2.5 2.5 L6.5 5 Z", "munchkin": "M2 8 L2.5 2 L6.5 5 Z",
            "fox": "M2.5 7.5 L2.5 1 L6.5 4.5 Z", "owl": "M3 6.5 L2.5 2.5 L6 4.5 Z"}
    if sp in ears:
        d = ears[sp]
        m = d.replace("M2 8", "M14 8").replace("L2.5 2.5", "L13.5 2.5").replace("L6.5 5", "L9.5 5") \
             .replace("L2.5 2 ", "L13.5 2 ").replace("M2.5 7.5", "M13.5 7.5").replace("L2.5 1", "L13.5 1") \
             .replace("L6.5 4.5", "L9.5 4.5").replace("M3 6.5", "M13 6.5").replace("L6 4.5", "L10 4.5")
        o.append(f'<path d="{d} {m}" fill="{fur}" {st}/>')
    o.append(f'<rect x="0.5" y="11.5" width="15" height="4" rx="1.5" fill="{WOOD}" {st}/>')
    o.append(f'<path d="M1.5 12 C1.5 7 4.5 4.5 8 4.5 C11.5 4.5 14.5 7 14.5 12 Z" fill="{fur}"/>')
    if sp == "shiba":
        o.append(px(3, 9, 10, 3, light))
    if sp == "owl":
        o.append(px(4, 10, 8, 2, light))
    o.append(f'<path d="M1.5 12 C1.5 7 4.5 4.5 8 4.5 C11.5 4.5 14.5 7 14.5 12" fill="none" {st}/>')
    if sp == "fold":
        o.append(f'<path d="M3 7.5 C3 5.5 4.5 4.5 6.5 4.8 L5.5 7 Z M13 7.5 C13 5.5 11.5 4.5 9.5 4.8 L10.5 7 Z" fill="{c["stripe"]}" {st}/>')
    if sp in ("munchkin", "fold"):
        o.append(px(7, 5, 2, 1, c["stripe"]))
    if sp == "fox":
        o.append(px(7, 5, 2, 1, c["mark"]))
    if sp == "chick":
        o.append(f'<path d="M8 4.5 L7.5 1.5 L10 2.5 Z" fill="{c["tuft"]}" {st}/>')
    if sp == "duck":
        o.append(f'<path d="M8 4.5 L8.5 2 L10.5 2.5" fill="none" {st}/>')
    o.append(f'<path d="M5 11.5 L11 11.5 L11 12.5 Q11 14.5 8 14.5 Q5 14.5 5 12.5 Z" fill="{light}" {st}/>')
    if sp == "owl":
        o += [px(4, 7, 3, 3, c["disc"]), px(9, 7, 3, 3, c["disc"])]
        o += [px(5, 8, 1, 1, INK), px(10, 8, 1, 1, INK)]
    else:
        o += [px(5, 7, 1, 2, INK), px(10, 7, 1, 2, INK)]
    o += [px(3, 9, 1, 1, "#FF8C94"), px(12, 9, 1, 1, "#FF8C94")]
    if sp in ("owl", "chick"):
        o.append(px(7, 9, 2, 1, c["beak"]))
    elif sp == "duck":
        o.append(px(6, 9, 4, 2, c["beak"]))
    elif sp in ("shiba", "munchkin", "fold", "fox"):
        o.append(px(7, 9, 2, 1, INK))
    if sp == "fox":
        o.append(px(7, 11, 2, 2, c["bell"]))
    return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">' + "\n".join(o) + '</svg>'

for sp in SP:
    open(f"{out}/e-{sp}.svg", "w").write(master(sp))
    open(f"{out}/e-{sp}16.svg", "w").write(small(sp))
