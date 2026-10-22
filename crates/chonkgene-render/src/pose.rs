//! Maps procedural `PoseParams` onto bones by their conventional names
//! (`root`, `body`, `belly`, `head`, `tail`, `eye_l`, `eye_r`).

use chonkgene_core::anim::PoseParams;
use chonkgene_rig::{Local, Pose, Rig};

/// Indices of the conventional bones; a rig may lack some.
#[derive(Clone, Copy, Debug)]
pub struct BoneMap {
    root: Option<usize>,
    body: Option<usize>,
    belly: Option<usize>,
    head: Option<usize>,
    tail: Option<usize>,
    eye_l: Option<usize>,
    eye_r: Option<usize>,
}

impl BoneMap {
    pub fn new(rig: &Rig) -> Self {
        Self {
            root: rig.bone("root"),
            body: rig.bone("body"),
            belly: rig.bone("belly"),
            head: rig.bone("head"),
            tail: rig.bone("tail"),
            eye_l: rig.bone("eye_l"),
            eye_r: rig.bone("eye_r"),
        }
    }

    pub fn apply(&self, p: &PoseParams, pose: &mut Pose) {
        let mut set = |bone: Option<usize>, local: Local| {
            if let Some(slot) = bone.and_then(|i| pose.bones.get_mut(i)) {
                *slot = local;
            }
        };
        let breath = 1.0 + 0.015 * p.breath;
        set(
            self.root,
            Local {
                translate: (0.0, p.bob),
                rotate: p.sway,
                scale: (p.facing, 1.0),
            },
        );
        set(
            self.body,
            Local {
                scale: (1.0 + 0.45 * p.squash, (1.0 - 0.45 * p.squash) * breath),
                ..Local::default()
            },
        );
        set(
            self.belly,
            Local {
                scale: (1.0 + 0.5 * p.belly, 1.0 - 0.5 * p.belly),
                ..Local::default()
            },
        );
        set(
            self.head,
            Local {
                translate: (0.0, 10.0 * p.squash - 0.8 * p.breath),
                ..Local::default()
            },
        );
        set(
            self.tail,
            Local {
                rotate: p.tail,
                ..Local::default()
            },
        );
        let eyes = Local {
            scale: (1.0, p.eyes_open),
            ..Local::default()
        };
        set(self.eye_l, eyes);
        set(self.eye_r, eyes);
    }
}
