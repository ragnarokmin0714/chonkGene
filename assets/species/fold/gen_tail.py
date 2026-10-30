# Prints the Fold's tail for quad.ron: python3 gen_tail.py | python3
# ../splice.py quad.ron
#
# A thick, relaxed cat's tail out of the middle of the rump: back and a
# little down, then curving up at the end, a stripe band or two on the way
# and the tip a shade darker.
import sys

sys.path.insert(0, "..")
from tailgen import emit, turn_path

N = 8
points = turn_path([0.0, -0.22, -0.98], heading=-120, bend=95, length=1.05, n=N)
names = ["tail"] + [f"tail_{k}" for k in range(2, N)] + ["tail_tip"]
radii = [0.15] * N + [0.13]


def mark(k, h):
    if k >= N - 2:
        return f'marks: [(at: (0, {h / 2:.3f}, 0), radii: (0.3, {h:.3f}, 0.3), color: "stripe", soft: 0.4)]'
    if k in (2, 4):
        return f'marks: [(at: (0, {h / 2:.3f}, 0), radii: (0.3, 0.06, 0.3), color: "stripe", soft: 0.5)]'
    return ""


emit(points, radii, names, blend=0.1, mark=mark)
