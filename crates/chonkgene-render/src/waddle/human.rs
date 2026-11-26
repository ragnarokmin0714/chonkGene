//! The fox's human form (PLAN §8): what her body does on top of
//! the two-legged walk. Her arms have elbows, and are moved by where her
//! hands go (two-bone reaching, [`Arm::reach`]): hanging at her sides,
//! swinging as she walks, in her lap sitting, over her mouth for a yawn,
//! at her head for a shy scratch, round a snack, up for a stretch or a
//! cheer. Her gestures, little fidgets and answers to a touch are her own,
//! not a bird's (Roger, 2026-11-24: 「動作有點生疏」; her straight arms stuck
//! out could do nothing but swing, and a bird's moves did nothing at all
//! or lifted a leg to her head).

use std::f32::consts::{PI, TAU};

use chonkgene_core::anim::{Act, DanceMove, Hug, dance_at, gesture_secs};
use chonkgene_rig::math::{M3, V3, smoothstep, v3};
use chonkgene_rig::{FLOOR, Model, sdf::Shape};

use super::{CURL_HEAD, CURL_LEGS, CURL_ROLL, DOZE_SLUMP, SEAT_FLARE, SEAT_LEAN, SEAT_LEGS};
use super::{HAIR_BOB, HAIR_HANG, HAIR_PACE, HAIR_TRAIL, HAIR_TURN};
use super::{SEAT_SPLAY, SIT_DROP, SIT_LEGS, Shake, Waddle};
use crate::fidget::{Kind, Out};
use crate::skin::{
    FLOW_HZ, Gesture, Joint, Levels, SWISH_HZ, SWISH_REST, SWISH_WALK, Skeleton, SleepPose, Wave,
    aim, bite, peck,
};

/// The middle of her palm lies this far past the elbow, in upper arm
/// lengths along the arm: the hand carries on along the sleeve's slant
/// (measured off figure_blend.py's `hands`).
const PALM_PAST: f32 = 1.6;

/// One of her arms: its two bones, where they turn (shoulder and elbow),
/// the middle of the palm at rest, and its side (+1 her left).
#[derive(Clone, Debug)]
pub(crate) struct Arm {
    pub upper: usize,
    pub fore: usize,
    shoulder: V3,
    elbow: V3,
    pub palm: V3,
    sign: f32,
}

impl Arm {
    fn of(sk: &Skeleton, side: usize) -> Option<Arm> {
        let (u, f) = if side == 0 {
            ("arm_l", "fore_l")
        } else {
            ("arm_r", "fore_r")
        };
        let (upper, fore) = (sk.bone(u)?, sk.bone(f)?);
        let (shoulder, elbow) = (sk.pivots[upper], sk.pivots[fore]);
        Some(Arm {
            upper,
            fore,
            shoulder,
            elbow,
            palm: elbow + (elbow - shoulder) * PALM_PAST,
            sign: if side == 0 { 1.0 } else { -1.0 },
        })
    }

    /// How far the palm reaches from the shoulder, the arm straight.
    fn length(&self) -> f32 {
        (self.elbow - self.shoulder).length() * (1.0 + PALM_PAST)
    }

    /// The upper arm's and the forearm's turns (as [`Joint::rot`]) that
    /// put the palm at `to` (in the body's frame at rest), the elbow out
    /// toward `pole`; out of reach, the arm points straight at it.
    pub fn reach(&self, to: V3, pole: V3) -> (V3, V3) {
        let (s, e, h) = (self.shoulder, self.elbow, self.palm);
        let (a, b) = ((e - s).length(), (h - e).length());
        let off = to - s;
        let d = off
            .length()
            .clamp((a - b).abs() + 1e-3, a + b - 1e-3)
            .max(1e-3);
        let u = if off.length() > 1e-5 {
            off.normalize()
        } else {
            v3(0.0, -1.0, 0.0)
        };
        let cos = ((a * a + d * d - b * b) / (2.0 * a * d)).clamp(-1.0, 1.0);
        let sin = (1.0 - cos * cos).sqrt();
        let side = pole - u * pole.dot(u);
        let w = if side.length() > 1e-4 {
            side.normalize()
        } else {
            v3(0.0, 0.0, -1.0)
        };
        let elbow = s + u * (a * cos) + w * (a * sin);
        let palm = s + u * d;
        let up = aim(e - s, elbow - s);
        let ru = M3::euler(up);
        let turn = M3::euler(aim(ru.apply(h - e), palm - elbow));
        (up, euler_of(&ru.transpose().mul(turn).mul(ru)))
    }
}

/// Euler angles (degrees, as [`M3::euler`]) of a turn.
fn euler_of(m: &M3) -> V3 {
    let x = (-m.r[1].z).clamp(-1.0, 1.0).asin();
    let z = m.r[1].x.atan2(m.r[1].y);
    let y = m.r[0].z.atan2(m.r[2].z);
    v3(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

/// Her arms (none on anyone else), for the renderer to find her hands:
/// each forearm's bone and the middle of its palm at rest, her left first.
pub(crate) fn hands(sk: &Skeleton) -> Option<[(usize, V3); 2]> {
    let (l, r) = (Arm::of(sk, 0)?, Arm::of(sk, 1)?);
    Some([(l.fore, l.palm), (r.fore, r.palm)])
}

/// Where a hand goes, worked out each frame: given for her left hand (the
/// right's is its mirror image), with the way the elbow points.
#[derive(Clone, Copy, Debug)]
pub(super) struct Place {
    on: On,
    at: V3,
    pole: V3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum On {
    /// From the shoulder, in arm lengths (so it fits her whatever her
    /// proportions), in the body's frame.
    Arm,
    /// On her head (moving with it): out along `at` from the head's middle,
    /// this many times its surface's distance.
    Head(f32),
    /// From the front of her mouth: as it is at rest (body's frame), or
    /// as it is this frame (moving with her head).
    Mouth { follow: bool },
    /// Over the floor: `at.y` above it, x and z under her.
    Floor,
}

const fn arm(x: f32, y: f32, z: f32) -> Place {
    Place {
        on: On::Arm,
        at: v3(x, y, z),
        pole: v3(0.5, -0.4, -0.8),
    }
}

const fn head(x: f32, y: f32, z: f32, out: f32) -> Place {
    Place {
        on: On::Head(out),
        at: v3(x, y, z),
        pole: v3(0.9, -0.6, 0.0),
    }
}

impl Place {
    const fn pole(self, x: f32, y: f32, z: f32) -> Place {
        Place {
            pole: v3(x, y, z),
            ..self
        }
    }

    fn plus(self, d: V3) -> Place {
        Place {
            at: self.at + d,
            ..self
        }
    }
}

// Her hands' places (her left; arm lengths from the shoulder: x out, y up,
// z forward).
/// Hanging at her sides, a little forward, the elbows soft.
const HANG: Place = arm(0.16, -0.92, 0.22).pole(0.4, -0.2, -1.0);
/// In her lap, sitting.
const LAP: Place = arm(-0.15, -0.65, 0.52).pole(0.7, -0.4, -0.5);
/// Clasped behind her back.
const BEHIND: Place = arm(-0.21, -0.69, -0.36).pole(0.7, -0.4, -0.6);
/// Together in front of her tummy.
const CLASP: Place = arm(-0.28, -0.43, 0.62).pole(0.8, -0.8, -0.2);
/// Together under her chin.
const CHIN: Place = arm(-0.27, 0.2, 0.6).pole(0.8, -1.0, 0.0);
/// Tapping away in front of her, as at a keyboard.
const TYPE: Place = arm(-0.19, -0.49, 0.7).pole(0.8, -0.6, -0.3);
/// Round someone in front of her.
const HUG_ROUND: Place = arm(-0.05, -0.23, 0.89).pole(0.9, -0.3, 0.0);
/// Holding something to her chest (a thing she carries, your thumb).
const HOLD: Place = arm(-0.24, -0.23, 0.62).pole(0.8, -0.7, -0.2);
/// On her hips, elbows out.
const HIPS: Place = arm(0.16, -0.62, 0.13).pole(1.0, 0.1, -0.5);
// Her head is as big as the rest of her and her arms are short: arms up
// go up and out in a V, clear of it (straight up, her hands were inside it,
// her sleeve across her face).
/// Up beside her head, waving.
const WAVE: Place = arm(0.68, 0.58, 0.38).pole(1.0, -0.5, -0.3);
/// Up high (out of reach: the arm points at it).
const UP: Place = arm(0.75, 0.75, 0.06).pole(1.0, 0.0, 0.0);
/// Up and out, cheering.
const CHEER: Place = arm(0.78, 0.62, 0.25).pole(1.0, -0.2, -0.3);
/// Out to the side.
const OUT: Place = arm(0.95, -0.15, 0.1).pole(0.0, -1.0, -0.3);
/// Held out to you.
const OFFER: Place = arm(-0.05, -0.12, 1.0).pole(0.5, -1.0, 0.0);
/// Folded across her chest (the left hand by the right elbow).
const CROSSED: Place = arm(-0.61, -0.33, 0.5).pole(0.6, -1.0, 0.0);
/// On her tummy, laughing.
const TUMMY: Place = arm(-0.16, -0.47, 0.6).pole(0.8, -0.6, -0.2);
/// Hanging limp, fed up.
const LIMP: Place = arm(0.3, -0.95, 0.25).pole(0.3, 0.0, -1.0);
/// Up to her chest, startled.
const STARTLED: Place = arm(-0.25, -0.15, 0.55).pole(0.8, -1.0, 0.0);
/// Resting on her knees, bending over.
const KNEES: Place = arm(-0.05, -0.85, 0.45).pole(0.6, -0.6, -0.5);
/// Over the floor in front of her, kneeling.
const FLOOR_FRONT: Place = Place {
    on: On::Floor,
    at: v3(0.1, 0.05, 0.42),
    pole: v3(0.6, -0.8, -0.3),
};
/// On her cheek.
const CHEEK: Place = head(0.75, -0.45, 0.55, 1.08).pole(0.8, -1.0, 0.0);
/// At the side of her head toward the back, the elbow up, scratching it
/// shyly (right behind it, the big head hid her hand).
const BACK_OF_HEAD: Place = head(0.9, 0.0, -0.35, 1.2).pole(1.0, 0.5, 0.0);
/// Beside her face, and back over her ear, tucking her hair behind it.
const BY_FACE: Place = head(0.85, -0.25, 0.45, 1.15);
const OVER_EAR: Place = head(0.95, 0.15, -0.1, 1.2);
/// Her side lock, high and low, combed through her fingers (her face is the
/// lower half of her head: beside her eyes, down past her chin).
const LOCK_HIGH: Place = head(0.85, -0.1, 0.4, 1.2);
const LOCK_LOW: Place = head(0.8, -0.6, 0.35, 1.25);
/// Under her cheek, a pillow, curled up asleep.
const PILLOW: Place = head(0.6, -0.75, 0.3, 1.05).pole(1.0, -0.2, 0.3);
/// Over her front, curled up asleep (the upper arm).
const OVER_FRONT: Place = arm(-0.45, -0.35, 0.6).pole(0.6, -0.8, -0.2);
/// Over her mouth, yawning; round a snack in front of it.
const OVER_MOUTH: Place = Place {
    on: On::Mouth { follow: true },
    at: v3(0.03, 0.0, 0.06),
    pole: v3(0.6, -1.0, 0.0),
};
const SNACK: Place = Place {
    on: On::Mouth { follow: false },
    at: v3(0.07, -0.1, 0.12),
    pole: v3(0.7, -1.0, -0.1),
};

/// Her arms and the places they go to, built from her skeleton and model.
pub(crate) struct Arms {
    pub side: [Arm; 2],
    head: usize,
    /// Where the head and the body turn.
    head_pivot: V3,
    body_pivot: V3,
    head_c: V3,
    head_r: V3,
    /// The front of her mouth, at rest.
    mouth: V3,
    reach: f32,
}

impl Arms {
    pub fn new(sk: &Skeleton, model: &Model) -> Option<Arms> {
        let side = [Arm::of(sk, 0)?, Arm::of(sk, 1)?];
        let ellipsoid = |name: &str| {
            let p = &model.parts[model.part(name)?];
            match p.shape {
                Shape::Ellipsoid(r) => Some((p.at, r)),
                _ => None,
            }
        };
        let (head_c, head_r) = ellipsoid("head")?;
        let mouth = ellipsoid("mouth")
            .map_or(head_c + v3(0.0, -0.3 * head_r.y, head_r.z), |(at, r)| {
                at + v3(0.0, 0.0, r.z)
            });
        let reach = side[0].length();
        let (head, body) = (sk.bone("head")?, sk.bone("body")?);
        Some(Arms {
            side,
            head,
            head_pivot: sk.pivots[head],
            body_pivot: sk.pivots[body],
            head_c,
            head_r,
            mouth,
            reach,
        })
    }

    /// Where `place` is for hand `k` this frame (body's frame), and the
    /// way its elbow points.
    fn resolve(&self, place: Place, k: usize, fr: &Frame) -> (V3, V3) {
        let arm = &self.side[k];
        let s = arm.sign;
        let at = v3(s * place.at.x, place.at.y, place.at.z);
        let pole = v3(s * place.pole.x, place.pole.y, place.pole.z);
        let to = match place.on {
            On::Arm => arm.shoulder + at * self.reach,
            On::Head(out) => {
                let dir = at;
                let t = 1.0 / dir.div_elem(self.head_r).length().max(1e-4);
                fr.head(self.head_c + dir * (t * out))
            }
            On::Mouth { follow: false } => self.mouth + at,
            On::Mouth { follow: true } => fr.head(self.mouth + at),
            On::Floor => fr.body_from_root(v3(at.x, fr.floor + at.y, at.z)),
        };
        (to, pole)
    }
}

/// This frame's head and body, for places on the head and over the floor.
struct Frame {
    head: (M3, V3),
    body: (M3, V3),
    /// The floor's height in the root's frame.
    floor: f32,
}

impl Frame {
    fn new(j: &[Joint], arms: &Arms, body: usize) -> Frame {
        let local = |jt: &Joint, pivot: V3| {
            let m = M3::euler(jt.rot);
            (m, pivot + jt.shift - m.apply(pivot))
        };
        Frame {
            head: local(&j[arms.head], arms.head_pivot),
            body: local(&j[body], arms.body_pivot),
            floor: FLOOR - j[0].shift.y,
        }
    }

    fn head(&self, p: V3) -> V3 {
        self.head.0.apply(p) + self.head.1
    }

    fn body_from_root(&self, p: V3) -> V3 {
        self.body.0.transpose().apply(p - self.body.1)
    }
}

/// Several hand places blended in turn: each hand's place and elbow, and
/// what comes over them.
struct Hands {
    want: [(V3, V3); 2],
}

impl Hands {
    fn put(&mut self, arms: &Arms, fr: &Frame, k: usize, place: Place, w: f32) {
        if w <= 0.0 {
            return;
        }
        let (to, pole) = arms.resolve(place, k, fr);
        let w = w.min(1.0);
        self.want[k] = (self.want[k].0.lerp(to, w), self.want[k].1.lerp(pole, w));
    }

    fn both(&mut self, arms: &Arms, fr: &Frame, place: Place, w: f32) {
        for k in 0..2 {
            self.put(arms, fr, k, place, w);
        }
    }
}

/// A layer of hand places from one source (a feeling, a fidget, a
/// gesture): each hand's place and how much of it.
type Layer = [Option<(Place, f32)>; 2];

/// Her side (0 left, 1 right) toward you: facing right, her right.
fn near(facing: f32) -> usize {
    usize::from(facing > 0.0)
}

/// 0 … 1 … 0 as `x` goes 0 … 1: half a sine wave.
fn bump(x: f32) -> f32 {
    (PI * x.clamp(0.0, 1.0)).sin()
}

impl Waddle {
    /// What her human form adds on top of the pose:
    /// sitting and sleeping her own ways, her gestures, fidgets and
    /// answers to a touch, her arms put where her hands go, nine tails
    /// flowing, hair, ears and the bell swinging on springs. Nothing on a
    /// bird.
    pub(super) fn human_form(&mut self, j: &mut [Joint], k: &Shake, lv: &Levels, f: &Out, dt: f32) {
        let Some(arms) = self.arms.take() else {
            return;
        };
        self.human_with(&arms, j, k, lv, f, dt);
        self.arms = Some(arms);
    }

    fn human_with(
        &mut self,
        arms: &Arms,
        j: &mut [Joint],
        k: &Shake,
        lv: &Levels,
        f: &Out,
        dt: f32,
    ) {
        let s = k.s;
        let yaw_acc = (self.yaw.v - self.yaw_v) / dt;
        self.yaw_v = self.yaw.v;
        // A gesture as a bird's walk would have it: standing ones only
        // standing, sitting ones sat down for.
        let gesture = self
            .gesture
            .filter(|g| s < 0.5 && (k.sit < 0.5 || g.act.seated() || g.act == Act::Hug));
        let kneel = gesture.map_or(0.0, |g| match g.act {
            Act::Dig | Act::Pick => g.e,
            Act::Sniff | Act::Peck => 0.7 * g.e,
            _ => 0.0,
        });
        let kneel = self.kneel.step(kneel, 0.0, 3.0, 1.0, dt).clamp(0.0, 1.0);
        // Sitting, she sits on the floor on her heels (a bird's legs fold
        // under it); kneeling down to something on the floor, the same;
        // asleep, she dozes off sitting up or curls up on her side.
        let curl_goal = if self.sleep_pose == SleepPose::Curl {
            k.lie
        } else {
            0.0
        };
        let curl = self.curl.step(curl_goal, 0.0, 1.0, 1.0, dt).clamp(0.0, 1.0);
        let seat = k.sit.max(kneel) * (1.0 - curl);
        let doze = k.lie * (1.0 - curl);
        // (Taking back the bird's sitting first.)
        j[0].shift.y += SIT_DROP * k.sit - self.seat_drop * seat;
        j[self.body].rot.x += -SEAT_LEAN * seat + DOZE_SLUMP * doze;
        if let Some(h) = self.hips {
            j[h].swell += SEAT_FLARE * (seat + doze);
        }
        for (side, &leg) in self.legs.iter().enumerate() {
            let sign = if side == 0 { 1.0 } else { -1.0 };
            // The upper leg, curled on her side, is the one away from the
            // floor: her right lying on her left.
            let upper = (side == 1) == (k.facing > 0.0);
            let curled = CURL_LEGS[usize::from(!upper)];
            j[leg].rot.x += SIT_LEGS * k.sit + SEAT_LEGS * seat - curled * curl;
            j[leg].rot.z += sign * SEAT_SPLAY * seat;
        }
        // Rolled onto her side (her left facing right), laid on the floor
        // by the renderer (see `ground`), her head laid over on her arm.
        j[0].rot.z -= k.facing * CURL_ROLL * curl;
        j[self.head].rot.z -= k.facing * CURL_HEAD * curl;
        self.ground = self.ground.max(curl);

        // Her own moves, each with its hands' places.
        let moods = self.her_moods(arms, j, lv, k.facing);
        let fidget = self.her_fidget(j, f, k, seat);
        let doing = gesture.map(|g| self.her_gesture(j, g, k.facing, dt));
        let hold = self
            .hold
            .step(f32::from(u8::from(self.holding)), 0.0, 2.5, 1.0, dt)
            .clamp(0.0, 1.0);

        // Her arms: where her hands go, in layers, then reached for.
        let fr = Frame::new(j, arms, self.body);
        let mut hands = Hands {
            want: [0, 1].map(|side| arms.resolve(HANG, side, &fr)),
        };
        for side in 0..2 {
            // Swinging with the other side's leg as she walks (smoothed: a
            // planted foot sweeps back evenly, an arm swings), out a little
            // at each step; stirring with her breath.
            let sw = self.arm[side].step(s * k.strides[1 - side], 0.0, 5.0, 0.7, dt);
            let out = 0.05 * s * (2.0 * TAU * k.ph).sin().abs();
            let walk = HANG.plus(v3(out, 0.1 * sw.max(0.0) + 0.012 * self.breath, 0.3 * sw));
            hands.put(arms, &fr, side, walk, 1.0);
            hands.put(arms, &fr, side, LAP, seat + doze);
            let lower = (side == 0) == (k.facing > 0.0);
            hands.put(
                arms,
                &fr,
                side,
                if lower { PILLOW } else { OVER_FRONT },
                curl,
            );
        }
        for layer in [Some(moods), Some(fidget), doing].into_iter().flatten() {
            for (side, l) in layer.into_iter().enumerate() {
                if let Some((place, w)) = l {
                    hands.put(arms, &fr, side, place, w);
                }
            }
        }
        hands.both(arms, &fr, HOLD, hold * (1.0 - curl));
        for (side, arm) in arms.side.iter().enumerate() {
            let (to, pole) = hands.want[side];
            let (up, fore) = arm.reach(to, pole);
            j[arm.upper].rot = up;
            j[arm.fore].rot = fore;
        }

        // Tails: the four-legged fox's wave, as slow; asleep they hardly
        // stir, laughing they thrash, melted they lie limp; pleased, they
        // wag.
        if !self.tails.is_empty() {
            let wag = f.get(Kind::Wag).map_or(0.0, |w| w.0).max(0.85 * f.lively);
            let hz = (SWISH_HZ.0 + (SWISH_HZ.1 - SWISH_HZ.0) * s) * FLOW_HZ;
            self.swish = (self.swish + dt * hz * (1.0 + 2.5 * wag)).fract();
            let wave = Wave {
                amp: (SWISH_REST + (SWISH_WALK - SWISH_REST) * s)
                    * (1.0 - 0.75 * k.lie)
                    * (1.0 + 0.9 * lv.laugh)
                    * (1.0 - 0.8 * lv.melt)
                    * (1.0 + 1.6 * wag),
                swish: self.swish,
                s,
                ph: k.ph,
                pace_acc: k.pace_acc,
                yaw_acc,
                flick: lv.flick,
                flow: true,
            };
            let side = usize::from(k.facing < 0.0);
            let n = self.tails.len();
            let ripple = f.run.filter(|r| r.kind == Kind::Ripple);
            // Swung round hard at someone in a play-fight (Act::Swish).
            let swish = gesture.filter(|g| g.act == Act::Swish).map_or(0.0, |g| {
                (TAU * g.t / gesture_secs(Act::Swish).unwrap_or(1.2)).sin() * g.e
            });
            for (i, (tail, quilt)) in self.tails.iter_mut().zip(&self.quilts).enumerate() {
                tail.wave(j, &wave, dt);
                let root = tail.bones[0];
                j[root].rot = j[root].rot + quilt[side] * curl;
                // One after another they lift and settle.
                if let Some(r) = ripple {
                    let lag = 0.6 * i as f32 / n.max(1) as f32;
                    let x = (r.t - lag) / (r.len - 0.6).max(0.3);
                    j[root].rot.x -= 16.0 * bump(x) * r.fade;
                }
                j[root].rot.z += k.facing * 40.0 * swish;
            }
        }
        // Hair hangs: leaning the head, it swings back against the lean;
        // it streams back at pace, bounces with the steps, and turning
        // flings it out (her head shaken, too).
        let lean = (
            j[self.body].rot.x + j[self.head].rot.x,
            j[self.body].rot.z + j[self.head].rot.z,
        );
        let head_turn = j[self.head].rot.y;
        let head_acc = (head_turn - 2.0 * self.head_turn[0] + self.head_turn[1]) / (dt * dt);
        self.head_turn = [head_turn, self.head_turn[0]];
        for (bone, x, z) in &mut self.hair {
            let rx = x.step(
                HAIR_TRAIL * s - HAIR_HANG * lean.0,
                HAIR_BOB * k.acc + HAIR_PACE * k.pace_acc,
                2.0,
                0.22,
                dt,
            );
            let rz = z.step(
                -HAIR_HANG * lean.1,
                -HAIR_TURN * (yaw_acc + 0.5 * head_acc.clamp(-4000.0, 4000.0)),
                1.8,
                0.2,
                dt,
            );
            j[*bone].rot = j[*bone].rot + v3(rx.clamp(-40.0, 40.0), 0.0, rz.clamp(-40.0, 40.0));
        }
        // Ears bob a beat behind the steps.
        let e = self
            .ear
            .step(0.0, -40.0 * k.acc, 3.5, 0.25, dt)
            .clamp(-10.0, 10.0);
        for (side, ear) in self.ears.iter().enumerate() {
            if let Some(b) = *ear {
                let sign = if side == 0 { 1.0 } else { -1.0 };
                j[b].rot.z += sign * e;
            }
        }
        // The bell swings on its cord against the lean, trailing at pace.
        if let Some(b) = self.bell {
            let lean = (
                j[self.body].rot.x + j[0].rot.x,
                j[self.body].rot.z + j[0].rot.z,
            );
            let x = self.bell_swing.0.step(
                -lean.0,
                -900.0 * k.pace_acc.max(0.0) - 40.0 * k.acc,
                1.7,
                0.12,
                dt,
            );
            let z = self.bell_swing.1.step(-lean.1, 0.0, 1.7, 0.12, dt);
            j[b].rot.x += x.clamp(-35.0, 35.0);
            j[b].rot.z += z.clamp(-35.0, 35.0);
        }
    }

    /// Her answers to how she feels and to a touch, with her hands:
    /// laughing, hands on her tummy; fed up, arms limp; sulking, arms
    /// folded; shy, hands to her cheeks; startled, hands to her chest;
    /// blissful or patted, hands together under her chin; a hand of hers
    /// touched, up it comes to yours.
    fn her_moods(&mut self, arms: &Arms, j: &mut [Joint], lv: &Levels, facing: f32) -> Layer {
        let mut layer: Layer = [None; 2];
        let mut both = |place: Place, w: f32| {
            if w > 0.01 {
                layer.fill(Some((place, w.min(1.0))));
            }
        };
        let t = lv.t;
        // Laughing she rocks back and forth, sitting.
        let l = lv.laugh.min(1.0);
        if l > 0.0 {
            j[self.body].rot.x -= l * (10.0 + 7.0 * (TAU * 2.2 * t).sin());
            j[self.head].rot.x -= 6.0 * l;
        }
        both(LIMP, lv.melt);
        both(TUMMY, l);
        both(CROSSED, lv.sulk);
        both(CHIN, (0.6 * lv.head).max(lv.bliss));
        both(CHEEK, lv.shy);
        both(STARTLED, lv.startle);
        if let Some((bone, up)) = lv.paw
            && let Some(side) = arms
                .side
                .iter()
                .position(|a| bone == a.upper || bone == a.fore)
        {
            let wave = 0.04 * (TAU * 2.0 * t).sin();
            layer[side] = Some((OFFER.plus(v3(0.0, 0.25 + wave, -0.1)), up));
            j[self.head].rot.z += facing * 6.0 * up;
        }
        layer
    }

    /// Her own little fidgets: tucking her hair behind her ear, hands
    /// behind her back rocking on her heels, swinging her arms, fiddling
    /// with her fingers, weight from foot to foot, a tilt of the head, a
    /// little shake that rings her bell (her tails' ripple is with the
    /// tails).
    fn her_fidget(&mut self, j: &mut [Joint], f: &Out, k: &Shake, seat: f32) -> Layer {
        let mut layer: Layer = [None; 2];
        let Some(run) = f.run else {
            return layer;
        };
        let (e, t, side) = (run.e(), run.t, run.side);
        let x = (t / run.len).clamp(0.0, 1.0);
        let standing = 1.0 - seat;
        let hand = usize::from(side < 0.0);
        let sg = if hand == 0 { 1.0 } else { -1.0 };
        match run.kind {
            Kind::HeadTilt => {
                j[self.head].rot.z += e * (side * 14.0 + 3.0 * (TAU * 0.7 * t).sin());
            }
            Kind::WeightShift => {
                let w = (TAU * 0.6 * t).sin();
                j[self.body].rot.z += 4.0 * e * w;
                j[self.body].shift.x += 0.02 * e * w;
                j[self.head].rot.z -= 2.0 * e * w;
                layer.fill(Some((BEHIND, e * standing)));
            }
            Kind::HairTuck => {
                let c = smoothstep(0.25, 0.75, x);
                let p = Place {
                    at: BY_FACE.at.lerp(OVER_EAR.at, c),
                    on: On::Head(1.15 + 0.05 * c),
                    pole: v3(1.0, -0.5, 0.2),
                };
                layer[hand] = Some((p, e));
                j[self.head].rot.z -= sg * 6.0 * e;
            }
            Kind::HandsBehind => {
                let w = (TAU * 1.1 * t).sin();
                j[self.body].rot.x += 3.0 * e * w * standing;
                j[self.body].shift.y += 0.012 * e * w.max(0.0) * standing;
                layer.fill(Some((BEHIND, e)));
            }
            Kind::ArmSwing => {
                let w = (TAU * 1.3 * t).sin();
                let p = HANG.plus(v3(0.0, 0.08 * w.max(0.0), 0.35 * w));
                layer.fill(Some((p, e * standing)));
                j[self.body].rot.x -= 2.0 * e * w * standing;
            }
            Kind::Fiddle => {
                j[self.head].rot.x += 7.0 * e;
                for (n, l) in layer.iter_mut().enumerate() {
                    let jitter = 0.02 * (TAU * 6.0 * t + PI * n as f32).sin();
                    *l = Some((CLASP.plus(v3(jitter, 0.03, 0.0)), e));
                }
            }
            Kind::Jingle => {
                let w = TAU * 6.0 * t;
                j[self.head].rot.y += 8.0 * e * w.sin();
                j[self.head].rot.z += 5.0 * e * (w + 0.8).sin();
            }
            Kind::Dream => {
                let lower = usize::from(k.facing <= 0.0);
                layer[lower] = Some((PILLOW.plus(v3(0.0, 0.0, 0.03 * (TAU * 5.0 * t).sin())), e));
            }
            _ => {}
        }
        layer
    }

    /// Her gestures, her own: a stretch with both arms up on her
    /// toes; a yawn behind her hand; a wave; her hand for a 握手; a snack
    /// held in both hands and nibbled; kneeling to dig in the floor or to
    /// pick something up; a shy scratch behind her head (the fox's
    /// scratch); combing her side lock through her fingers (the fox's
    /// wash); a shake of the head; a little dance from foot to foot with
    /// her hands under her chin; a cheer for each hop; a twirl, arms out;
    /// typing along; a finger to her cheek with a tilt of the head;
    /// hugging; the 肉肉舞.
    fn her_gesture(&mut self, j: &mut [Joint], g: Gesture, facing: f32, dt: f32) -> Layer {
        let (e, t) = (g.e, g.t);
        let mut layer: Layer = [None; 2];
        let n = near(facing);
        let far = 1 - n;
        let set = |layer: &mut Layer, side: usize, place: Place, w: f32| {
            layer[side] = Some((place, w));
        };
        let both = |layer: &mut Layer, place: Place, w: f32| {
            for l in layer.iter_mut() {
                *l = Some((place, w));
            }
        };
        match g.act {
            Act::Stretch => {
                let up = smoothstep(0.0, 0.8, e);
                let wob = 0.05 * (TAU * 1.5 * t).sin() * up;
                both(&mut layer, UP.plus(v3(wob, 0.0, 0.0)), up);
                j[self.body].shift.y += 0.035 * up;
                j[self.body].rot.x -= 5.0 * up;
                j[self.body].rot.z += 2.0 * up * (TAU * 0.75 * t).sin();
                j[self.head].rot.x -= 12.0 * up;
            }
            Act::Yawn => {
                set(&mut layer, n, OVER_MOUTH, e);
                set(
                    &mut layer,
                    far,
                    arm(0.72, 0.45, 0.0).pole(1.0, -0.3, -0.3),
                    0.6 * e,
                );
                j[self.head].rot.x -= 16.0 * e;
                j[self.body].rot.x -= 4.0 * e;
                self.chin.v += 0.5 * e * dt;
            }
            Act::Wave => {
                let wave = 0.12 * (TAU * 2.2 * t).sin();
                set(&mut layer, n, WAVE.plus(v3(wave, 0.0, 0.0)), e);
                j[self.head].rot.z += facing * 8.0 * e;
                j[self.body].rot.z += 3.0 * e * (TAU * 1.1 * t).sin();
            }
            Act::Paw => {
                let shake = 0.05 * (TAU * 2.6 * t).sin();
                set(&mut layer, n, OFFER.plus(v3(0.0, shake, 0.0)), e);
                j[self.head].rot.z += facing * 10.0 * e;
            }
            Act::Eat | Act::Chew => {
                let (dip, chew) = bite(t);
                both(&mut layer, SNACK, e);
                j[self.head].rot.x += e * (10.0 * dip - 4.0 * chew.abs());
                j[self.body].rot.x += e * 4.0 * dip;
                if chew.abs() > 0.9 {
                    self.chin.v -= 0.4 * e;
                }
            }
            Act::Dig => {
                for side in 0..2 {
                    let pat = (TAU * 2.4 * t + PI * side as f32).sin().max(0.0);
                    set(
                        &mut layer,
                        side,
                        FLOOR_FRONT.plus(v3(0.0, 0.08 * pat, 0.0)),
                        e,
                    );
                }
                j[self.body].rot.x += 18.0 * e;
                j[self.head].rot.x += 10.0 * e;
            }
            Act::Pick => {
                both(&mut layer, FLOOR_FRONT.plus(v3(-0.03, 0.01, 0.0)), e);
                j[self.body].rot.x += 18.0 * e;
                j[self.head].rot.x += 14.0 * e;
            }
            Act::Sniff | Act::Peck => {
                both(&mut layer, KNEES, e);
                j[self.body].rot.x += 16.0 * e;
                j[self.head].rot.x += e * (14.0 + 6.0 * peck(t, 2.4));
            }
            Act::Scratch => {
                let c = v3(0.0, (TAU * 5.0 * t).sin(), (TAU * 5.0 * t).cos()) * 0.03;
                set(&mut layer, n, BACK_OF_HEAD.plus(c), e);
                let sg = if n == 0 { 1.0 } else { -1.0 };
                j[self.head].rot.x += 6.0 * e;
                j[self.head].rot.z -= sg * 8.0 * e;
            }
            Act::Groom | Act::Preen => {
                // Down the lock and quickly back up, over and over.
                let p = (0.8 * t).fract();
                let c = if p < 0.75 {
                    smoothstep(0.0, 0.75, p)
                } else {
                    1.0 - smoothstep(0.75, 1.0, p)
                };
                let place = Place {
                    at: LOCK_HIGH.at.lerp(LOCK_LOW.at, c),
                    on: On::Head(1.2 + 0.05 * c),
                    pole: v3(1.0, -0.6, 0.0),
                };
                set(&mut layer, n, place, e);
                let sg = if n == 0 { 1.0 } else { -1.0 };
                j[self.head].rot.z -= sg * 10.0 * e;
            }
            Act::Shake => {
                let fade = 1.0 - (t / 1.2).clamp(0.0, 1.0) * 0.6;
                let w = TAU * 4.5 * t;
                j[self.head].rot.y += e * fade * 18.0 * w.sin();
                j[self.head].rot.z += e * fade * 6.0 * (w + 0.8).sin();
                j[self.body].rot.z += e * fade * 4.0 * w.sin();
                for side in 0..2 {
                    let flap = 0.08 * (w + 1.0 + PI * side as f32).sin();
                    set(&mut layer, side, HANG.plus(v3(flap, 0.0, 0.0)), 0.5 * e);
                }
            }
            Act::Wiggle => {
                let w = TAU * 2.8 * t;
                j[self.body].rot.z += e * 10.0 * w.sin();
                j[self.head].rot.z -= e * 7.0 * (w - 0.8).sin();
                for (k, &leg) in self.legs.iter().enumerate() {
                    let up = if k == 0 { w.sin() } else { -w.sin() }.max(0.0);
                    j[leg].shift.y += e * 0.05 * up;
                }
                both(&mut layer, CHIN, e);
            }
            Act::Bounce | Act::Flap => {
                let hz = if g.act == Act::Flap { 6.5 } else { 7.0 };
                for side in 0..2 {
                    let flutter = 0.05 * (TAU * hz * t + PI * side as f32).sin();
                    set(&mut layer, side, CHEER.plus(v3(flutter, flutter, 0.0)), e);
                }
                j[self.head].rot.x -= 8.0 * e;
            }
            Act::Spin => {
                let secs = gesture_secs(Act::Spin).unwrap_or(1.4);
                let u = (t / secs).clamp(0.0, 1.0);
                j[0].rot.y += facing * 360.0 * u * u * (3.0 - 2.0 * u);
                for (k, &leg) in self.legs.iter().enumerate() {
                    let p = TAU * 3.5 * t + PI * k as f32;
                    j[leg].rot.x += e * 14.0 * p.sin();
                    j[leg].shift.y += e * 0.05 * p.cos().max(0.0);
                }
                if let Some(h) = self.hips {
                    j[h].swell += 0.12 * e;
                }
                both(&mut layer, OUT, e);
            }
            Act::Tap | Act::Knead => {
                for side in 0..2 {
                    let tap = 0.05 * (TAU * 4.5 * t + PI * side as f32).sin().max(0.0);
                    set(&mut layer, side, TYPE.plus(v3(0.0, tap, 0.0)), e);
                }
                j[self.head].rot.x += e * 4.0 * (TAU * 4.5 * t).sin().abs();
            }
            Act::Tilt => {
                j[self.head].rot.z += facing * 26.0 * e;
                set(&mut layer, n, CHEEK, e);
            }
            // Her cheeks just squeezed (PLAN §8, Roger's twenty-seventh
            // pass, item 6 (a)): she holds them, rubbing, red, the head
            // shaking a little; or, loved, rubs her face on your hand.
            Act::CheekRub => {
                let c = v3(0.0, (TAU * 2.0 * t).sin(), (TAU * 2.0 * t).cos()) * 0.015;
                both(&mut layer, CHEEK.plus(c), e);
                j[self.head].rot.z += 6.0 * e * (TAU * 1.1 * t).sin();
            }
            Act::Nuzzle => {
                j[self.head].rot.z += 14.0 * e * (TAU * 1.4 * t).sin();
                j[self.head].rot.y += 8.0 * e * (TAU * 1.4 * t).sin();
            }
            Act::Rub | Act::Bliss => {
                both(&mut layer, CHEEK, e);
                j[self.head].rot.z += 10.0 * e * (TAU * 0.8 * t).sin();
                j[self.body].rot.z += 4.0 * e * (TAU * 0.8 * t - 0.6).sin();
            }
            Act::Swish => {
                let u = (t / gesture_secs(Act::Swish).unwrap_or(1.2)).clamp(0.0, 1.0);
                j[self.body].rot.y -= facing * 35.0 * bump(u) * e;
                both(&mut layer, OUT.plus(v3(0.0, -0.2, 0.0)), 0.6 * e);
            }
            Act::Nudge => {
                let u = (t / gesture_secs(Act::Nudge).unwrap_or(0.8)).clamp(0.0, 1.0);
                j[self.legs[n]].rot.x -= 40.0 * bump(u);
                both(&mut layer, OUT.plus(v3(-0.3, -0.35, 0.0)), 0.5 * e);
            }
            Act::Dance => {
                let d = dance_at(t);
                let on_beat = (TAU * d.beat).cos();
                let side = PI * (d.count as f32 + d.beat);
                j[self.body].rot.z += e * 8.0 * side.sin();
                j[self.head].rot.z -= e * 6.0 * (side - 0.8).sin();
                for (k, &leg) in self.legs.iter().enumerate() {
                    let up = if k == 0 { side.sin() } else { -side.sin() }.max(0.0);
                    j[leg].shift.y += e * 0.05 * up;
                }
                match d.step {
                    DanceMove::Bounce => {
                        let flutter = 0.05 * on_beat;
                        both(&mut layer, CHEER.plus(v3(flutter, flutter, 0.0)), e);
                        j[self.head].rot.x -= 8.0 * e;
                    }
                    DanceMove::Wiggle => {
                        j[self.body].rot.y += e * 18.0 * side.sin();
                        both(&mut layer, HIPS, e);
                    }
                    DanceMove::Paws => {
                        let up = (d.count % 2) as usize;
                        let wave = 0.1 * (TAU * 2.0 * t).sin();
                        set(&mut layer, up, WAVE.plus(v3(wave, 0.12, 0.0)), e);
                        set(&mut layer, 1 - up, HIPS, e);
                    }
                    DanceMove::Spin => {
                        let u = d.u;
                        j[0].rot.y += facing * 360.0 * u * u * (3.0 - 2.0 * u);
                        if let Some(h) = self.hips {
                            j[h].swell += 0.12 * e;
                        }
                        both(&mut layer, OUT, e);
                    }
                    // 「登登！」: one hand up high, the other on her hip.
                    DanceMove::Pose => {
                        let hold = smoothstep(0.0, 0.25, d.u);
                        set(&mut layer, n, UP, e * hold);
                        set(&mut layer, far, HIPS, e * hold);
                        j[self.head].rot.x -= e * hold * 10.0;
                        j[self.head].rot.z += facing * e * hold * 10.0;
                    }
                }
            }
            Act::Hug => {
                let sway = (TAU * 0.55 * t).sin();
                let (lean, tilt, lift) = match self.hug {
                    Hug::Cuddle { front } => {
                        if front {
                            // An arm over the other's shoulder.
                            set(&mut layer, n, arm(0.05, 0.25, 0.85), e);
                            set(&mut layer, far, HUG_ROUND, e);
                        } else {
                            both(&mut layer, HUG_ROUND, e);
                        }
                        (10.0, 14.0, 0.0)
                    }
                    Hug::Bear => {
                        both(&mut layer, HUG_ROUND.plus(v3(0.15, 0.05, 0.0)), e);
                        (2.0, 6.0, 0.0)
                    }
                    // Lifted off her feet in a bear hug: arms pinned, feet
                    // paddling.
                    Hug::Squeezed => {
                        both(&mut layer, arm(0.3, -0.8, 0.35), e);
                        for (k, &leg) in self.legs.iter().enumerate() {
                            let kick = (TAU * 3.2 * t + PI * k as f32).sin();
                            j[leg].rot.x -= e * 22.0 * kick;
                        }
                        (-6.0, 8.0, 0.32)
                    }
                    // In your hands: hugging your thumb.
                    Hug::ByYou(_) => {
                        both(&mut layer, HOLD, e);
                        j[self.head].rot.z += e * 8.0 * (TAU * 1.1 * t).sin();
                        (2.0, 6.0, 0.0)
                    }
                };
                j[self.body].rot.x += e * lean;
                j[self.body].rot.z += e * 5.0 * sway;
                j[self.head].rot.z += facing * e * (tilt + 3.0 * sway);
                j[0].shift.y += e * lift;
            }
            _ => {}
        }
        layer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::human;

    /// Reaching puts the middle of her palm where it was asked to go, for
    /// places all round her (in reach), either arm, whichever way the
    /// elbow is to point.
    #[test]
    fn her_palm_lands_where_she_reaches() {
        let (_, sk) = human("fox", "ponytail", "white").unwrap();
        for side in 0..2 {
            let arm = Arm::of(&sk, side).unwrap();
            let s = arm.sign;
            let len = arm.length();
            for (i, dir) in [
                v3(0.2, -0.9, 0.2),
                v3(0.3, 1.0, 0.0),
                v3(-0.6, 0.3, 0.7),
                v3(0.9, -0.1, 0.1),
                v3(-0.2, -0.6, -0.4),
                v3(0.0, 0.2, 1.0),
            ]
            .into_iter()
            .enumerate()
            {
                let dir = v3(s * dir.x, dir.y, dir.z).normalize();
                let to = arm.shoulder + dir * (0.75 * len);
                let pole = v3(s * 0.6, -0.5, -0.4);
                let (up, fore) = arm.reach(to, pole);
                let mut j = vec![Joint::default(); sk.names.len()];
                j[arm.upper].rot = up;
                j[arm.fore].rot = fore;
                let posed = sk.pose(&j);
                let palm = posed[arm.fore].apply(arm.palm);
                assert!(
                    (palm - to).length() < 0.01,
                    "side {side}, place {i}: palm at {palm:?}, wanted {to:?}"
                );
                let elbow = posed[arm.upper].apply(arm.elbow);
                assert!(
                    (elbow - arm.shoulder).length() > 0.0,
                    "side {side}, place {i}: elbow {elbow:?}"
                );
            }
        }
    }
}

#[cfg(test)]
mod gestures {
    use super::*;
    use crate::skin::human;
    use crate::waddle::Mover;

    /// Her hands, posed in the world, sitting (or standing) a while and
    /// then a moment into `act` (none: just settled), facing right (her
    /// right hand toward you); with her arms, head and how they are posed.
    fn hands_in(act: Option<Act>, sitting: bool) -> ([V3; 2], Arms, Vec<crate::skin::Affine>) {
        let (model, sk) = human("fox", "ponytail", "white").unwrap();
        let arms = Arms::new(&sk, &model).unwrap();
        let mut w = Mover::new(&sk, 1.0, &model).unwrap();
        w.still();
        w.sit(sitting);
        let mut j = Vec::new();
        for f in 0..240 {
            if f > 150 {
                w.gesture(act.map(|act| Gesture {
                    act,
                    e: 1.0,
                    t: 0.3 + (f - 150) as f32 / 600.0,
                }));
            }
            j = w.step(1.0 / 60.0, false, 1.0);
        }
        let posed = sk.pose(&j);
        let palms = [0, 1].map(|k| posed[arms.side[k].fore].apply(arms.side[k].palm));
        (palms, arms, posed)
    }

    /// Her hands go where her gestures want them, on her as she is posed:
    /// over her mouth for a yawn (it moves with her head thrown back),
    /// round a snack under it, up and clear of her big head for a
    /// stretch, by her head for a shy scratch, in her lap sitting.
    #[test]
    fn her_hands_go_where_her_gestures_put_them() {
        let near = 1; // facing right, her right hand
        let (palms, arms, posed) = hands_in(Some(Act::Yawn), true);
        let mouth = posed[arms.head].apply(arms.mouth);
        assert!(
            (palms[near] - mouth).length() < 0.1,
            "yawning, her hand {:?} is not over her mouth {mouth:?}",
            palms[near]
        );
        let (palms, arms, posed) = hands_in(Some(Act::Eat), false);
        let mouth = posed[arms.head].apply(arms.mouth);
        for p in palms {
            let d = p - mouth;
            assert!(
                d.length() < 0.3 && d.y < 0.0,
                "eating, a hand {p:?} is not round a snack under her mouth {mouth:?}"
            );
        }
        let (palms, arms, posed) = hands_in(Some(Act::Stretch), false);
        let centre = posed[arms.head].apply(arms.head_c);
        for (k, p) in palms.into_iter().enumerate() {
            let shoulder = posed[arms.side[k].upper].apply(arms.side[k].shoulder);
            assert!(
                p.y > shoulder.y + 0.3,
                "stretching, a hand at {p:?} is not up"
            );
            let r = (p - centre).div_elem(arms.head_r).length();
            assert!(r > 1.05, "stretching, a hand is inside her head ({r:.2})");
        }
        let (palms, arms, posed) = hands_in(Some(Act::Scratch), true);
        let centre = posed[arms.head].apply(arms.head_c);
        let r = (palms[near] - centre).div_elem(arms.head_r).length();
        assert!(
            (1.0..1.35).contains(&r),
            "scratching, her hand is not on her head ({r:.2})"
        );
        let (palms, arms, posed) = hands_in(None, true);
        let chin = posed[arms.head].apply(arms.mouth);
        let shoulder = posed[arms.side[0].upper].apply(arms.side[0].shoulder).y;
        for p in palms {
            assert!(
                p.y < shoulder - 0.2 && p.y < chin.y - 0.3,
                "sitting, a hand {p:?} is not in her lap"
            );
        }
    }
}
