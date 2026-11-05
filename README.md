# chonkGene (肉肉賴桌)

A gang of chubby desktop pets that lounge on your desktop all day being cute.

- **Six pets**: Shiba Inu, Munchkin cat, Scottish Fold, owl, chick and duckling, each with its own personality.
- **Poke the chonk**: bellies jiggle; keep poking and some roll over laughing while others look thoroughly fed up.
- **Irrational mode** (on by default): with two or more pets they squabble over small things, always making up in the end. The chick and duckling talk past each other (雞同鴨講).
- **They sleep**: snot bubbles and sleep talk; wake them gently and all is well, shake them awake and they get grumpy (the Shiba gets the zoomies).
- **Nurturing**: affinity only goes up and unlocks tricks; snacks; each pet writes a growth diary in its own voice. Pets never die or get sick.
- **Sound mode** (on by default): volume and rate limits; muted automatically in full screen and video calls.
- **Appearance**: coat colour, pattern, accessories, body shape (slightly chubby by default), size, personality.
- **Doesn't get in the way**: clicks pass through transparent areas; pets hide in full screen; one key hides them all.
- **Portable**: unpack and run on Windows and Linux; no administrator rights, no network access.

The UI is in Traditional Chinese first, with English to follow.

## Status

Working toward beta `0.6.0-beta.1` (the `0.5.0` betas are withdrawn): seven pets as skinned 3D meshes that walk across the desktop and its windows, sit, sleep, eat, play with a grass stalk, a ball, a frisbee and catnip, do as they are asked (sit, lie down, come, spin, shake hands), keep you company while you read or type, have little scenes of their own and play together, answer touch and never stop fidgeting, with synthesised voices. See [docs/PLAN.md §8](docs/PLAN.md#8-phases) for what is in it and what is left.

## Trying it

Download the archive for your system from [Releases](https://github.com/ragnarokmin0714/chonkGene/releases) (until `0.6.0-beta.1` is out, take the build of the latest push from [Actions](https://github.com/ragnarokmin0714/chonkGene/actions), kept 14 days), unpack it and run `chonkGene.exe` (Windows) or `chonkgene` (Linux). Settings and pets are kept in the `chonkGene-data` folder next to it.

- Click a pet to poke it; keep poking and it laughs belly-up or looks thoroughly fed up.
- Drag to lift it, shake it, let go to throw it.
- Right-click a pet for its menu: pat, snacks, sleep, rename, settings.
- Ctrl+Alt+P hides every pet; press it again to bring them back.

Building from source: `cargo run --release -p chonkgene`. The pets are 3D models in `assets/species/<id>/model.ron`; `cargo run --release -p chonkgene-render --example sprites -- shiba out` renders every pose of one into `out/` for a look.

## Platforms

Windows 10/11 and Linux (x86_64, aarch64; X11 and Wayland sessions).
Some features are reduced in Wayland sessions; see [docs/PLAN.md §4](docs/PLAN.md#4-platform-support-matrix).
