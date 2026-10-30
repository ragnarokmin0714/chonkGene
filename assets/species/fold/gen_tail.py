# Prints the Fold's tail for quad.ron: python3 gen_tail.py | python3
# ../splice.py quad.ron
#
# A thick tail wrapped round its left side from the rump toward the front,
# the end a shade darker.
import math
import sys

sys.path.insert(0, "..")
from tailgen import emit

N = 8
# Out of the rump, round the side (x about 0.72), to just behind the front
# leg; a quarter ring seen from above, hugging the body.
points = []
for k in range(N + 1):
    t = math.radians(100 * k / N)
    points.append([0.0 + 0.78 * math.sin(t) * 0.9, -0.62, -0.92 + 1.0 * (1 - math.cos(t)) * 1.05])
names = ["tail"] + [f"tail_{k}" for k in range(2, N)] + ["tail_tip"]
radii = [0.13] * N + [0.12]


def mark(k, h):
    if k >= N - 2:
        return f'marks: [(at: (0, {h / 2:.3f}, 0), radii: (0.3, {h:.3f}, 0.3), color: "stripe", soft: 0.4)]'
    if k in (2, 4):
        return f'marks: [(at: (0, {h / 2:.3f}, 0), radii: (0.3, 0.06, 0.3), color: "stripe", soft: 0.5)]'
    return ""


emit(points, radii, names, blend=0.1, mark=mark)
