# chonkGene build plan

> This document records the design decisions that are settled, and for each phase what to build and when it counts as done.
> Build from this document; when a decision changes, change this document first, then the code.
> Items marked **"verify in Phase 0"** are reasoned, not yet confirmed on real machines.
> UI copy and pet lines are quoted in Traditional Chinese (zh-TW), the product's default language, with an English gloss where it helps.
>
> Last updated: 2026-10-22

---

## 0. Settled decisions

| Topic | Decision | Why |
|---|---|---|
| Platforms | **Windows 10/11 + Linux (x86_64, aarch64)**; no macOS | Requirement |
| Distribution | **Portable, no installer**: one `.exe` on Windows, one binary on Linux, each in an archive | Requirement; see §2 |
| Language | **Rust** (edition 2024, latest stable) | Matches gamegene, graphicgene, etc.; memory safety |
| Pet rendering | **CPU rasterising (tiny-skia)**, one small transparent window per pet | Windows are small (~256 px), so CPU drawing is cheap; avoids per-platform GPU transparency driver issues; no resident GPU context while idle |
| Pet art | **Parametric vector parts + bones + morph targets** | Coat colour, pattern and body shape are parameters, so no redraw per combination; crisp at any scale |
| Settings UI | **Slint**, run in a separate child process (same executable with `--settings`) | The UI must be very cute and Slint makes that easiest (declarative animation, state transitions); portability rules out a WebView. Comparison in §3.6 |
| Linux display protocol | v1 always speaks **X11** (native X11 or XWayland) | A normal Wayland window cannot place itself; XWayland exists on GNOME and KDE. Native Wayland is later (§9) |
| Network | **v1 never touches the network** | No feature needs it; network crates banned with cargo-deny (§6) |
| Settings and save format | **RON** | Matches borrowborne and gamegene; human-readable |
| Defaults | Sound **on**, irrational mode **on**, body **slightly chubby (略肥)**, start-at-login **off** (asked on first run) | Requirement; writing a login item without consent is bad practice |
| Affinity | **Only ever goes up**; annoying a pet only lowers its short-term mood, which recovers within minutes | The people who poke pets most are the ones who like the app most and should not be punished; guilt makes people quit the app |
| Nurturing floor | Pets **never die, get sick or run away**; days without opening the app cost nothing | Something that is on all day must not become a chore; same principle as "don't disturb" |
| Documentation language | Repo docs (README, `docs/`) in **English**; UI copy in zh-TW | Public repo |

---

## 1. Product spec

### 1.1 Pets

v1 has six species. A species only sets **default personality**; each individual rolls its own personality near those defaults when adopted, editable later in settings.

| Species | Default name | Ticklish | Patience | Energy | Daily rhythm | Signature moves |
|---|---|---|---|---|---|---|
| Shiba Inu (柴犬) | 阿柴 | high | medium | high | most excited in the morning | belly-up laughing, the Shiba scream, playing dead to refuse walking |
| Munchkin cat (曼赤肯貓) | 奶茶 | high | high | medium | daytime | legs too short to jump up; rolls belly-up and cannot roll back |
| Scottish Fold (蘇格蘭折耳貓) | 芋圓 | low | low | low | lazy all day | "uncle sit", dead-fish eyes, sighing, swatting the cursor away |
| Owl (貓頭鷹) | 咕咕 | medium | medium | low by day, high at night | **nocturnal** (system clock) | 270° head turn, puffing into a ball, dozing by day |
| Chick (小雞) | 蛋黃 | medium | low | high | **early riser**: up first, asleep first | pecks at the cursor and window edges; puffs up and drops a feather when startled; tiny rapid steps |
| Duckling (小鴨) | 湯圓 | medium | high | low | daytime | waddles; lines up behind another pet and follows it; floats on the taskbar as if on water; "呱" when poked |

- **How many pets are on the desktop at once** is a setting: **6 by default, no upper limit**. Above 6 the setting shows a plain hint that CPU and memory use grow with each pet (§5); nothing stops the user.
- Adopted pets beyond that number rest in the **pet house (寵物窩)** and can be swapped onto the desktop at any time. The same species can be adopted more than once.
- First run shows an **adoption screen** (multi-select), with the start-at-login choice.
- Chick morning call: the first wake of the day may crow (「啾啾啾——！」), subject to sound mode and quiet hours (§1.7).

### 1.2 Poking and click interactions

**Hit zones**: each rig defines these zones, each with its own reaction.

| Zone | Single poke | Long press |
|---|---|---|
| Belly (the main event) | flesh jiggles; adds tickle | belly rub: melts into a puddle |
| Cheeks | puff up | cheek squish |
| Paw pads (birds: feet / wings) | reaches back and touches the cursor (birds flap) | handshake (birds: high-five with a wing) |
| Bottom | startles and looks back | tail flick at you |
| Head | squints | head pat: nuzzles the cursor |

**The flesh must jiggle**: belly and cheek control points hang on spring-damper systems; a higher body value means more mass and amplitude. "Slightly chubby by default" and poking share one parameter set.

**Escalation**: pokes in quick succession build "tickle", which decays over time (half-life about 2 s, tuned in implementation).

| Stage | Trigger | Reaction |
|---|---|---|
| 1 | low tickle | small reaction (head tilt, flinch, side-eye) |
| 2 | tickle passes the first threshold | splits by personality: **ticklish → belly-up laughing**; **not ticklish → fed up (厭世)** |
| 3 | overdone | species finale: Shiba plays dead, Munchkin rolls away, Fold turns its back, Owl stares in judgement, Chick runs a panicked lap flapping, Duckling spins and plops on its bottom |

- Personality only sets weights, nothing is hard-coded: the same Shiba is sometimes fed up, so reactions stay unpredictable.
- Current state adjusts weights: **poked while asleep is always fed up**; good mood leans towards laughing.
- **Fed up must be funny, never really angry**: no permanent affinity loss; calms down after 1–3 minutes and comes back to nuzzle the cursor.
- Spam clicks do not restart animations: a counter only picks the stage; poke sounds are rate-limited too (§1.5).

**Other interactions**
- **Lift**: dragging grabs the scruff; body hangs, feet dangle.
- **Shake**: while dragging, 4+ left-right reversals within 1.2 s count as a shake. Shaken while awake → dizzy; shaken while asleep → see §1.12.
- **Throw**: release carries velocity: arc, bounce off screen edges, land with a bounce; chubbier pets land heavier.
- **Right click**: pet menu. The header shows name, species, affinity level and current state (e.g. 「睡覺中」 asleep, 「有起床氣」 grumpy). Items in order: 摸摸頭 (pat), 餵零食 (feed), 改名字… (rename), 幫牠換造型… (change look), 讓牠去睡覺 (put to sleep; while asleep it becomes 輕輕叫醒牠, wake gently), 全部收起來 (hide all), 設定… (settings).
- **Rename** from the right-click menu, no settings window needed; the same popover also edits 「牠怎麼叫你」 (what it calls you).

### 1.3 Behaviour

- **Action library**: at least 15 idle/move actions per species; weighted random choice with a penalty for recent actions (**the same action never runs more than 2 times in a row**).
- **Needs**: energy, mood, social; they steer action choice (low energy → dozing).
- **Daily rhythm**: each species has a 24-hour activity curve tied to the system clock.
- **Idle detection**: user idle for N minutes → pets fall asleep or come over to look at the cursor; user returns → they stretch. Uses only the system idle-time API, **never hooks keyboard or mouse** (§6).
- **Cursor curiosity**: when the cursor comes near, sometimes turn the head or chase it a little (low-rate cursor polling, no hooks).
- **Desktop terrain**:
  - walk along the **bottom of the work area** (top edge of the taskbar), with the taskbar on any side;
  - sit on **window tops**; fall when the window is dragged away or closed (Windows and native X11; Wayland sessions see §4);
  - walk between monitors, with correct per-monitor DPI scaling.

### 1.4 Irrational mode (on by default)

- Only with **two or more** pets.
- **Relationship table**: every pair has a fondness value; some pairs love to squabble (e.g. Shiba × Fold, Chick × Duckling).
- **Skits**: at least 7 in v1:
  - fight over a spot, stare-off, swatting, chasing;
  - one pet is poked into laughter, the other laughs at it → the first chases it in embarrassment;
  - **雞同鴨講** (chick and duck talk past each other): their bubbles go 「啾啾！」/「呱呱。」 back and forth, louder each time, until both shrug;
  - the duckling lines up behind another pet and follows it everywhere; the leader gets annoyed, then gives up and leads a parade.
- **Always ends in making up**: grooming each other, sleeping side by side.
- **Frequency**: settings offer 偶爾 / 常常 / 超常 (sometimes / often / constantly); each pair has a cooldown.
- **Breaking it up**: clicking either pet during a skit stops it; both look at you, then make up.
- Paused while any "don't disturb" state is active (§1.7).

### 1.5 Sound mode (on by default)

- One sound set per species (OGG Vorbis): calls, laugh, sigh, snore, landing. The Shiba scream and the duck's 「呱」 are mandatory.
- **Master volume slider**, plus per-pet mute.
- **Rate limit**: a cap on sounds per pet per minute; a poke burst plays one sound.
- Muted automatically in any "don't disturb" state.

### 1.6 Appearance

| Item | Detail |
|---|---|
| Name | Chinese input must work (IME, verify in Phase 0); max 12 characters |
| What it calls you (牠怎麼叫你) | How the pet addresses you in bubbles; per pet, max 8 characters. Defaults by species: Shiba 「主人」, Munchkin 「好朋友」, Fold 「奴才」, Owl 「同學」, Chick 「老大」, Duckling 「領隊」 |
| Eating makes it rounder (吃多了會慢慢變圓) | Optional, **off** by default. When on, feeding slowly raises the body value above the slider setting and play brings it back; when off, body shape follows only the slider |
| Coat colour | Preset sets per species (Shiba: red, black, sesame, white; cats: orange, grey, milk-tea, black…; Chick: yolk yellow, white, black silkie; Duckling: white, yellow, mallard), plus custom colours |
| Pattern | tabby, calico, tuxedo, etc.; implemented with palette slots |
| Body | continuous slider with four snap points: **標準 standard → 略肥 slightly chubby (default) → 圓滾滾 round → 麻糬 mochi** |
| Size | overall scale (50%–200%) |
| Accessories | scarf, hat, collar, bow tie; **seasonal / holiday outfits** applied automatically (can be turned off) |
| Personality | ticklish, patience and energy sliders |

- **Live preview** while editing: changing body shape makes the pet look down at its belly in surprise; changing colours makes it spin to look at itself.
- Lunar holidays (Lunar New Year, etc.) come from a **built-in date table** covering 10 years; no lunar-calendar crate.

### 1.7 Don't disturb

| Situation | Behaviour |
|---|---|
| Foreground full screen (games, video, presentations) | pets hide + mute |
| Microphone or camera in use (video calls) | mute + irrational mode paused (pets stay visible) |
| Quiet hours (user-set) | mute; optionally hide too |
| Global hotkey | hide / bring back all pets |
| State ends | pets peek back in from the screen edge |

- **Click-through**: clicks on transparent pixels must pass through; only the pet's body is interactive.
- Per-platform detection in §3.5.

### 1.8 Adapting to the desktop

- **Light / dark**: follow the system; settings UI and bubbles follow.
- **Accent colour**: pets' scarves default to the system accent colour (can be turned off).
- **Taskbar position and size**: read the work area from the system, never hard-code.
- **Reduced motion**: follow the system setting where available, plus a manual switch; when on, large wobbles, bounces and UI spring animations are off.

### 1.9 UI and fonts

**Fonts** (all SIL OFL, bundled; full licence texts in `licenses/`)

| Use | Font |
|---|---|
| Main UI | **jf open 粉圓體 (jf open huninn)** |
| Speech bubbles | **芫荽 Iansui** (handwritten feel); the build keeps only glyphs used by built-in lines; user-entered names fall back to huninn |
| Latin and digits | **Fredoka** or **Nunito** (decided in Phase 4 by how they pair) |

**Font check results** (Phase 0 S5, done 2026-10-22)
- jf open huninn 2.1: TTF 4.9 MB (woff2 2.2 MB). Declares Reserved Font Names "open huninn" and "huninn", so **any subset or modified version must be renamed**. The UI ships the **unmodified full font** (user-entered names can be any character), so no rename is needed.
- Iansui 1.020: TTF 9.4 MB (woff2 3.0 MB); no Reserved Font Name, so subsetting needs no rename.
- Both cover the test characters (萌, 戳, 糬, 嘆, 鷹, etc.).

**Visual target**: interactive mockup <https://claude.ai/artifact/YNitgf9JsVtqCVXY9PE4xe> (desktop poke demo, first-run adoption, settings window; uses the real fonts above).

**UI elements**
- Switches: paw pads; turning one on makes a soft 「啵」 pop (respects sound mode).
- Sliders: the thumb is a dried fish or a bone; the body slider's thumb gets fatter with the value.
- Buttons: squash on press, then spring back.
- Cursor: becomes a pointing hand over a pet.
- Theme colour follows the selected pet: Shiba red-orange, Munchkin milk tea, Fold blue-grey, Owl chestnut with moon yellow, Chick yolk yellow, Duckling pond blue.
- Dark mode uses warm dark greys, never pure black.

**Floor**: text contrast meets WCAG AA, minimum text size 13 px; wobbling elements stay restrained and respect reduced motion.

### 1.10 Tray, menu, hotkey, start at login

- **Tray icon**: hide / bring back, settings, pause irrational mode, quit. Linux uses StatusNotifierItem; GNOME without the AppIndicator extension has no tray, so **every function is also reachable from the pet's right-click menu**.
- **Global hotkey**: `Ctrl+Alt+P` by default (configurable).
- **Start at login**: off by default, asked on first run; see §2.3.
- **Single instance**: a second launch does not start another copy; it tells the running one to bring the pets back and open settings.

### 1.11 UI copy rules

Every string in the UI follows this section; check the glossary below before adding text.

**Two voices**
- **Labels** (switches, buttons, tab names): plain and clear at a glance, e.g. 「聲音模式」, 「勿擾時段」, 「大小」. No puns or baby talk in labels.
- **Descriptions, hints, pet lines, completion screens**: this is where the cuteness goes, e.g. 「開機時一起醒來」, 「牠們先躲起來了」, 「我是一顆麻糬。」

**Writing**
- Taiwan usage: 設定 (not 設置), 螢幕, 資料夾, 捷徑, 預設, 視窗, 滑鼠, 檔案, 麥克風, 鏡頭.
- Animals are 「牠」, two or more 「牠們」. Sentences that depend on the count must change with it (「帶牠回家」 / 「帶牠們回家」).
- A switch label says **what happens when it is on**: 「全螢幕時躲起來」, never 「全螢幕偵測」.
- Descriptions are at most two sentences, each ending in 「。」. Exclamation marks only in the welcome title and pet lines.
- Items that open another window or ask for confirmation end in 「…」: 「設定…」, 「移除…」.
- Full-width Chinese punctuation; a space between Chinese and Latin text or digits (「按 Ctrl+Alt+P」); no space before a percent sign (「100%」).
- Privacy-related features say plainly **what they do not do**: 「只看是否使用中，不會聽內容。」
- Error messages say what happened and what you can do, and never blame the user.

**Glossary**

| Concept | Always write | Never write |
|---|---|---|
| hide all pets | 收起來 | 隱藏, 最小化 |
| bring pets back | 叫回來 | 顯示, 還原 |
| add a pet | 領養 | 新增, 建立 |
| fat vs thin | 體型 (標準, 略肥, 圓滾滾, 麻糬) | 身材, 胖度 |
| scale | 大小 | 尺寸, 比例 |
| squabble feature | 非理性模式 | 打架模式 |
| start at login | 開機時一起醒來 (descriptions may say 「開機自動啟動」) | 開機自啟, 自動執行 |
| long-term fondness | 親密度 (levels written 「Lv.3」) | 好感度, 愛心值 |
| short-term emotion | 心情; when annoyed 「有起床氣」, 「在鬧脾氣」 | 怒氣值, 生氣中 |
| how the pet addresses you | 牠怎麼叫你 | 暱稱設定, 使用者名稱 |
| nurturing record | 成長日記 | 養成, 日誌, 紀錄檔 |
| adopted pets not on the desktop | 在寵物窩休息 | 停用, 收納, 封存 |

**Pet lines**
- One voice per species:
  - Shiba: warm and clingy;
  - Fold: often opens with 「……」, aloof but secretly fond (傲嬌);
  - Munchkin: tries hard, a little hard done by;
  - Owl: sleepy by day, hyper at night;
  - Chick: impatient, short bursts (「啾？！」);
  - Duckling: half a beat slow, lines often end in 「呱。」
- Lines live in RON string tables; tests check that **a bubble line is at most 14 full-width characters** and that every language has every line.
- Lines may include `{name}` (the pet's name) and `{you}` (what it calls you); the length check counts them at their maximum lengths.

### 1.12 Sleep and waking up grumpy

**When pets sleep**
- Daily rhythm: Shiba, Munchkin, Fold and Duckling sleep at night; Owl dozes by day; Chick sleeps earliest and wakes earliest.
- The user has been idle a long time (§1.3).
- Naps when energy runs out.
- Right-click menu 「讓牠去睡覺」.

**While asleep**: a snot bubble grows and shrinks with breathing, occasional sleep talk (「……肉乾……」), very quiet snoring. Respects sound mode and don't disturb (§1.7).

**How it is woken decides the reaction**

| How | Reaction | Mood |
|---|---|---|
| One poke | does not wake: rolls over, mumbles | unchanged |
| Two or more pokes within 2 s, or menu 「輕輕叫醒牠」 | wakes: rubs its eyes, yawns, greets you (using 「牠怎麼叫你」) | unchanged |
| **Lifted and shaken** (§1.2) | startled awake, species-specific grumpiness | drops; holds a grudge |

**Grumpiness**

| Species | After being shaken awake |
|---|---|
| Shiba | **Zoomies**: tears back and forth along the taskbar for about 3 s, then pants 「醒了！」 |
| Munchkin | groggy, walks crooked, bumps into the screen edge, looks hard done by |
| Fold | dead-fish eyes, swats you once, then turns its back and goes back to sleep |
| Owl | woken by day: feathers puff up, glares with one eye, falls asleep again immediately |
| Chick | bursts into a puff of feathers, flaps around clucking, then smooths its feathers as if nothing happened |
| Duckling | dizzy: spins, plops down, 「呱？」, and nods off again sitting up |

**Holding a grudge**: for 3–5 minutes after the mood drops, pokes get cold replies and head pats are dodged. When time is up it calms down by itself and comes back to nuzzle you (「……好啦，原諒你。」). **Affinity still only goes up** during a grudge. Feeding shortens it.

### 1.13 Nurturing

**Principle**: see §0 "Affinity" and "Nurturing floor". Nurturing gives people a reason to come back each day, not pressure.

**Affinity (親密度)**
- Per pet: pokes, pats, snacks and playing after waking all add; **nothing subtracts**.
- A daily cap on gainable affinity, so click-spamming cannot farm levels.
- Levels (from Lv.1) unlock new lines, new actions (tricks) and accessories.
- After days away, pets greet you with 「你終於回來了！」 and nothing has decreased.

**Mood (心情)**: short-term; steers action choice and the tone of lines; drops when annoyed (§1.12) and returns to normal within minutes.

**Snacks**
- Fed from the right-click menu, or by dragging a snack from the tray onto a pet. Each species has a favourite: Shiba jerky, cats wet food (罐罐), Chick corn kernels, Duckling peas, Owl to be decided.
- A daily portion; a full pet declines (「吃不下了……」). Pets never go hungry from not being fed.
- Affects body shape only when 「吃多了會慢慢變圓」 is on (§1.6).

**Tricks**: learned with affinity, e.g. handshake (Lv.1), sit (Lv.2), spin (Lv.3), roll over (Lv.4), act cute (Lv.6). Learned tricks are sometimes performed spontaneously and can be requested from the menu. Birds get bird versions (wing wave instead of handshake).

**Growth diary (成長日記)**, a settings tab
- Affinity level and progress, learned tricks.
- Today's small stats: times poked, belly-up laughs, squabbles (all made up), time asleep.
- Diary: the pet writes the day's funny moments in its own voice (「午睡被搖醒，忍不住在工作列上爆走了三圈。」). Keeps the last 30 days.
- **Stays on this computer** and is never uploaded; 「移除所有痕跡」 in settings also asks whether to delete it.

---

## 2. Portable distribution

### 2.1 Release artefacts

| File | Contents |
|---|---|
| `chonkGene-<version>-windows-x64.zip` | `chonkGene/chonkGene.exe` + `chonkGene/chonkGene-data/` |
| `chonkGene-<version>-linux-x86_64.tar.gz` | `chonkGene/chonkgene` + `chonkGene/chonkGene-data/` |
| `chonkGene-<version>-linux-aarch64.tar.gz` | same |
| `SHA256SUMS` | hashes of all archives |

- Unpack and double-click; **no administrator rights, no writes to system directories**.
- All art, fonts and sounds are **embedded in the executable**; no external assets are read at run time.
- **Windows**: statically linked CRT (`+crt-static`), so no VC++ redistributable; embedded icon and version info. **No UPX** (antivirus false positives).
- **Linux**: built against a glibc 2.31 baseline (cargo-zigbuild), covering Ubuntu 20.04+ and Debian 11+. X11, xkbcommon and ALSA, present on every desktop, stay dynamically linked. **No AppImage**: Ubuntu 22.04+ lacks libfuse2 by default, so it would not even open.
- Settings has 「加到應用程式選單」 (add to the app menu): writes a `.desktop` file and icon to `~/.local/share/applications/` on Linux; on Windows it optionally creates a Start menu shortcut.

### 2.2 Where data lives (portable mode)

- A `chonkGene-data/` folder next to the executable → **portable mode**: all settings and saves go there (the whole folder can be carried around).
- Otherwise → the system config directory (`%APPDATA%\chonkGene` on Windows, `~/.config/chonkgene` on Linux).
- Release archives already include `chonkGene-data/`, so portable mode is the default.
- Settings shows the current data location.

### 2.3 Start at login and removal

- Start at login writes user-level entries: Windows `HKCU\...\Run`; Linux `~/.config/autostart/chonkgene.desktop`.
- Checked on every start: if start at login is on but the recorded path is not the current executable → update it (moving the folder does not break it).
- Settings has 「移除所有痕跡」 (remove all traces): deletes the login entry and menu entries, and asks whether to delete data. After that, deleting the folder removes everything.

### 2.4 Updates

- v1 has no network access, so **no auto-update**. 「檢查更新」 just opens the GitHub Releases page in the browser.

---

## 3. Architecture

### 3.1 Workspace

| Crate | Responsibility | unsafe |
|---|---|---|
| `chonkgene-core` | pet state, action choice, tickle, relationships, skits, spring physics, daily rhythm, affinity and mood. **Pure logic, no IO**; seedable RNG | `forbid` |
| `chonkgene-rig` | rig format, morph targets, palettes, hit zones, animation clips; pose → paths | `forbid` |
| `chonkgene-render` | rasterises paths and bubble text with tiny-skia (ab_glyph for text) | `forbid` |
| `chonkgene-platform` | transparent window presentation, click-through, work area, full-screen / meeting / idle detection, theme, login item, tray, hotkey | **only crate allowed**, every block commented with its reason |
| `chonkgene-settings` | Slint settings UI (`.slint` files), cute widgets, theme; runs as the child process | `forbid` |
| `chonkgene-ipc` | message format and channel between the pet process and the settings process | `forbid` |
| `chonkgene` (bin) | event loop, wiring, saves, audio (rodio); runs the settings UI when started with `--settings` | `forbid` |
| `xtask` | art build (SVG → rig data), font subsetting, benchmarks | — |

### 3.2 Windows and event loop

- The pet process has **one winit event loop** owning every pet window: one per pet and one per speech bubble (so a pet window does not grow to fit its bubble).
- The settings window is **a separate process**: the same executable started with `--settings`, talking to the pet process over the single-instance IPC channel (§3.5). Why:
  - winit allows one event loop per process, and Slint needs its own, so separate processes are cleanest;
  - closing settings returns all of that process's memory to the system;
  - a crash in the settings UI cannot take the pets with it.
- The live preview in settings is rasterised by the same `chonkgene-render` and handed to Slint as an image; changes go over IPC to the pet process immediately.
- Pet windows bypass the GPU; `chonkgene-platform` pushes pixels straight to the window:
  - **Windows**: `UpdateLayeredWindow` with premultiplied BGRA. Layered windows **natively let clicks through** pixels with alpha = 0.
  - **X11**: ARGB visual + `XPutImage` (or MIT-SHM); an **XShape input region** set to the pet's silhouette (updated at a low rate).
- Lifting and throwing move the window every frame.

> **Verify in Phase 0**: adding `WS_EX_LAYERED` to a winit-created HWND and using `UpdateLayeredWindow`; drawing into a winit-created X11 window from a separate x11rb connection. If either fails, the platform crate creates pet windows itself.

### 3.3 Art pipeline

- Art is drawn as SVG in Inkscape (or graphicgene), with parts named by convention (`body`, `belly`, `cheek_l`, `paw_fl`, `wing_l`, …).
- `rig.ron` describes bones and pivots, part-to-bone weights, palette slots (`fur_main`, `fur_belly`, `pattern`, `nose`, `eye`; birds use `feather_main`, `beak`, `comb`), morph targets (standard ↔ mochi as two structurally identical paths), hit zones, and anchors (bubble, accessories).
- `cargo xtask art` compiles SVG + `rig.ron` into compact binary data embedded in the executable. **No SVG parsing at run time** (fast start, fewer dependencies, smaller attack surface).
- Build-time checks: morph targets have matching path structure, every palette slot is defined, every hit zone exists, every bone referenced by a clip exists.
- Two animation layers, blended: **procedural** (breathing, blinking, tail, ear twitch, belly spring) + **keyframed clips** (belly-up, playing dead, uncle sit, etc., in RON).

### 3.4 Data files

- Settings and saves: `settings.ron`, `pets.ron` (appearance, personality, affinity, mood, today's stats), `diary.ron` (last 30 days); each has a `format` version field.
- Writes always go to a temp file, then rename, so a power cut mid-write cannot corrupt them.
- Reads have a size cap; on a parse failure the bad file is renamed as a backup and the app starts with defaults and says so.
- UI and pet-line localisation: one RON string table per language (`zh-TW` default, `en`).

### 3.5 Platform detection

| Feature | Windows | X11 (incl. XWayland) |
|---|---|---|
| Work area | `rcWork` from `GetMonitorInfo` | `_NET_WORKAREA` |
| Full screen | `SHQueryUserNotificationState` | `_NET_WM_STATE_FULLSCREEN` on the active window |
| Mic / camera in use | `LastUsedTimeStop == 0` under `CapabilityAccessManager\ConsentStore` | decided in Phase 2: candidates are `/proc/asound` capture state or PipeWire |
| Idle time | `GetLastInputInfo` | XScreenSaver extension; Wayland sessions use D-Bus (GNOME `Mutter.IdleMonitor`, KDE `org.freedesktop.ScreenSaver`) |
| Cursor position | `GetCursorPos` (polled) | `XQueryPointer` (polled) |
| Window positions (sitting on windows) | `EnumWindows` + DWM extended frame bounds | `_NET_CLIENT_LIST_STACKING` + geometry |
| Light/dark, accent colour | registry `AppsUseLightTheme`, DWM accent colour | xdg-desktop-portal `org.freedesktop.appearance` (zbus) |
| Global hotkey | `RegisterHotKey` | `XGrabKey`; Wayland sessions use the GlobalShortcuts portal where available |
| Tray | tray-icon | StatusNotifierItem (ksni) |
| Single instance + IPC | named mutex + named pipe | Unix socket in the user runtime directory |

### 3.6 Settings UI toolkit comparison (2026-10-22)

Order of comparison follows Roger's rule: stability, performance and compatibility first; when those are close, appearance decides.

| | egui | iced | Slint |
|---|---|---|---|
| Stability | mature; small API changes each release | 0.x, frequent large changes | 1.x with a compatibility promise |
| Performance | good (immediate mode, redraws every frame) | good | good; also a GPU-free software renderer |
| Chinese IME | supported, but immediate-mode toolkits have long-standing gaps (why graphicgene moved to a web UI) | added in recent releases, maturity unconfirmed | supported, to be tested in Phase 0 |
| Animation | simple value animation only; springs and transitions are hand-written | present, basic | declarative `animate`, states and transitions built in |
| Styling | mostly global styles; cute widgets must be hand-painted | per-widget styles | per-element design, close to writing CSS |
| Headless screenshot tests | kittest (works on this machine) | unchecked | software renderer outputs PNG directly |
| Licence | MIT / Apache-2.0 | MIT | GPLv3, royalty-free (with attribution) or commercial |

**Conclusion: Slint.** None of the first three criteria rules any toolkit out, and Slint leads clearly on appearance. Costs:
- The licence must be chosen before going public: GPLv3 open source, or the royalty-free licence with the required attribution (re-read the licence text before publishing).
- Chinese IME is a must-pass item in Phase 0 S3; if it fails, redo this comparison.

---

## 4. Platform support matrix

In Wayland sessions v1 runs under XWayland and **cannot see native Wayland windows**, so some features degrade. The Wayland columns are reasoned and get filled in after Phase 0.

| Feature | Windows | X11 session | GNOME Wayland | KDE Wayland |
|---|---|---|---|---|
| Transparent, always on top, click-through | ✅ | ✅ | verify in Phase 0 | verify in Phase 0 |
| Walking along the work area, lift and throw | ✅ | ✅ | verify in Phase 0 | verify in Phase 0 |
| Sitting on window tops | ✅ | ✅ | ❌ (only XWayland windows are visible) | ❌ |
| Cursor curiosity | ✅ | ✅ | ⚠️ no cursor position over native windows | ⚠️ |
| Full-screen detection | ✅ | ✅ | ⚠️ native full-screen windows are invisible to us; Phase 0 checks whether they cover the pets anyway | ⚠️ |
| Meeting mute, idle detection | ✅ | ✅ | ✅ (D-Bus / audio layer) | ✅ |
| Global hotkey | ✅ | ✅ | only with the portal | only with the portal |
| Tray | ✅ | ✅ | needs the AppIndicator extension | ✅ |

---

## 5. Performance budget

Measured on a mid-range laptop **with the settings window closed**:

| Metric | Target |
|---|---|
| CPU (6 pets, the default, normal behaviour, 5-minute average) | < 1% of one core |
| Memory (6 pets) | < 80 MB |
| Each pet beyond 6 | < 0.2% of one core and < 8 MB (measured in Phase 0 S2) |
| Poke to first visual reaction | ≤ 1 frame |
| Launch to pets on screen | < 1 s |

**Frame rate by state**

| State | fps |
|---|---|
| poked, bouncing, lifted, thrown | 60 |
| walking, normal actions | 30 |
| breathing in place | 10 |
| asleep | ≤ 5 |
| hidden, full screen | 0 (only ~1 Hz state detection) |

- No redraw and no pixel upload when the pose has not changed.
- Capped at 30 fps on battery.
- `cargo xtask bench`: headless per-frame simulation + raster timing, tracked in CI.

**Phase 0 S2, first numbers (2026-10-22)**: `pet-spike --bench` on the development machine (aarch64 VM under QEMU), 256 px stand-in Shiba, default mix (one in six walking, the rest idle, occasional pokes):

| Pets | Draw + BGRA convert per frame | Frames/s | CPU at full clock | Extra memory |
|---|---|---|---|---|
| 6 | 199 + 5 µs | 123 | 2.5% of one core | +2.4 MiB |
| 24 | 197 + 5 µs | 463 | 9.4% of one core | +6.2 MiB |

- Memory is far inside the budget (about 0.3 MiB per pet).
- CPU is about **2.5× over** the 6-pet budget, and each pet beyond 6 costs about 0.4% against the 0.2% target. (The VM's measured CPU, 18%, is inflated by wake-up overhead in QEMU and is not used.)
- Still owed from real desktops: present cost, real CPU and memory (spikes/README.md).

**Phase 1 renderer (2026-10-22)**: `cargo run --release -p chonkgene-render --example bench`, same machine, real Shiba rig, animated (breathing, blinking, walking, a poke every 2 s):

| Scale | Window | Per frame | 6 pets (one walking at 30 fps, five idle at 10 fps) | Each extra idle pet |
|---|---|---|---|---|
| 100% | 220 × 192 | 119 µs | **0.95%** of one core | 0.12% |
| 150% | 330 × 288 | 201 µs | 1.61% (over budget) | 0.20% |

What got it there, all in `chonkgene-render`:
1. paths are rebuilt only when the body value changes; line parts (tail, mouth) are expanded to filled outlines at the same time instead of being stroked every frame;
2. runs of parts on one bone (the face) are drawn once into a layer and blitted while the bone only moves; that bone and its children snap to whole pixels on those frames so the eyes stay put on the face;
3. the window fits the pet (220 × 192 at 100%, 37% fewer pixels than 256 × 256);
4. idle breathing runs at 10 fps.

The budget holds at 100% on this machine. If real machines or larger sizes miss it, the next step is caching the body's runs at a few quantised breathing scales.

---

## 6. Security and privacy

1. **No network in v1**: cargo-deny `bans` forbids network crates such as reqwest, hyper, ureq and curl; local D-Bus is exempt.
2. **No global keyboard or mouse hooks**: idle and cursor use polling APIs. Hooks trip antivirus and are a privacy problem in themselves.
3. **No content is read**: meeting detection reads only an "in use" flag and never touches audio; windows are read only for position and full-screen state, never titles; no keystrokes are recorded.
4. **No administrator rights**: writes only to the portable folder, the user config directory and `HKCU`.
5. **unsafe only in `chonkgene-platform`**; every other crate has `#![forbid(unsafe_code)]`.
6. **No external files are parsed**: in v1 every asset is compiled and embedded at build time; the only external files read are our own saves (size caps and bad-file handling in §3.4).
7. **Supply chain**: `Cargo.lock` committed, CI uses `--locked`; cargo-deny checks advisories, licences and sources (crates.io only).
8. **Releases**: built by CI from tags, with `SHA256SUMS`; no UPX. Code signing in §10.
9. **IPC is same-user only**: the Unix socket lives in `$XDG_RUNTIME_DIR` (mode 0700); the Windows named pipe rejects remote clients and admits only the current user. Messages are size-capped RON; anything that fails to parse is dropped.

---

## 7. Testing and verification

| Layer | How | Where |
|---|---|---|
| Behaviour | seeded unit tests: no action more than 2 times in a row, skits always end in making up, no sound while not disturbing, tickle thresholds, shake detection, grudges always end | local + CI |
| Nurturing | property tests: **no event sequence ever lowers affinity**; the daily cap holds; values are unchanged after days offline | local + CI |
| Art data | the `xtask art` build checks (§3.3) run as tests too | local + CI |
| Rendering | **golden PNG snapshots** for each species × body snap point × several palettes; CPU rendering runs headless | local + CI |
| Settings UI | Slint software-renderer PNG snapshots (no GPU) | local + CI |
| UI copy | bubble length cap; no "never write" word from the glossary (§1.11) appears | local + CI |
| Localisation | every key exists in every language | local + CI |
| Platform behaviour | manual checklist: Windows 11, Windows 10, Ubuntu 24.04 GNOME (Wayland and X11 sessions), KDE Plasma 6 Wayland | Roger, real machines |

- CI (GitHub Actions): `ubuntu-24.04`, `ubuntu-24.04-arm`, `windows-2025`; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked`, `cargo deny check`; tags trigger releases (§2.1).
- The development machine is headless aarch64: Windows code is checked with `clippy --target x86_64-pc-windows-msvc` via a stub C compiler; real window behaviour is confirmed by Roger on real machines.

---

## 8. Phases

Each completed phase gets an annotated tag (`v0.1.0`, `v0.2.0`, …).

### Phase 0: technical spikes (not merged to main)

| ID | What | Done when |
|---|---|---|
| S1 | transparency, always on top, per-pixel click-through, per-frame moves (both approaches in §3.2) | results from Windows 11, X11, GNOME Wayland and KDE Wayland written back into §4 |
| S2 | tiny-skia drawing a 256 px test figure × 6 windows, then × 24 to get the per-pet cost | measured CPU and memory written back into §5; if over budget, revisit the rendering decision |
| S3 | Slint settings child process + IPC; frameless rounded window; Chinese IME in the name field | Windows Zhuyin, Linux fcitx5 and ibus all work; the pet process survives the settings process exiting or crashing |
| S4 | portable artefacts | run directly on a clean Windows VM (no VC++ redistributable), Ubuntu 22.04/24.04 and Debian 12 |
| S5 | font sizes and Reserved Font Names | **done** (2026-10-22), results in §1.9 |
| S6 | whether native full-screen windows cover the pets on GNOME Wayland | result written into §4, deciding whether extra handling is needed |

**Done when**: every S1–S6 result is written back here, and every "doesn't work" comes with the route taken instead.

### First beta — `v0.5.0-beta.1`

Roger asked (2026-10-22) for a downloadable test version before the phases are finished one by one. The beta cuts across Phases 1–5 and takes the core of each:

| Area | In the beta | Left for later |
|---|---|---|
| Pets | all six species with placeholder art, standard ↔ mochi body and 2–4 coats each | final art, accessories, seasonal outfits |
| Poking | five zones, three tickle stages, laugh / fed-up routes, lift, shake, throw (placeholder expressions) | keyframed poses (belly-up, playing dead, turned away) |
| Speech bubbles | per-species lines in zh-TW with `{name}` / `{you}` | English |
| Sleep | sleeping by rhythm, idle and menu; snot bubble, sleep talk; gentle wake vs shaken awake; grudges | — |
| Nurturing | affinity (only up, daily cap), snacks (3 a day), level-up note in the menu | tricks, growth diary |
| Pet menu | custom-drawn right-click menu: pat, feed, rename / change look (open settings), sleep / wake, hide all, settings, quit | — |
| Irrational mode | two skits (stare-off and chase, 雞同鴨講), always making up, frequency setting | the other skits, relationship table |
| Settings | Slint child process: adopt and rest pets, name, 「牠怎麼叫你」, coat, body, size, personality, sound, irrational mode, don't disturb, start at login, data location, remove all traces | growth diary tab, quiet hours |
| Don't disturb | hide in full screen, global hotkey | meeting detection, quiet hours |
| Desktop | tray icon (Windows, Linux StatusNotifierItem), single instance, saved settings in the portable folder | sitting on window tops, multi-monitor walking |
| Sound | sound mode with **synthesised placeholder sounds**, volume, rate limit | real recorded sounds (§10) |
| Release | GitHub pre-release from a tag: zip / tar.gz with `chonkGene-data/`, `SHA256SUMS` | code signing |

**Done when**: the archives from the tag's release unpack and run on Windows 11 and Ubuntu 24.04, and every row above works there.

### Phase 1: Shiba prototype (feel) — `v0.1.0`

- The art pipeline (§3.3) works end to end for one Shiba, with placeholder art.
- Procedural animation: breathing, blinking, tail, belly spring.
- Poking: five hit zones, three tickle stages, both the laughing and fed-up routes.
- Lift, throw, land; walk along the bottom of the work area.
- Sound switch, minimal right-click menu, single instance.

**Done when**: Roger has poked it on a real machine and confirms the feel is right; golden PNGs exist; one pet meets the §5 budget.

**Status (2026-10-22, branch `feat/phase1-shiba`)**
- Done: art pipeline with standard/mochi morph targets and four coats; procedural breathing, blinking, tail and belly spring; five hit zones; three tickle stages with the laugh and fed-up routes; lift, shake, throw, landing squash; walking along the work area; golden PNGs; the §5 budget at 100% on the development machine. CI builds the app for all three targets.
- The laugh, fed-up and overdone reactions are **procedural placeholders** (squint and giggle bounce, half-lidded eyes, eyes shut): the real belly-up, playing-dead and turned-away poses need keyframed clips and art (§10).
- Still to do: sound switch, minimal right-click menu, single instance.

### Phase 2: behaviour, sleep and don't disturb — `v0.2.0`

- Full behaviour system (§1.3): action library, needs, daily rhythm, idle, cursor curiosity.
- Sleep and grumpiness (§1.12): snot bubble, sleep talk, the three ways of waking, Shiba zoomies, grudges and calming down.
- Core of mood and affinity (§1.13): only goes up, daily cap; level and state in the right-click menu; rename and 「牠怎麼叫你」.
- All don't-disturb mechanisms (§1.7, §3.5); Linux mic detection is decided here.
- Tray, global hotkey, saved settings, portable mode, start at login, menu entries, remove all traces.

**Done when**: the §7 behaviour tests pass; the "don't disturb" part of the manual checklist passes in all four environments.

### Phase 3: the whole cast and irrational mode — `v0.3.0`

- Art and rigs for Munchkin, Fold, Owl (270° head turn, puffing up), Chick (feather burst, pecking) and Duckling (waddle, follow-the-leader).
- Individual personalities, relationship table, 7 skits (including 雞同鴨講 and the duckling parade), breaking it up.

**Done when**: all six on the desktop at once meet the §5 budget; skit tests pass.

### Phase 4: appearance and the settings UI — `v0.4.0`

- Cute UI widgets (§1.9), adoption screen, live preview.
- Every appearance option (§1.6), seasonal / holiday outfits.
- Follow theme, accent colour, reduced motion; subset bubble font.

**Done when**: Slint screenshot snapshots exist and are compared side by side with the §1.9 mockup; Roger confirms the style.

### Phase 5: nurturing — `v0.5.0`

- Snacks (menu and drag from tray), daily portions, 「吃多了會慢慢變圓」.
- Tricks: learned with affinity, performed spontaneously, requested from the menu.
- Growth diary tab: level and progress, tricks, today's stats, diary in the pet's voice (30 days).

**Done when**: the §7 nurturing property tests pass; diary lines pass the §1.11 length and glossary checks.

### Phase 6: polish and release — `v1.0.0`

- Sitting on window tops (Windows, X11), multiple monitors, HiDPI.
- English translation of the UI.
- Performance tuning on real machines until every §5 target is met.
- Release pipeline (§2.1), `SHA256SUMS`, code-signing decision (§10).

**Done when**: the manual checklist passes in every environment; the archives on GitHub Releases unpack and run on clean machines.

---

## 9. Later (not in v1)

- **Native Wayland**: `wlr-layer-shell` + foreign-toplevel for KDE and wlroots compositors to close the §4 gaps; a GNOME Shell extension for GNOME.
- **Community skin packs**: data only, never code; validated like `xtask art`, with size and resolution caps.
- **Reminders**: knowing your routine (telling you to go to bed), water, pomodoro (all optional).
- **More interactions**: feather-wand cursor mode, ball throwing.
- **Japanese**, more species.

---

## 10. Open questions

| Question | Affects | Decide by |
|---|---|---|
| **Who draws the art** (Roger, a commission, or my placeholders first) | critical path; cuteness depends on it | end of Phase 1 |
| Sound sources (self-recorded, commissioned, CC0) | licensing | end of Phase 1 |
| Owl's favourite snack | snack content | Phase 5 |
| Windows code signing (unsigned triggers SmartScreen warnings; cloud signing is about USD 10/month, to be rechecked) | first-run experience | Phase 6 |
| Whether the GNOME Wayland degradation is acceptable | whether §9's native Wayland moves earlier | after Phase 0 |
| Open-source licence and repo visibility | art, font and sound licences must agree | before going public |
