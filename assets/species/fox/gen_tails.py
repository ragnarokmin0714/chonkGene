# Prints the fox's nine tails for quad.ron: `python3 gen_tails.py`, then
# paste the PIVOTS, PARENTS and PARTS sections over the tail entries there.
# Nine tails as a bouquet: each leaves the rump along a cone opening up and
# back, curves outward and lifts at the tip; the last segment is a cone so
# the tip comes to a point.
import math
def norm(v):
    l=math.sqrt(sum(c*c for c in v)); return [c/l for c in v]
def add(a,b,k=1.0): return [x+y*k for x,y in zip(a,b)]
def cross(a,b): return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
def f(v): return "(" + ", ".join((f"{c:.3f}".rstrip('0').rstrip('.') if abs(c)>=5e-4 else "0") for c in v) + ")"
base=[0.0,-0.02,-0.95]
axis=norm([0.0,0.9,-0.45])
u=norm(cross(axis,[1,0,0]))   # in the y-z plane, perpendicular to axis
v=[1.0,0.0,0.0]
HALF=math.radians(50)
LEN=[0.34,0.4,0.38]; R=[0.1,0.175]
pivots=[]; parents=[]; parts=[]
for i in range(9):
    phi=math.radians(90+i*40)   # i = 0 leans out to the pet's left (+x)
    out=add([c*math.cos(phi) for c in u],[c*math.sin(phi) for c in v])
    d=norm(add([c*math.cos(HALF) for c in axis],[c*math.sin(HALF) for c in out]))
    p=base[:]
    names=[f"tail{i+1}", f"tail{i+1}_mid", f"tail{i+1}_tip"]
    for k in range(3):
        if k>0:
            d=norm(add(add(d,out,0.25),[0,1,0],0.15))
        to=[x*LEN[k] for x in d]
        pivots.append(f'        "{names[k]}": {f(p)},')
        parents.append(f'        "{names[k]}": "{"hips" if k==0 else names[k-1]}",')
        if k<2:
            parts.append(f'        (name: "{names[k]}", bone: "{names[k]}", shape: Capsule(to: {f(to)}, r: {R[k]}), at: {f(p)},\n            blend: 0.07, color: "fur", zone: "bottom"),')
        else:
            # A cone along +y, turned onto d: roll about z, then pitch about x.
            g=-math.degrees(math.asin(max(-1,min(1,d[0]))))
            a=math.degrees(math.atan2(d[2],d[1]))
            parts.append(f'        (name: "{names[k]}", bone: "{names[k]}", shape: Cone(r1: 0.175, r2: 0.035, h: {LEN[k]}), at: {f(p)},\n            rot: {f([a,0,g])}, blend: 0.07, color: "fur", zone: "bottom",\n            marks: [(at: (0, {LEN[k]}, 0), radii: (0.22, 0.24, 0.22), color: "light", soft: 0.5)]),')
        p=add(p,to)
print("PIVOTS"); print("\n".join(pivots)); print("PARENTS"); print("\n".join(parents)); print("PARTS"); print("\n".join(parts))
