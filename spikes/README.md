# Phase 0 spikes

Throwaway code that answers the Phase 0 questions in [docs/PLAN.md §8](../docs/PLAN.md#phase-0-technical-spikes-not-merged-to-main).
It lives on the `chore/phase0-spikes` branch and is never merged to main; the answers go back into the plan.

`pet-spike` is one executable:

| Mode | Command | Answers |
|---|---|---|
| Pets on the desktop | `pet-spike` (or `--pets 24`) | S1, S2 on a real desktop |
| Settings window | right-click any pet | S3 |
| Headless bench | `pet-spike --bench` | S2 without a desktop |

## Getting a build

Every push to `chore/phase0-spikes` makes GitHub Actions build all three targets and attach them to the run as artifacts:
`pet-spike-windows-x64.exe`, `pet-spike-linux-x86_64`, `pet-spike-linux-aarch64`.
The same run prints the headless bench for each runner.

Building locally instead: `cargo build --release` in `spikes/pet-spike`.

## What to try, and what to report

Run it from a terminal: it prints everything worth reporting, including a line every 5 seconds with frames per second, drawing and presenting cost, CPU and memory.

### S1: windows (every environment)

Environments: Windows 11, Ubuntu 24.04 GNOME (Wayland session and "Ubuntu on Xorg"), KDE Plasma 6 Wayland. On Linux the spike always uses X11 (XWayland in Wayland sessions), as v1 will.

1. Six Shiba stand-ins appear at the bottom of the screen, on top of other windows, **with no box or black square around them**.
2. Clicking **between** pets, or on the transparent corners of a pet, reaches the window underneath.
3. Clicking a belly makes it jiggle (`poke` in the terminal).
4. Dragging lifts a pet; shaking it left and right prints `shake detected`; letting go throws it and it lands with a bounce.
5. Pets walk along the top of the taskbar, not behind it.

On Linux, if the pets misbehave, try again with `--x11-managed` (a window-manager-managed window instead of an override-redirect one) and note which works better.

### S2: numbers

Copy the 5-second lines after the pets have settled, for `pet-spike` and for `pet-spike --pets 24`. The plan's budget is under 1% of one core and under 80 MB for 6 pets (PLAN §5).

### S3: settings process (every environment)

1. Right-click a pet: a rounded, frameless settings window opens (`settings process started`).
2. Type a name with the **Chinese input method** (Zhuyin on Windows, fcitx5 and ibus on Linux): the candidate window should appear next to the text, and the text should commit correctly.
3. Press 「送給寵物」: the terminal prints `IPC <- name …`.
4. Toggle the paw switch and press the buttons: judge whether Slint's animation feels cute enough.
5. Press 「模擬當掉」: the settings window dies, the terminal prints `settings process ended`, and **the pets keep running**.

### S4: portable

Run the downloaded file directly, without installing anything: on a clean Windows VM (no VC++ redistributable), Ubuntu 22.04, Ubuntu 24.04 and Debian 12. Report any missing-library error.

### S6: GNOME Wayland full screen

In a GNOME Wayland session, start a full-screen video in a native Wayland app (e.g. Firefox) while the pets run: are the pets drawn over the video, or hidden behind it?

## Known shortcuts in the spike

- The IPC socket uses the generic namespace (an abstract socket on Linux); Phase 2 moves it under `$XDG_RUNTIME_DIR` per PLAN §6.
- Paths are rebuilt every frame and the window is a fixed 256 px square; see the S2 notes in PLAN §5 for what Phase 1 changes.
- The settings window uses the system's fonts, not the bundled ones.
