# Prints the Munchkin's tail for quad.ron: python3 gen_tail.py | python3
# ../splice.py quad.ron
#
# It stands up behind and curls forward over its back at the tip, a stripe
# band or two on the way, the tip dark.
import sys

sys.path.insert(0, "..")
from tailgen import emit, f, turn_path

N = 8
points = turn_path([0.0, -0.29, -0.98], heading=-20, bend=110, length=1.15, n=N)
names = ["tail"] + [f"tail_{k}" for k in range(2, N)] + ["tail_tip"]
radii = [0.12] * N + [0.1]


def mark(k, h):
    if k in (2, 4):
        return f'marks: [(at: (0, {h / 2:.3f}, 0), radii: (0.3, 0.06, 0.3), color: "stripe", soft: 0.5)]'
    if k >= N - 2:
        return f'marks: [(at: (0, {h / 2:.3f}, 0), radii: (0.3, {h:.3f}, 0.3), color: "stripe", soft: 0.4)]'
    return ""


emit(points, radii, names, blend=0.1, mark=mark)
