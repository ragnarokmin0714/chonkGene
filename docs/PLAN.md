# chonkGene build plan

> This document records the design decisions that are settled, and for each phase what to build and when it counts as done.
> Build from this document; when a decision changes, change this document first, then the code.
> Items marked **"verify in Phase 0"** are reasoned, not yet confirmed on real machines.
> UI copy and pet lines are quoted in Traditional Chinese (zh-TW), the product's default language, with an English gloss where it helps.
>
> Last updated: 2026-11-03

---

## 0. Settled decisions

| Topic | Decision | Why |
|---|---|---|
| Platforms | **Windows 10/11 + Linux (x86_64, aarch64)**; no macOS | Requirement |
| Distribution | **Portable, no installer**: one `.exe` on Windows, one binary on Linux, each in an archive | Requirement; see §2 |
| Language | **Rust** (edition 2024, latest stable) | Matches gamegene, graphicgene, etc.; memory safety |
| Pet rendering | **3D, ray-marched on the CPU into a few sprites per pet in the background; every frame is a 2D blit (tiny-skia)**, one small transparent window per pet | Roger played the 2D beta (2026-10-23): pets looked flat, not round. Pre-rendering keeps the frame cost of the 2D renderer (§5); CPU only, so no GPU driver, transparency or VM issues, and golden tests stay deterministic |
| Pet art | **Soft 3D primitives on bones, blended by smooth unions (signed distance fields), with a painted face** | Every seam is round (the plush, 猛獸派對 / 動森 look Roger picked); coat, body, pose and face are parameters; models are text, so no artist tools; costumes later hang on the same bones |
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

v1 has seven species (the 九尾狐 joined on 2026-10-28). A species only sets **default personality**; each individual rolls its own personality near those defaults when adopted, editable later in settings.

| Species | Default name | Ticklish | Patience | Energy | Daily rhythm | Signature moves |
|---|---|---|---|---|---|---|
| Shiba Inu (柴犬) | 阿柴 | high | medium | high | most excited in the morning | belly-up laughing, the Shiba scream, playing dead to refuse walking |
| Munchkin cat (曼赤肯貓) | 奶茶 | high | high | medium | daytime | legs too short to jump up; rolls belly-up and cannot roll back |
| Scottish Fold (蘇格蘭折耳貓) | 芋圓 | low | low | low | lazy all day | "uncle sit", dead-fish eyes, sighing, swatting the cursor away |
| Owl (貓頭鷹) | 咕咕 | medium | medium | low by day, high at night | **nocturnal** (system clock) | 270° head turn, puffing into a ball, dozing by day |
| Chick (小雞) | 蛋黃 | medium | low | high | **early riser**: up first, asleep first | pecks at the cursor and window edges; puffs up and drops a feather when startled; tiny rapid steps |
| Duckling (小鴨) | 湯圓 | medium | high | low | daytime | waddles; lines up behind another pet and follows it; floats on the taskbar as if on water; "呱" when poked |
| Nine-tailed fox (九尾狐) | 小九 | medium | medium | high | **late**: asleep 1:00–9:00 | spins when grumpy; its bell rings as it moves; plays with its tails (and with others using them, §1.4) |

- **How many pets are on the desktop at once** is a setting: **7 by default (every species once; it was 6 until Roger's sixteenth pass), no upper limit**. Above 7 the setting shows a plain hint that CPU and memory use grow with each pet (§5); nothing stops the user.
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

**Touching turns it to you** (Roger, 2026-11-03): any poke, pat, long press or tickle first turns the pet to face you and look at the cursor, and the reaction plays facing you (rolling belly-up toward you, leaning into the hand). See §8 "Roger's fourth pass", item 3.

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
- **Typing and a still mouse** (Roger, 2026-11-03): while the mouse stays still, pets settle down (lie down, scratch an itch, wag); while you type they react too (paws tapping along, a head tilt, curling up next to you). Typing is inferred without hooks: the system's last-input time keeps resetting while the polled cursor stays put. Only that timing is used; no key is ever read (§6).
- **Species idle scenes** (Roger, 2026-11-03): with time on their hands pets do their own thing with a prop and effects: the owl flies to a branch, the duckling swims in a pond, and so on. Props are their own click-through windows (§3.2). See §8 "Roger's fourth pass", item 4.
- **Never frozen** (Roger, 2026-11-04): in any held pose something keeps moving (ears, tail, blinks, the tongue, wings, the belly, the fox's bell, a paw scratching), each species its own way. See §8 "Roger's seventh pass", item 1.
- **Cursor curiosity**: when the cursor comes near, sometimes turn the head or chase it a little (low-rate cursor polling, no hooks). Built 2026-11-12 (§8 "Cursor curiosity").
- **Desktop terrain**:
  - walk along the **bottom of the work area** (top edge of the taskbar), with the taskbar on any side;
  - sit on **window tops**, and hop up onto a low one nearby; ride along when the window is dragged, get thrown off when it is yanked, fall when it is closed or minimised (Windows and native X11; Wayland sessions see §4);
  - walk off the end of a window and land on whatever is below;
  - walk between monitors, with correct per-monitor DPI scaling.

### 1.4 Irrational mode (on by default)

- Only with **two or more** pets.
- **Relationship table**: every pair has a fondness value; some pairs love to squabble (e.g. Shiba × Fold, Chick × Duckling).
- **Skits**: at least 7 in v1:
  - fight over a spot, stare-off, swatting, chasing;
  - one pet is poked into laughter, the other laughs at it → the first chases it in embarrassment;
  - **雞同鴨講** (chick and duck talk past each other): their bubbles go 「啾啾！」/「呱呱。」 back and forth, louder each time, until both shrug;
  - the duckling lines up behind another pet and follows it everywhere; the leader gets annoyed, then gives up and leads a parade.
  - **play-fights** (Roger, 2026-11-03): swatting, wrestling, pouncing, each species reacting its own way; the fox's bell rings and it fights and plays with its tails (§8 "Roger's fourth pass", item 5).
- **Always ends in making up**: grooming each other, sleeping side by side.
- **Frequency**: settings offer 偶爾 / 常常 / 超常 (sometimes / often / constantly); each pair has a cooldown.
- **Breaking it up**: clicking either pet during a skit stops it; both look at you, then make up.
- Paused while any "don't disturb" state is active (§1.7).

### 1.5 Sound mode (on by default)

- One sound set per species: calls, laugh, sigh, snore, landing. Since 2026-11-10 the calls are **real CC0 recordings** picked by Roger (§8 "Roger's fifteenth pass", item 5): cut by `tools/soundbake` from its list `sounds.json` into `assets/sounds/*.wav`, credited in `licenses/sounds.md`; knocks (poke, landing, eating, panting, the bell, wings) stay synthesised. The duckling peeps rather than quacks, by Roger's pick (the 「呱～」 in its lines stays). The Shiba scream is still mandatory.
- **Master volume slider**, plus per-pet mute.
- **Rate limit**: a cap on sounds per pet per minute; a poke burst plays one sound.
- Muted automatically in any "don't disturb" state.
- Loud enough and animal-like (Roger, 2026-11-04: the placeholder voices are too weak); several sounds can play at once. See §8 "Roger's seventh pass", item 2.

### 1.6 Appearance

| Item | Detail |
|---|---|
| Name | Chinese input must work (IME, verify in Phase 0); max 12 characters |
| What it calls you (牠怎麼叫你) | How the pet addresses you in bubbles; per pet, max 8 characters. Defaults by species: Shiba 「主人」, Munchkin 「好朋友」, Fold 「奴才」, Owl 「同學」, Chick 「老大」, Duckling 「領隊」, Fox 「小主人」 |
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

**Program icon** (Roger picked it 2026-11-04 from five directions on the design canvas <https://claude.ai/artifact/HavsTSzXHh5Yb4m4NN4KkW>: a Shiba face, a chubby blob, a paw pad with a face, a nest, a mochi on a desk edge)
- **一窩 (the nest)**: the Shiba in front, the Fold and the chick on either side, piled into a woven nest; brown outline `#4B3528`, the pets' own coat colours.
- Sources in `assets/icon/src`: the 256 px drawing, plus 16, 24 and 32 px versions drawn on their own pixel grid (scaled down, the 256 px drawing blurs the faces into blobs). `tools/iconbake` (a developer tool outside the workspace, using resvg) bakes them into `assets/icon/chonkgene-{16…256}.png` and `chonkgene.ico` (16, 20, 24, 32, 40, 48, 64, 256), which are committed; rerun it after changing a source.
- Where it shows: the exe's icon resource (Explorer, taskbar, Alt-Tab; `build.rs` with winresource, which also sets the name Task Manager shows, 「肉肉賴桌」); the settings window (Windows: loaded back from the exe at the title-bar and Alt-Tab sizes for its DPI, `chonkgene_platform::use_exe_icon`; Linux: the 256 px image through Slint); the tray (Windows: one image at 16 px × display scale; Linux: 16–64 px for the StatusNotifierItem host to choose). It replaces the paw drawn in code.
- Not yet: the Linux app-menu entry of §2.1 (「加到應用程式選單」) is still to be built, and will install `chonkgene-256.png` with its `.desktop` file; whether the settings window gets icons of its own (tabs, buttons) is still Roger's call.

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

**Sleeping posture**: each species sleeps in a pose that clearly reads as asleep, never standing: curled into a ball, flat on its side, belly-up, or (birds) sat down puffed with the head tucked; only the owl sleeps upright (§8 "Roger's fourth pass", item 6).

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

**Tricks**: learned with affinity, e.g. handshake (Lv.1), sit (Lv.2), spin (Lv.3), roll over (Lv.4), act cute (Lv.6). Learned tricks are sometimes performed spontaneously and can be requested from the menu. Roger asked (2026-11-03) to command pets (sit, catch a frisbee); whether the basic commands are there from the start or still unlocked by affinity is open (§10, §8 "Roger's fourth pass", item 2). Birds get bird versions (wing wave instead of handshake).

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
| `chonkgene-rig` | 3D model format (parts, bones, coats, marks, face, poses), the posed signed distance field and its surface colours, hit zones | `forbid` |
| `chonkgene-render` | ray-marches poses into sprites on background threads, blits frames, bubble text and the menu with tiny-skia (skrifa for text) | `forbid` |
| `chonkgene-platform` | transparent window presentation, click-through, work area, full-screen / meeting / idle detection, theme, login item, tray, hotkey | **only crate allowed**, every block commented with its reason |
| `chonkgene-settings` | Slint settings UI (`.slint` files), cute widgets, theme; runs as the child process | `forbid` |
| `chonkgene-ipc` | message format and channel between the pet process and the settings process | `forbid` |
| `chonkgene` (bin) | event loop, wiring, saves, audio (rodio); runs the settings UI when started with `--settings` | `deny` (Slint's generated code allows it locally, which `forbid` would reject) |

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
- **Props and effects** (planned, §8 "Roger's fourth pass"): a branch, a pond, a frisbee, ripples or sparkles each get their own small click-through layered window like a pet's, shown only while in use, hidden with the pets in every don't-disturb state, and off under reduced motion where they are only decoration.

> **Verify in Phase 0**: adding `WS_EX_LAYERED` to a winit-created HWND and using `UpdateLayeredWindow`; drawing into a winit-created X11 window from a separate x11rb connection. If either fails, the platform crate creates pet windows itself.

### 3.3 Art pipeline

The 2D SVG pipeline of the first beta was replaced on 2026-10-23 after Roger played it: the pets looked flat. The look spike (`spikes/blob3d`, a GPU prototype) settled the style; the shipped renderer is CPU only.

- **Models** (`assets/species/<id>/model.ron`, embedded at build time and parsed at start): parts are ellipsoids, capsules, tori and cones hung on named bones (`body`, `head`, `front_legs`, `back_legs`, `tail`, `wings`), listed in blend order with a smooth-union radius each, so the pet reads as one soft bean. A part has a colour slot, gloss, fuzz (plush noise), how much the body value widens it, an optional mirror copy, a hit zone, and **marks**: soft ellipsoidal colour patches in the part's own frame (a Shiba's cream face, tabby bands, an owl's face disc) that can also set the hit zone (belly, cheeks).
- **Face**: eyes, brows, blush and whiskers are painted onto the head ellipsoid, the mouth onto the muzzle, in "face space"; each **look** (open, half, shut, happy, laugh, fed up, pout, asleep) is drawn there, so expressions cost nothing to add. Eyes may have a coloured iris (owls, black cats).
- **Poses** (`assets/species/poses.ron`, overridable per model): sit, belly-up, melt, loaf, sulk. A pose turns and shifts bones about their pivots; `rest: true` lowers a bone until it touches the floor and every pose settles the whole pet onto the floor, so poses never hard-code heights for one species' proportions.
- **Coats** fill the colour slots; the body value (標準 → 麻糬) widens parts by their `chonk` weight. Both are parameters, so no art is redrawn per combination.
- **Rendering** (`chonkgene-render`): an orthographic camera tilted 9° down; sphere tracing with per-part bounding spheres; wrapped two-tone light, soft occlusion, a fuzzy rim and fine normal noise for the plush look; a contact shadow; extra rays only on silhouette and depth edges. Each pet renders its nine sprites (pose × look) on up to three background threads; frames squash, breathe, sway and mirror a sprite with one 2D transform, and idle breathing is a copy of a few cached images (§5).
- **Checks**: the model loader rejects unknown slots, zones, bones and degenerate shapes; every built-in model is loaded by a test; golden PNGs cover each pose and look.
- **Costumes, extra poses and decorations** come next on the same bones: a costume is parts on a bone (a hat on `head`), a pose is a table of bone moves, a decoration is a model of its own.

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

**Licence in use (2026-10-22)**: until the project's own licence is chosen (§10), Slint is used under its **Royalty-free Desktop licence 2.0**, whose condition is attribution: the settings window's 一般 page shows Slint's `AboutSlint` widget. `deny.toml` allows that licence for the Slint crates only.

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
| turning round, hopping, a visible wobble | 60 |
| a tail wagging, waving | 30 |
| breathing in place, glancing, stretching, yawning | 10 |
| asleep | ≤ 5 |
| hidden, full screen | 0 (only ~1 Hz state detection) |

- No redraw and no pixel upload when the pose has not changed.
- Capped at 30 fps on battery.
- `cargo run --release -p chonkgene-render --example bench`: sprite render time and per-frame cost.

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

**3D sprites (2026-10-23)**: same command and machine, Shiba, nine sprites per pet:

| Scale | Window | Sprite, one thread | Per frame | 6 pets (one walking at 30 fps, five idle at 10 fps) |
|---|---|---|---|---|
| 100% | 227 × 201 | 35 ms | 70 µs | **0.56%** of one core |
| 150% | 340 × 301 | 70 ms | 161 µs | 1.29% |
| 200% | 453 × 401 | 110 ms | 260 µs | 2.08% |

- A pet's nine sprites take about 0.3 s of CPU at 100%, on background threads; the pet appears when its first sprite lands. New coat, body or size re-renders in the background while the old sprites stay up.
- Frames got cheaper than the 2D renderer: sprites are cropped to what they draw, and frames that only breathe copy one of six cached images.
- Memory: about 1.6 MB of sprites per pet at 100% (four times that at 200%), plus up to six cached idle frames.

**Lively motion (2026-10-27)**: same command and machine, 18 sprites plus a tail sprite per pose. The bench now runs each pet at the frame rate it asks for for two minutes, so it counts glances, wags and cached frames as they really happen:

| Scale | Sprite, one thread | Idle pet | Walking pet | 6 pets (one walking, five idle) |
|---|---|---|---|---|
| 100% | 37 ms | 14.9 fps, 0.14% | 30 fps, 0.02% | **0.71%** of one core |
| 150% | 80 ms | 0.30% | 0.05% | 1.53% |
| 200% | 135 ms | 0.49% | 0.09% | 2.55% |

- A filtered, turned blit of a sprite costs about 190 µs here (tiny-skia), so motion that changes every frame is what costs. Two things keep it down: a steady walk snaps to 12 beats a step (the tail wags once a step, in time) and each beat is drawn once and copied after; a wag at rest draws only the small tail over a cached tailless body.
- What is left of an idle pet's cost is mostly the short wobble after each gesture's pop (60 fps for a moment).
- Pixels to show: about 10 frames a second for an idle pet (beta 3: about 4) and 26 for a walking one. Under Xvfb, where uploading a frame is slow, 2 pets took 5.6% of a core against beta 3's 2.65%. Real desktops are owed (§0).
- Sprites now drop their full-size image once cropped, so 18 sprites take about the memory 9 did.

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
| Art data | every built-in model loads and passes the loader's checks (§3.3) | local + CI |
| Rendering | **golden PNG snapshots**: the Shiba at each body snap point, coat, pose and look, every species at two bodies; hit zones read through the frame transform; CPU rendering runs headless | local + CI |
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
| Desktop | tray icon (Windows, Linux StatusNotifierItem), single instance, saved settings in the portable folder | (window tops and multi-monitor walking came in the terrain work, §8) |
| Sound | sound mode with **synthesised placeholder sounds**, volume, rate limit | real recorded sounds (§10) |
| Release | GitHub pre-release from a tag: zip / tar.gz with `chonkGene-data/`, `SHA256SUMS` | code signing |

**Done when**: the archives from the tag's release unpack and run on Windows 11 and Ubuntu 24.04, and every row above works there.

**Status (2026-10-22, branch `feat/beta-1`)**: every row above is built and covered by tests where logic allows (affinity property test, skits, lines, saves, IPC, golden PNGs for pets, bubbles and menu, headless settings screenshots). **Not yet run on a real desktop**: the development machine has no display, so window behaviour, the tray, the hotkey, sound output and the settings window under an IME are owed to the first test on Windows 11 and Ubuntu. The release workflow (`.github/workflows/release.yml`) builds the archives from a `v*` tag.

### 3D pets — `v0.5.0-beta.2`

Roger played beta 1 (2026-10-23) and found the pets not cute enough: not round, too flat. He chose 3D in the direction of 猛獸派對 (Party Animals) and 動森 from a look spike, and asked to plan costumes, poses and decorations afterwards.

| Area | In beta 2 | Next |
|---|---|---|
| Models | all six species as soft 3D models with their beta-1 coats | tuning each species with Roger |
| Poses | sit, belly-up (giggling), melt (厭世), loaf (asleep), sulk (turned away) | waving, dangling while lifted, playing dead, eating |
| Faces | open, blink, shut, happy, laughing, fed up, pout, asleep | — |
| Costumes | the bones and sockets they will hang on | hats, scarves, bows (§1.6) |
| Decorations | — | cushions, beds, bowls as their own small models |

**Status (2026-10-23, branch `feat/3d-pets`, version `0.5.0-beta.3`, superseded by beta 4 below)**: built and tested headlessly; beta 2 left afterimages behind moving pets (a cached idle frame copied the previous frame), fixed in beta 3; the app ran under Xvfb, where it also showed an event-loop busy spin present since beta 1 (fixed) (goldens, hit zones, the settings preview and adoption list); the frame budget is met (§5). Not yet seen on a real desktop. The tail no longer wags (it is part of each sprite); a separate tail layer can bring it back.

### Lively motion — `v0.5.0-beta.4`

Roger played beta 3 (2026-10-23): cuter, but walking, idling and poke reactions all felt too stiff. Beta 4 adds:

| Moment | Before | Beta 4 |
|---|---|---|
| Walking | the sitting sprite sliding along, a small bob | two step poses turned the way it goes, a bounce and a rock onto the planted foot each step (chick and duck waddle most), a small squash on each footfall, the tail wagging in time |
| Turning round | mirrored in one frame | squash-flip: squeezed thin and a little taller, mirrored, filled out, with a small hop |
| Idle | breathing and blinking | every 5–12 s a glance left or right, a 伸懶腰 (both paws or wings up), a yawn (eyes squeezed, mouth an O, a head shake after) or a wave; short bursts of tail wagging; a wave hello on start |
| Poking | a squash when the finger lets go | it squishes in while the finger is down and pops back into a stretch when let go; a poke in the bottom makes it jump; giggling rocks side to side; sulking flicks its tail and huffs |
| Pats and snacks | the happy face | a double hop when it cheers up |

How it is built:
- New poses in `assets/species/poses.ron`: `walk_a`, `walk_b`, `look_left`, `look_right`, `stretch`, `yawn`, `wave`. A pose can move one side alone with `"<bone>.mirror"`, and mirrored copies now do the mirror image of a move (written for the +x side).
- Tails live on a `tail` bone in every species (birds got one) and render as their own small sprite per pose, wagged in 2D about where they join the body, behind or in front of it as the pose has it.
- `Animator` (chonkgene-core) gained the walk cycle, the turn, presses, hops, gestures and wag bursts; `PoseParams.act` picks the pose and `PoseParams.beat` marks frames that repeat.
- `cargo run --release -p chonkgene-render --example motion -- <species> <dir>` renders the frames of each moment for checking motion without a desktop.

Limits: the Shiba's curled tail and the cats' tails are mostly hidden while sitting facing you, so their wag shows when walking, sulking or rolling about. Each gesture is one held pose with a pop into and out of it, not a keyframed clip.

**Status (2026-10-27, branch `feat/lively-motion`, version `0.5.0-beta.4`)**: built and tested headlessly (core motion tests, goldens for every new pose, cached frames match fresh ones); ran under Xvfb; within the §5 budget at 100% on this machine. Not yet seen on a real desktop.

### Real-time 3D on the GPU

Roger played beta 4 (2026-10-27): head turns, walking, lying down and above all turning round still looked unnatural, and the shadow was flat. All of it came from drawing with a few pre-rendered poses warped in 2D: they cannot turn in 3D, move between poses, or keep the shadow on the floor. He chose real-time 3D on the GPU, with the sprites kept for machines without one.

| Moment | Sprites (beta 4) | GPU |
|---|---|---|
| Turning round | squash-flip of a flat picture | the pose blends into its mirror image: a 3D turn through facing you (0.3 s) |
| Head turns, lying down, rolling over | a cut, hidden by a pop | every bone blends over 0.22–0.45 s |
| Walking | two step pictures in turn | the legs swing from one step pose to the other |
| Hops, squash, lean | the whole picture moves, shadow and all | the pet moves in 3D; the contact shadow stays on the floor, smaller and fainter |
| Tail, waving | a tail sprite turned in 2D; the body rocks | the tail wags on its bone; the paw waves |

How it is built:
- `chonkgene-render`: `gpu` + `pet.wgsl` port the CPU ray-marcher (two passes: one ray per pixel, four more at edges), so GPU and sprites look alike (`tests/gpu.rs` compares every pose and face). `GpuRenderer` blends poses (`Pose::blend`, `Pose::mirrored`); `Renderer` picks it when `shared_gpu()` finds a real GPU, an integrated one included; otherwise `SpriteRenderer`. `CHONKGENE_RENDERER=sprites|gpu` overrides.
- Frames that repeat (breathing in place, a steady walk's beats) are cached, so idle pets rarely use the GPU. The app keeps 30 fps while a blend runs.
- Hit-testing traces one ray on the CPU through the same pose and frame.
- `chonkgene --gpu-check` (also `--example gpu`) writes a report and sample frames next to the program: the adapter, GPU-vs-CPU difference and cost per frame at 100/150/200%.

First numbers from Roger's machine (Intel UHD Graphics 770, 2026-10-27, issue #1): about 9 ms a frame at 100%, of which about 4 ms was waiting for the readback, so one pet moving all the time would have kept the GPU about a quarter busy and stalled the UI thread. Since then:
- frames are sent without waiting and picked up on a later call (one frame late), so the UI thread never waits on the GPU and a busy GPU skips frames;
- breathing at rest is drawn in 2D over a cached frame (1.5% stretch), a wag at rest is cached in 3° steps, and the cache drops its oldest frames instead of everything on a change of pose;
- with frames that blend, the pops that hid cuts between sprites are off (`Animator::pops`);
- the bounding sphere follows the frame exactly instead of being 1.5 times too big.

A **GPU budget** caps the pets' share of GPU time (default 25% since 2026-11-06, was 10%; `CHONKGENE_GPU_BUDGET=<percent>` to change): GPU time builds up at that share and every frame sent spends what it is expected to cost; a frame it cannot pay for waits, so motion drops frames rather than the GPU share rising. Costs are measured with GPU timestamps where the GPU supports them (Intel and llvmpipe do) and guessed high otherwise.

`--gpu-check` now also simulates an idle and a walking pet. On this machine (llvmpipe, for proportions) an idle pet sends about 3.4 frames a second to the GPU and a walking one about 1.8; the rest come from the cache. At Roger's measured cost that is roughly 1% of a UHD 770 per idle pet; his next report gives the real figure.

### Desktop terrain

Roger asked for terrain to matter (2026-10-27): standing on windows, falling off edges, reacting when windows move or close, and walking across monitors (Windows and X11; under Wayland only XWayland windows are visible, §4).

- `chonkgene-core::terrain`: ledges are each monitor's work-area bottom plus the top edges of windows where no window in front covers them, and with a pet's height of room above (a maximised window's top is not one). Pure and tested.
- `motion::Body` walks along ledges, walks off their ends and falls, lands on the first ledge below, rides along when its window moves (thrown off above 1500 px/s), falls when the window closes or minimises, and turns back at gaps between monitors. Idle pets now and then hop up onto a window top within reach.
- `chonkgene-platform`: `Shared::windows` lists windows front to back. On X11 it reads `_NET_CLIENT_LIST_STACKING` (normal, dialog and utility windows on the current desktop, frame extents in, GTK shadows out), and the server's stacking order when there is no EWMH window manager. On Windows it uses `EnumWindows` (visible, not minimised, not cloaked, not tool windows, not the desktop or taskbar) with DWM frame bounds. `Shared::work_areas` gives one work area per monitor (RandR on X11, `GetMonitorInfo` on Windows).
- The app reads the terrain every 400 ms, or every 100 ms while a pet is on a window or in the air.
- Checked under Xvfb with a real window: pets hopped onto it, rode two moves exactly, and fell when it closed. The Windows side is type-checked only; it is owed a run on Windows.

### Pets on all fours: skinned meshes

Roger played the gait walk (2026-10-27) and still found it stiff: a body standing upright cannot walk like a cat or a dog. He wanted the body level with the ground, legs stepping in turn, a tail that sways with the steps, and later wobbling bellies, bottoms and chins, flying, ball games and fights. Live2D was considered and dropped (it needs hand-drawn art per pet and a paid runtime; the pets are already 3D). Instead the SDF models are baked once into a mesh and drawn by a raster pass, which costs a fraction of ray-marching, so every frame can be a new pose.

| | Ray-marched (above) | Skinned mesh |
|---|---|---|
| A frame | a ray per pixel through every part | one raster pass over a mesh baked once |
| Roger's UHD 770, 100% | 7.95 ms GPU | 1.15 ms GPU (Shiba walking, nothing cached) |
| Poses | blends of a few posed poses | worked out every frame from the walk |
| Tails, bellies | a bone each | bone chains with springs; jiggle bones |

How it is built (all in `chonkgene-render`):
- `assets/species/<id>/quad.ron`: the pet on all fours, in the same model format (one bone per leg, tail chains named `tail…`, `jiggles:` for soft spots). The Shiba has one (its upright `model.ron` stays for the sprites); the fox has only this one. Tail shapes come from `shiba/gen_tail.py` and `fox/gen_tails.py`.
- `mesh`: bakes the SDF into a mesh by surface nets (0.025 cells, 0.2–0.8 s on the CPU): per vertex the coat colour, occlusion, where it sits on the face (the face is still painted per pixel, `paint.wgsl`, shared with `pet.wgsl`), its hit zone, and four bone weights that fade across each seam's blend.
- `skin`: the skeleton and `Walk`: a four-beat walk with planted paws, the spine rocking, a 3D turn through facing you; tails sway as a wave run from root to tip; belly, bottom and chin on springs (a poke, a plop into a sit, a waddle); sitting tips the body back about the front paws, and several tails fan out behind on both sides of the head.
- `mesh_gpu`: one 4× multisampled pass, the shadow first; `mesh_frames::MeshRenderer` drives a `Walk` from the animator's params and answers clicks (the nearest vertex under the cursor) and speech-bubble anchors (the top of the head).
- `Renderer::new` picks the mesh on a real GPU when the species has a four-legged model; other species stay ray-marched; without a GPU everyone is drawn from sprites (the fox's sprites come from its four-legged model and its tails are cut at the edge of the upright canvas while walking).

Checked: the mesh matches the ray-marcher at rest (silhouettes overlap 98.9% for the Shiba and 98.7% for the fox, colours 0.4 and 0.55 apart out of 255), planted paws slide under 0.5 px, the tail tip stays clear of the body, and the app ran under Xvfb with both pets walking. Owed: Roger's look on Windows, the fox's cost on his UHD 770, frames drawn without waiting (now each frame waits for its readback, about 0.5 ms there), baking off the UI thread, lying down, and the other species.

### Next: Roger's feedback on the mesh pets (2026-10-29)

Roger played the mesh build on Windows (`feat/mesh-pets`) and asked for these to be noted and fixed later.

1. **Cost (issue #1, `--gpu-check`, Intel UHD Graphics 770).** A walking pet, every frame a new pose:

   | | Bake | GPU a frame, 100% | Share at 30 fps, 100% | Share at 30 fps, 150% |
   |---|---|---|---|---|
   | Shiba (35k vertices, 20 bones) | 0.20 s | 0.94 ms | 2.8% | 3.8% |
   | Fox (68k vertices, 47 bones) | 0.76 s | 2.84 ms | 8.5% | 11.7% |

   Two walking foxes are over the 10% budget, and mesh frames do not go through the budget yet (`MeshRenderer` draws and waits every frame). **Done (2026-10-30):** mesh frames go through the GPU budget and are sent without waiting (`MeshCanvas::try_submit` / `collect`; picked up one call late, a frame the budget cannot pay for waits and the next one carries a newer pose; costs learned from GPU timestamps); the mesh bakes on its own thread (`MeshRenderer::poll`, the pet shows when it lands; a coat or body change bakes again while the old mesh keeps drawing); the fox bakes on a coarser grid (`bake_cell: 0.03` in its `quad.ron`, 53k vertices instead of 77k, bake about 40% quicker, hardly visible; the Shiba stays on 0.025). Hits and the bubble go by the pose of the picture shown. Still to check on Roger's UHD 770: the real GPU cost with two walking foxes (`--gpu-check`).
2. **Pets vanish after 「全部收起來」.** Reported as: 縮小視窗時，將寵物全部收起來再放出，寵物實體會全部看不見，只剩視窗在移動 — after hiding all pets (Ctrl+Alt+P or the menu) and showing them again, the pets cannot be seen though their windows still move. 「縮小視窗」 is the settings window being minimised (Roger, 2026-10-29). Not reproduced yet; to check on Windows, with mesh and ray-marched pets:
   - whether a pet's picture is sent again after its window is shown (`set_visible` only marks the pets due; a frame is presented only when the renderer reports a change, and a hidden layered window may have dropped its picture);
   - whether minimising the settings window (a child process) flips `fullscreen_active` (`SHQueryUserNotificationState` reporting busy), which hides and re-shows every pet through the same path.

   **Likely cause fixed, not yet seen on Windows (2026-10-30):** `set_visible` only marked the pets due, and `draw` presents only when the renderer reports a change, so a pet at rest showed its window again without a picture; a hidden layered window may have dropped it. Every pet now repaints once when its window is shown again (`Pet::repaint`). The second suspicion (minimising the settings window flipping `fullscreen_active`) goes through the same path, so it is covered too; both still owe a Windows run.
3. **The other species are still upright.** *Done (2026-10-30), in the same render tests; Roger's look owed.* Munchkin and Fold have a `quad.ron` (a long low loaf on tiny legs; a round teddy with folded ears and its tail wrapped round its side), their tails made by `gen_tail.py` through the shared `assets/species/tailgen.py` (chains of round cones along a path; `splice.py` pastes the output in). Leg swing now scales with leg length (`LEG_REF`) so short legs keep pace with the window. Chick, duckling and owl have a `biped.ron`: the same bird as `model.ron` with a bone for each leg and wing, walked by `waddle::Waddle` (legs in turn, the body rolling onto the planted leg and rising over it, the head steadier, wings swinging out with each step, the tail wagging, belly / bottom / chin on springs; the step rate follows from the leg length and swing so a planted foot keeps pace, within about 2 px). `waddle::Mover` is either walk; `skin::mesh_model` loads whichever a species has, and `MeshRenderer` and `--gpu-check` cover all seven. The sprites for these species still come from `model.ron` (no GPU). Idle gestures work on the meshes too (`Gesture` in `skin.rs`, fed by `MeshRenderer` from the animator's `act` and `anim::gesture_secs`): a stretch is a play bow (rump up, head forward) or spread wings, a yawn tips the head back, a wave raises a front paw or a wing. Still to do on the meshes: the birds' flying, the owl's 270° turn and puffing up, and lying down (asleep, they sit).

Looks (2026-10-30; Roger likes the Shiba's tail; the rest is the fox). **Items 4–9 are done**, checked headlessly on llvmpipe; Roger's look on Windows is owed:

4. **Shiba tail: smoother.** Keep the open doughnut, but the joints between its segments still show. Options: more, shorter segments with wider blends, or one curved part (an arc of a torus) skinned along its length so the ring is a single smooth surface.
5. **Fox ears: bigger and less pointed.** Wider at the base, a rounded tip instead of a sharp cone.
6. **Fox tails: little flames.** Sway like small flames flickering, but smooth (no visible segments or lumps), and long enough that, sitting, the fan's tips rise a little above the ear tips.
7. **Fox: a fluffy tuft between the chest and the bell.**
8. **Fox bell: bigger, with more detail** (for instance the slit, a cap where the cord meets it, a highlight).
9. **Fox forehead mark: less plain.** It is a single red dot now; a proper flame (or a richer kitsune mark) instead, which likely needs a painted shape rather than an ellipsoid colour mark.

   What was done:
   - 4: `gen_tail.py` now makes 16 short segments blended 0.1 wide (the joints no longer show). The tail wave's lag per bone is capped so the whole tail still spans `WAVE_SPAN` radians (skin.rs): a tail of many short segments ripples no more than one of a few.
   - 5: ears 0.38 wide at the base with a 0.13 rounded tip, a little shorter (0.6).
   - 6: each tail is four round cones whose radii meet (a thin root, fullest a third up, a point that flicks back in and up), so one smooth surface; longer (0.4–0.64 a segment) so sitting, the top tips rise a little above the ear tips. Still four bones a tail (the fox is at 47 of `MAX_BONES` = 64).
   - 7: a throat tuft (a cone each side and one in the middle) between the ruff and the bell.
   - 8: bell 0.14 radius with a cap, a hanging bit of cord, a groove, a slit and a hole, and a highlight. Kept high enough that it stays out of the shadow's paw set (`mesh.rs`).
   - 9: a painted flame (`face.forehead`, a colour slot; `face.rs` `flame` and `paint.wgsl`, tested to agree on the CPU and the GPU), with a leaning tip, a lick and a paler core. Colour marks baked into vertices were too coarse for it (0.025 cells).
   - Cost: the fox bakes to about 77k vertices (68k before), so item 1's "make the fox cheaper" now has a little further to go.

### Roger's second Windows pass (2026-10-30)

Roger ran `feat/mesh-pets` on Windows again and reported seven things; each is fixed here, checked on this machine only (Windows type-check, tests, renders), so each still owes a look on Windows:

1. **Pets turn into plain windows after hiding them (the settings window minimised, 全部收起來, back again).** Cause found: winit 0.30 rewrites the whole extended window style from its own flags whenever one changes (`apply_diff`, on show and hide among others), which drops the `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` the presenter added. The window stops being layered and shows as an empty plain window. `Presenter::restyle` puts the styles back; it runs after every `set_visible` and before every present (with the repaint on show from earlier).
2. **Every pet now and then slides while walking.** Three causes: idle mesh pets drew at 30 fps and used up the GPU budget, so walking pets' frames were skipped (legs still while the window moved); the legs ignored the animator's pace (zoomies run four times as fast); and the turn at a screen edge took half a second while the window already went the other way. Now walking frames skip the budget and are waited for if the last is late, idle mesh pets draw at 15 fps (`Renderer::min_fps`), `PoseParams::pace` reaches both walks (quicker and longer steps, `tempo_split`), and walking turns and starts and stops are quicker (`TURN_HZ`, `SPEED_UP`).
3. **The fox's tails are not soft-edged like little flames.** Each tail is now a teardrop (thin root, plump a third of the way up, an ogive in to a rounded tip) sampled into 16 round cones, four to a bone, blended only a hair (a wide smooth union swells where cones overlap, which showed as rings). The tips still rise just above the ears when sitting.
4. **No goodbye from the pet menu.** 「說再見…」 is in the menu (a waving-hand icon); the first click asks 「真的要說再見嗎？」, the second says goodbye for good.
5. **The settings button sometimes does nothing.** It did nothing whenever settings was already open, minimised or behind other windows. Settings now gets `show` / `show <pet>` on its stdin and restores itself, comes to the front and opens on that pet. Coming to the front is `chonkgene_platform::bring_to_front` on the window's handle (Slint's `raw-window-handle-06`), allowed by the pet process with `AllowSetForegroundWindow` as it writes the line. (Slint's `unstable-winit-030` would have done it too, but it switches on winit's Wayland support and its unmaintained `ttf-parser`, which failed cargo-deny.)
6. **A pet just called out cannot be seen.** Same cause as 1 (a new pet's window is hidden and shown as it is placed).
7. **The cats' legs look short.** Both cats stand taller with longer legs (Fold like the Shiba; the Munchkin still short-legged, but its legs now show under a slimmer tummy); a front leg leaning back as the pet sits is lifted by as much as the lean would sink its paw.

### Sitting, sleeping and eating on the meshes (2026-10-30)

Roger: the Fold's tail sat oddly, pets slept standing, eating had no motion of its own, and pets never seemed to sit.

- **Sitting.** It worked, but only while a feeling showed (a few seconds), so a mesh pet stood about all day where the sprites always sat. Now a pet that has stood still doing nothing for `IDLE_SIT` (1.5 s) sits down, and gets up quickly when it walks off; gestures are done standing.
- **Sleeping.** Asleep it now lies down (`Walk::lie`): the body sinks until the belly rests on the floor, front legs forward along it, hind legs folded, the head down on the paws, the tails hardly stirring. A bird settles down, tucks its head and folds its wings.
- **Eating.** `Act::Eat` (`anim::EAT_SECS`, three bites of `BITE_SECS`): a snack from the menu stops the pet (no happy hop, no walk off until it is done), its head dips for each bite (a bird pecks) and comes up chewing, mouth opening and closing, over a cookie on the floor under its nose that loses a bite each time (`snack_overlay`). Sprites show a happy sit.
- **The Fold's tail** now comes out of the middle of the rump, held low behind and curving up at the end. Tail swish is shared between turning about z (a tail held up) and the vertical (one held out behind), from which way the tail's root points, so a tail held out behind sways too.
- A frame's readback now tells "still mapping" from "failed", and waiting polls again for a moment before giving up (two threads polling one device could make a waited-for frame look lost).

### Roger's third pass (2026-11-02)

Roger played `feat/mesh-pets` again and reported five things; each is fixed here and checked on this machine (tests, llvmpipe renders, an Xvfb run), so each still owes a look on Windows:

1. **Pets walking along stop dead, very unnaturally.** The window went from full pace to standing in one frame (and the legs stopped in a sixth of a second), and a stopped pet then drew at 15 fps while it turned to face you. Now a walk gets going and comes to a stop over `WALK_RAMP` (0.5 s, quicker at a faster pace) in `Body` (`motion.rs`, `Body::speed`, ground speed `gait(speed)`); the legs ramp the same way (`SPEED_UP` from `WALK_RAMP`, times the pace), so a planted paw stays put while slowing down (test: within half a pixel on all fours, about 2 px for the birds). A pet keeps facing the way it goes until it has nearly stopped (`STILL_GOING`); it no longer turns round on the spot as it stops (only as it sets off); and while slowing, turning, sitting down, getting up or in a gesture it draws at 30 fps (`Walk::busy`, `MeshRenderer::min_fps`), with frames that keep up with the window as a walking pet's do. Still there: after stopping, turning to face you pivots the paws on the spot (about 6 px at a paw); stepping round would need a turn-in-place gait.
2. **Sitting hardly shows.** Rests were as short as walks (3–8 s), a pet waited 1.5 s before sitting, and every gesture stood it up again. Now walks last 3–7 s and rests 4–11 s, a pet sits after `IDLE_SIT` 0.8 s, and a glance or a yawn stays sitting (a stretch, a wave or a snack still stands it up).
3. **The fox's window cut off its tails.** The fan of tails reached past the 4.5 × 3.3-unit picture on every side, even standing. A model may now ask for a bigger picture (`canvas` in its file; `Model::canvas`); the fox has 5.8 × 3.8 (337 × 221 px at 100%). A test walks, turns, runs (zoomies), sits, hops, gestures and lies down every mesh pet and fails if any comes within 3 px of the left, top or right edge (the faint edge of the shadow may touch the bottom).
4. **The bubble shakes along with the pet.** It hung from the top of the head of each frame, which bobs with every step. Its anchor now eases toward the head (`BUBBLE_EASE` 0.3 s): a 3 px bob at two steps a second moves it under a pixel and a half, while sitting down still carries it down.
5. **Changing 「同時在桌面上的寵物」 does nothing.** The pet process applied it at once, but only to pets marked as out: a pet adopted with the desktop full, or sent to rest by hand, stayed in 寵物窩 however high the limit went, and the settings list never showed who the limit had sent to rest. Now the limit moves pets both ways (`Pets::fit_to`, run by both processes so their lists agree): lowering it sends the last ones out to rest, raising it calls resting ones out, first adopted first; calling a pet out by hand with the desktop full raises the limit by one. Saves with more pets out than the limit are fitted on start. Checked under Xvfb over the pet process's socket: limit 1 → 3 → 4 brings pets out at once, including one rested by hand.

### Queued (2026-11-02)

1. **The GPU check report in English.** `chonkgene --gpu-check` (`crates/chonkgene-render/src/gpu_check.rs`, also `--example gpu`) writes `report.txt` in Chinese (27 lines carry Chinese text: headings, the per-species mesh lines with the species' Chinese names, units and notes). Roger asked for it in English: write every line of the report in English (species by their English names), keep the numbers and the layout, and update anything that reads the report back (tests, the issue #1 notes) to match.

### Roger's fourth pass: more to do and more to see (2026-11-03)

Roger played `feat/mesh-pets` again and asked for more actions and backgrounds, to make the pets smoother and more interactive. His notes, with what the plan already had and what is new. Steps A–C were built in the fifth, sixth and icon rounds; D–G in the ninth pass (below, "Roger's ninth pass").

1. **沒有 ui/ux icon (no UI/UX icons).** What exists: the pet menu draws an icon per row (`menu.rs`), the tray icon is a paw drawn in code (`app.rs` `tray_icon`). What is missing: the program has no icon of its own (no `.ico` resource in the exe, no Slint window icon), so Explorer, the taskbar and Alt-Tab show the default one for chonkGene and its settings window; the settings window has no icons at all; the cute widgets of §1.9 (paw switches, fish / bone slider thumbs, squashy buttons) are Phase 4 and not built. Plan: one icon set in the §1.9 style (a chubby pet face for the program, line icons with round ends for settings tabs and buttons, the tray paw redrawn to match), embedded at build time (§6.6), the program icon at 16–256 px. Which of these Roger meant is open (§10).
2. **Commands and play: 坐下, 丟飛盤… (sit, throw a frisbee).** The plan had tricks unlocked by affinity and asked for from the menu (§1.13, Phase 5) and ball throwing for later (§9, now moved into v1). Plan:
   - a 「叫牠…」 entry in the pet menu: 坐下 (sit), 趴下 (lie down), 過來 (come here, to the cursor), 轉圈 (spin), 握手 (shake hands; a wing for birds); a pet in a bad mood may take its time, never refuses for good;
   - toys: a frisbee or ball thrown from the menu (or flicked with the mouse); it flies as a prop (§3.2) with the throw physics pets already use; dogs and the fox run and leap to catch it and bring it back, cats bat it about and lose interest, birds chase it a few steps and give up; more pets means a race for it (and a squabble, §1.4);
   - needs on the meshes: lying on command (the sleep pose without sleeping), a leap and a catch, carrying in the mouth.
3. **Tickling: the pet should face me.** The plan had the reactions (§1.2: belly rub melts into a puddle, head pat nuzzles the cursor), but on the meshes they are still faces over the normal pose, at whatever angle the pet stood. Plan: any touch turns the pet to face you (yaw to about 0, head and eyes toward the cursor) within about 0.25 s before the reaction; laughing rolls it belly-up toward you, a pat makes it lean into the cursor, a belly rub melts it into a puddle facing you; it keeps looking at you for a moment after.
4. **A still mouse, typing, and scenes of their own.** The plan had idle detection (§1.3: fall asleep, or come over to look at the cursor; stretch when you come back); only sleep is wired up. New:
   - mouse still (you are reading or typing): pets lie down and scratch an itch (趴下撓癢癢), wag their tails, groom;
   - typing (inferred from timing only, §1.3, §6.2–6.3): paws tapping along, a head tilt at fast bursts, a cat curling up close to where you type, a Shiba falling asleep to the rhythm;
   - species scenes: the owl flies to a branch and perches (birds' flight is still owed from 2026-10-30), the duckling swims in a small pond with ripples (§1.1 already had it floating on the taskbar), the chick pecks at seeds, cats lie in a sunbeam or chase a butterfly, the Shiba chews a bone, the fox chases its own tails among fox-fire orbs;
   - needs: a props and effects layer (§3.2: branch, pond, seeds, sunbeam; ripples, splashes, leaves, sparkles, hearts, dust on landing), all hidden in don't-disturb states and toned down under reduced motion; flight for the birds; swimming for the duckling.
5. **Pets play together, each in its own way; the fox's bell rings and it fights with its tails.** The plan had at least seven skits (§1.4); two are built (stare-off and chase, 雞同鴨講). New:
   - play-fights: swatting, wrestling, pouncing, a tail tug, chasing in turns, each species with its own answer (the Shiba play-bows and does zoomies, the Fold swats once and turns its back, the Munchkin rolls over and cannot roll back, the chick pecks, the duckling quacks and flees, the owl puffs up and stares);
   - the fox: its bell swings on a spring and rings (sound mode) when it runs, lands, is poked or plays; it swats others with its tails, tickles them with a tail tip, and may tuck a small pet under its tails;
   - needs: pets touching each other. Each pet has its own window, so for the length of a contact skit both pets are drawn into one shared window (or their windows overlap with the near one on top); this is the main technical question of this item.
6. **Why do they sleep standing, except the owl? And they hardly ever face me.**
   - Sleeping: lying down asleep was added on 2026-10-30 (`Walk::lie`: the body sinks 0.42 of the leg length, front legs forward, hind legs folded, head down; birds sit down and tuck in), yet Roger still sees them standing. To check first: which build he played, whether the sprite fallback (no GPU, upright `model.ron` with eyes shut) was in use, and whether the pose is simply too slight from the camera. Then give each species a pose that cannot be read as standing (§1.12): the Shiba curled nose to tail or flat on its side, the cats curled into a doughnut (the Munchkin sometimes belly-up), the fox curled under its tails like a blanket, chick and duckling sat down puffed with the head under a wing; the owl stays upright, which looks right. A comparison page of the poses comes before any code.
   - Facing: at rest a pet turns 28° from you (`REST_YAW`), sitting 8° (`SIT_YAW`), walking 72° (`WALK_YAW`), and walks take up much of the time; cursor curiosity (§1.3) is not built. Plan: standing at rest it faces you too, it looks at the cursor when the cursor comes near (head and eyes), glances at you while walking, turns and greets you when you come back after being away, and more of the idle choices are ones done facing you.

**Proposed order**, each step checked headlessly here and then on Roger's Windows machine:

| Step | Items | Why in this order |
|---|---|---|
| A | 6 (sleep poses, facing you) | fixes what looks wrong now; small, all in `skin.rs` / `waddle.rs` and the idle choices |
| B | 3 (touch turns it to you, the real reactions) | builds on A's facing; makes poking feel personal |
| C | 1 (icons) | independent; quick once Roger says which icons he meant |
| D | 2 (commands, frisbee) | needs the props layer, which D builds |
| E | 4 (still mouse, typing, scenes, effects, flight, swimming) | the biggest; reuses D's props |
| F | 5 (play-fights, the fox's bell and tails) | needs two pets drawn together, the hardest technical part |
| G | catnip (below, after the sixth pass) | a prop on the floor, so it needs D's props layer; small after that |

### Roger's fifth pass (2026-11-03)

Roger played `feat/mesh-pets` (01dad28) and reported four things; each is changed here and checked on this machine (tests, llvmpipe renders of every pose through `--example mesh`), so each still owes a look on Windows. This takes on part of step A of the fourth pass (sleep poses); facing you, and the rest of that list, are still to do.

1. **Idling goes on too long; add little actions (a dog digging, scratching, the duckling…).** Rests stay 4–11 s (the third pass made pets sit more), but they are no longer empty: the first gesture comes 1.2–2.5 s after stopping and the next 1.5–4 s after one ends (was 5–12 s), and each species picks from its own habits (`Species::habits`, `Animator::with_habits`) instead of one list for all. New gestures (`Act`): **Dig** (Shiba, fox: nose down, bottom up, front paws scooping in turn, earth piling up and clods flying, `dirt_overlay`), **Scratch** (Shiba, cats, fox: sitting, a hind leg out to the side drumming, head leaning into it), **Groom** (cats, fox: a front paw up to be licked, then wiped over the face), **Shake** (everyone: a quick roll from head to tail), **Preen** (birds: the head turned back nibbling at a wing), **Peck** (chick, duckling: bursts of pecks at the floor), **Flap** (birds). `Act::seated` says which are done sitting (yawn, scratch, groom, preen: the pet sits down for them at once); the rest stand. Sprites (no GPU) show the nearest pose they have. 「鴨會戒划」 was read as preening and paddling: the duckling preens and flaps, but paddling needs water, which comes with the pond scene (fourth pass, item 4); to confirm with Roger. Cost: an idle pet's average frame rate (sprite path) rises from under 18 to under 22 fps (`idle_pets_mostly_run_slow`).
2. **The owl's face goes misshapen when it turns its head to you.** Its head is melted into its body (blend 0.45), so a head turning alone slid the face disc across the body's unmoving outline and squeezed the far eye against the edge. A model's `gait` now has `neck`: the share of a head turn the head takes alone, the body turning for the rest; the owl has 0.35 (others 1.0). Glances, the turn toward you at rest and preening all go through it.
3. **Only the cats, the dog and the owl have a sleeping pose.** Confirmed in renders: the fox's tails stood up as a bouquet over a body lying down, and the chick and duckling only sat. Now:
   - the fox folds its nine tails forward over its back like a blanket (`blanket`, `BLANKET_TIP` 62° at the root as one bunch, each segment drooping on `BLANKET_DROOP`); from the front the tips peek out round its face. Fanning them out to both sides was tried first and dropped: the tails' roots share skin, and pulling them apart stretched it into a flat sheet;
   - the chick and duckling (`gait.tuck` 1.0) sink their heads into their feathers and lay them over on a shoulder, chin down (`TUCK_TURN`, `TUCK_LEAN`), the body a little lower and a wing up over the head; the owl stays upright (`tuck` 0).
4. **Feeding looks unnatural on every pet.** It stood turned away and nodded: a 30° dip left the nose far above the snack. Now a snack takes 3.6 s, three bites of 1.2 s (`EAT_SECS`, `BITE_SECS`): the pet turns to you (`SIT_YAW`) and crouches over the snack (four-legged: sinks `EAT_CROUCH` of its legs, front legs forward; tails swishing faster), and each bite goes down to the snack, nibbles there a moment (the head bobbing), comes up and chews with the head rocking side to side. Birds peck quickly at it (`peck`) and swallow with the head tipped back.

Tests: `a_resting_pet_soon_does_its_own_little_things` (first gesture within 2.6 s, only its own habits, at least five kinds in two minutes, every species), `a_sleeping_fox_folds_its_tails_over_itself`, `sleeping_chicks_and_ducklings_tuck_their_heads_in`, `the_owl_turns_its_body_with_its_head` (the last three checked to fail with the change taken out); `idle_gestures_move_every_mesh_pet` and the edge-of-picture test now run every species' own gestures and eating. `--example mesh` gained `lie`, `look`, every gesture by name, the model's own canvas and `TURN=<deg>` for side views.

### Roger's sixth pass (2026-11-03)

Roger asked why cats, dogs and the fox slept with their heads up, and why touching the pets (squeezing, the grass stalk, chin, belly, tail) had gone. Both causes were found in the code, and everything below is built on `feat/mesh-pets`. It is checked here with tests, llvmpipe renders of every scene (`--example touch -- <species> <dir> [scene]`) and an Xvfb run of the app, so each item still owes a look on Windows. This also covers step B of the fourth pass (touch turns it to you) and the sleeping half of step A.

1. **Sleeping with the head up.** Lying down only tipped the head 16°. Four-legged pets now sleep in one of four poses (`SleepPose`), picked by the species' liking each time a pet falls asleep and changed every 45–90 s (`SleepPose::weights`, `MeshRenderer::sleep_pose`):
   - **chin down**, the head sunk onto the paws, turned 30° so its length shows;
   - **curled up**, chest and hips bent toward each other, the head turned to you lying on its cheek;
   - **on its side**, legs out toward you;
   - **belly up**, paws folded in the air, the head tipped back looking at you upside down.

   The Shiba uses all four, the cats mostly curl (the Munchkin often sleeps belly up), and the fox only lies chin down or curled: on its side or back its nine tails would fan out flat. Birds keep their tuck, and the owl stays upright.
2. **Touching was gone.** The mesh renderer never read `PoseParams::squash` or `sway`, so the squish, the pop and the swing went when pets became meshes. Now (`mesh_frames.rs` `settle`, `dent`; `skin.rs` `Feel`, `Moods`, `Walk::react`; `waddle.rs` `Waddle::react`):
   - **squish**: the whole body squashes under a finger, breathes, leans while held. The flesh under the finger also dents in (mesh.wgsl `press`: deepest on the belly and bottom, deeper on a chubbier pet, the dent's walls shaded as a dent). It follows a kneading finger and springs back past flat with a wobble when the finger lets go;
   - **facing you**: any touch turns the pet to face you within about a quarter second, and the head follows the finger (`Feel::gaze`). A touched tail or bottom is the exception: the pet stays side-on and looks back over its shoulder (`Feel::behind`);
   - **per zone**: pressing the head makes it push up into the hand, nuzzling, eyes ^ ^. A chin scratch lifts the chin, and a dog's hind leg kicks. A cheek: it leans into the hand. A held belly (from about 0.35 s) rolls it belly-up for a rub, a dog kicking. A paw is given and shakes along with the hand (a bird gives a wing, a high five). A tail held goes stiff, and poked it flicks; a bottom poked makes the pet hop and look back. A poke on the head makes it flinch and squint. A press held 0.6 s on the head, chin, cheek or belly counts as a pat (the pet is pleased), not a poke (`HOLD_SECS`);
   - **new zones**: `chin` and `tail`, found at bake time from the skin's bones (`mesh.rs` `refine_zone`: skin moving mostly with `tail…` bones, and head skin below the nose that wobbles with the `chin` soft spot), so no model file changed;
   - **feelings with the whole body** (they were faces over a sitting pet): laughing rolls a four-legged pet onto its back, paws pedalling and the head upside down toward you. The fox laughs sitting up and rocking instead, since its tails would prop it off its back. Fed up (厭世), the pet melts flat with legs splayed and chin down. Sulking, it turns its back, glancing round every few seconds. The birds rock back kicking their feet, sink with wings drooping, or turn their backs;
   - rolled over or melted, the renderer settles the skin onto the floor and centres it in the picture.
3. **狗尾草逗貓 (teasing with a grass stalk).** This was in §9 and never built; it is now in v1. 「拿狗尾草逗牠們」 in the pet menu and the tray takes out a stalk of foxtail grass that follows the cursor (`chonkgene-render` `toy.rs`; app `Toy`). It is a window that lets every click through (`Presenter::set_ghost`: an empty X11 input shape, `WS_EX_TRANSPARENT` on Windows). Its seed head lags behind the hand and swings back to rest, and it is put away by itself after two minutes without the cursor moving. Pets in reach (900 px either side, not below the floor they are on) watch the head and walk to it (`Species::toy_keen`), then play as their species does (`Species::plays_like`): cats crouch bottom-up and pounce, the Shiba and the fox pounce and swat, the chick and duckling peck at it low and flap and hop at it high, and the owl only watches. Under Xvfb both pets walked straight to the stalk, and its window had no input region. Roger wrote 鼠尾草 and confirmed on 2026-11-04 that he meant 狗尾草; the catnip he also liked is below ("Catnip").

Tests: `every_mesh_pet_has_a_chin_and_a_tail_to_touch`, `a_touched_pet_turns_to_you_and_looks_at_the_finger`, `four_legged_pets_sleep_with_their_heads_down`, `laughing_rolls_a_pet_onto_its_back`, `a_finger_dents_the_flesh_under_it`, `pressed_flesh_springs_back_when_let_go` (the last four checked to fail with the change taken out); the edge-of-picture test now also runs every feeling, touches on four zones and every sleep pose; `toy.rs` tests the swing and that the stalk stays in its picture. Not covered: pets without a GPU (sprites) still only squash, and pets do not yet play with each other over the stalk (fourth pass, item 5).

### Catnip (2026-11-04)

Roger liked the idea while confirming 狗尾草: 貓薄荷 (catnip) is a different kind of play from the stalk. The stalk is waved in your hand; catnip is put down and the pets come to it.

- 「放一把貓薄荷」 in the pet menu and the tray drops a small sprig on the floor (taskbar or a window top) under the cursor, as a prop (§3.2); it stays about a minute, its smell drawn as a faint wavy line, then is used up. One at a time.
- Cats in reach walk over, sniff, rub their cheeks on it, flop onto their side and roll belly-up, kick at the air and go glassy-eyed for a while (a happy, silly state, not drunk); some cats do not care, as in life (a per-pet roll at adoption, about one in three indifferent).
- Everyone else stays in character: the Shiba sniffs, sneezes and loses interest; the fox sniffs curiously and paws it once; the chick and duckling peck at the leaves; the owl watches.
- Two cats near one sprig: both roll beside it, or a short squabble over it (§1.4) that ends with both rolling.
- Never a snack: it does not count towards the three snacks a day (§1.13), and affinity rises only by the usual touch rules. Hidden in don't-disturb states; the rolling is toned down under reduced motion.
- Needs on the meshes: rubbing a cheek on the floor, the roll onto the back (the laugh's belly-up roll reused), the kicks; the props layer from step D.
- Alternative if catnip reads as too adult: 木天蓼 (silver vine), the same play with a twig. Decided when it is built.

### Roger's seventh pass: never frozen, louder voices (2026-11-04)

Roger's notes after the green CI builds of `feat/mesh-pets` 799cd2f and `feat/app-icon` cb07b42, for **the next version** (steps H and I below). Both are built on `feat/mesh-pets` (see "Built" after the order table); what follows is the plan as written before.

1. **Pets must never stop dead: whatever they are doing (sitting, idling, smiling, looking back at you…), something keeps moving: ears, tails, the tongue, blinks, a wing shifting, the belly wobbling, the bell, a paw scratching the body, each species its own way.**

   What moves today in a held pose: breathing (`mesh_frames.rs`: the body grows 1.2 % at most, hardly visible), the tails' slow sway, and the belly, bottom and chin settling on their springs after a jolt; gestures (fifth pass) come every 1.5–4 s with stillness in between. Blinking is in the animator (`anim.rs`, every 3.5–5.5 s) but only for the neutral and fed-up faces, so a smiling pet never blinks. Ears, wings, the tongue and the fox's bell (`fox/quad.ron` `bell`, `bell_cap`) never move by themselves.

   Plan: a **fidget layer** under every held pose, separate from gestures:
   - **Channels**, each with its own timer so they never move in step: ears (a flick of one ear, both swivelling toward the cursor or a sound, folding back when fed up), tails (the sway's speed and size follow the mood; wag bursts; a cat's tip curling and flicking; the fox's nine tails rippling one after another), blinks (in every face, smiling ones too: ^ ^ closing; now and then a double blink; cats a slow blink when you are near; the owl one eye at a time), the tongue (out and panting, or a quick lick of the nose), wings (shifting, half-lifting and settling, feathers puffing up and sleeking), the belly (breathing that can be seen, more on a chubbier pet, a wobble when shifting weight), the fox's bell (swings on a spring with every head movement, a soft ring now and then in sound mode), weight shifts and paw shuffles, sniffing (the nose twitching), and **a paw scratching the body** (a front paw at the cheek or chest; birds scratch the head with a foot). The hind-leg scratch stays a gesture.
   - **Rule**: in a held pose at least one channel is always moving, and none is still longer than about 1.5 s; breathing and the tail never stop. It runs while sitting, idling, smiling or in another feeling, looking at you or back over the shoulder, between bites, held in a touch reaction, and in a skit; while a gesture moves the whole body it waits. Asleep it keeps to the quiet channels (breathing, an ear twitch, the tail tip, a dreaming paw twitch).
   - **Per species** (the starting lists; tuned on renders):

     | Pet | Its own fidgets |
     |---|---|
     | Shiba | ears flick and swivel, curled tail wags in bursts, tongue out panting when happy or after running, nose sniffs, front paws shuffle |
     | Scottish Fold | the folded ears twitch a little, tail tip curls and flicks, slow blinks, licks its nose, a front paw scratches the cheek, kneads with its front paws |
     | Munchkin | ears swivel and flick, tail swishes, slow blinks, a tongue left out (blep), short legs paddle, the belly wobbles |
     | Fox | the big ears swivel on their own, the nine tails ripple out of step, the bell swings and rings softly, nose sniffs, licks its muzzle |
     | Owl | slow blinks one eye at a time, the head bobs (judging distance), feathers puff up and sleek, wings shift, toes re-grip |
     | Chick | quick head tilts, little wing flutters, a foot scratches the head, the fluff breathes, the beak opens for a cheep |
     | Duckling | tail waggles, the bill clacks, half a wing stretch, a foot paddles a step, shakes its head |

   - **Mood and personality**: a happy pet wags and pants more; fed up, ears go back and the tail only thumps; sleepy, everything slows; lively personalities fidget more often than lazy ones. Reduced motion makes the fidgets smaller, never none (breathing and blinks stay).
   - **Needs on the meshes**: ear bones exist on the Shiba, the cats and the fox, tail bones on all, wing bones on the birds; new are a tongue (a small pink part on a `tongue` bone, hidden in the mouth until it comes out), the bell on a spring bone, a front paw reaching the cheek or chest, a bird's foot reaching the head. Sprites (no GPU) get blinks in every face, visible breathing and the tail sway.
   - **Cost**: idle mesh pets draw at 15 fps (`min_fps`); a fidget lasts well under a second and draws at 30 fps only while it moves. The idle average must stay within §5's budget (`idle_pets_mostly_run_slow` keeps its limit).
   - **Tests**: in every held pose and feeling, for every species, something besides the tail moves within every 1.5 s over a minute (and without fidgets it would not); each species uses its own channels; blinks happen in every face; reduced motion still moves. Renders of each species' fidgets through `--example mesh` before the look on Windows.

2. **The animal voices are a bit weak.**

   Why, from the code: every sound is still a **synthesised placeholder** (`chonkgene-core` `sound.rs`, from the first beta), built from a few pure sine harmonics with no breath or noise, so it sounds like a beep more than an animal. Each is normalised to a low peak: voices 0.55, sighs and eating 0.35, snoring 0.25 of full scale; the volume slider (default 70) then scales that again, so at the default a voice peaks around −8 dBFS and snoring around −15 dBFS. On Windows `PlaySound` plays one sound at a time, so a new one cuts off the last. Only some lines have a sound, and at most six a minute per pet.

   Plan:
   - **Louder**: normalise by loudness (short-term RMS per sound class), not by a low peak: voices and laughs loud, pokes and landings medium, snoring and eating quiet but clearly heard; peaks up to about −1 dBFS through a soft limiter. The slider gets a perceptual curve, so 100 % is properly loud and the default stays comfortable.
   - **More like animals**: source-filter synthesis per species: a pulse train plus breath noise through formant filters, with each call's own shape: the Shiba's 汪 a noisy burst at the onset; 喵 gliding through its vowels (m, i, a, u); the owl's 咕咕 low and breathy, in a pair; the chick's 啾 a fast sweep up; the duck's 呱 a nasal buzz. Several takes per sound and a little pitch jitter, so repeats are never identical; pitch follows size and body (a chubbier, bigger pet sounds lower).
   - **New sounds** where an animal would make one: a cat's purr while patted, the Shiba panting (with the tongue, item 1), the fox's bell, wing flutters, soft paw pats on landing, a greeting when you come back, the chick's morning call (§1.1).
   - **Several at once**: a small mixer instead of one sound at a time: on Windows a `waveOut` stream (nothing to install, like `PlaySound`), on Linux the ALSA worker mixes; about four voices, summed through the limiter. The rate limit becomes per sound class, so a purr or the bell does not use up the voices.
   - **Recorded sounds** stay the open question of §10. Recommended: the better synthesis first (embedded, no licence to check, any variation for free); CC0 or commissioned recordings can replace single sounds later.
   - **Listening before Windows**: a page with old and new for every species and sound (rendered WAVs), so Roger can pick by ear before it goes into a build.
   - **Tests**: loudness of each class within its target window, no clipping, takes differ, the mixer never clips with four voices.

3. **Remove the `v0.5.0-beta.*` releases.** Removed on 2026-11-04: the local tags `v0.5.0-beta.1`–`.4` here; the GitHub releases and tags (beta.1, .2 and .4; beta.3 was never pushed) are removed by Roger, as pushing needs his credentials. The sections of this plan named after them stay as the record of what was built. The next release is `v0.6.0-beta.1` (§10).

**Order for the next version**:

| Step | Items | Why in this order |
|---|---|---|
| H | 1 (fidget layer) | what looks wrong now; reuses the springs and channels already in `skin.rs` / `waddle.rs` |
| I | 2 (voices, mixer) | independent of H; the listening page comes before any change to the app; the bell and panting sounds wait for H |

Steps D–G of the fourth pass follow.

**Built (2026-11-04, `feat/mesh-pets`)**, checked here with tests and llvmpipe renders; the look and the sound on Windows are still owed.

- **H, fidgets** (`chonkgene-render` `fidget.rs`; `skin.rs` and `waddle.rs` `fidgets`): one fidget follows another with gaps of 0.15–1.0 s (a lively pet waits less: `Personality::energy`, passed through `Renderer::set_fidgets` with a seed per pet), each species from its own weighted list as in the table above; asleep only the quiet ones (a dreaming paw, the tail tip, a ruffle) every 1.5–4 s. Each ear twitches on its own clock (every 1.6–4.5 s, asleep 5–11 s), both ears turn to listen every 2–5 s, lie back fed up or sulking and perk up pleased (a Fold's folded ears less). Ears got bones of their own: the mirrored `ear` part of each four-legged model is now `ear_l` and `ear_r` (checked to give the same shape and colours to the bit). The fox's bell is on a `bell` bone, a spring pendulum that keeps hanging down as the body tips, trails back walking and is knocked into a swing by a shake of the head. The tongue is two new faces, `Look::Blep` (open eyes, the tip out) and `Look::Pant` (^ ^, mouth open, tongue out), on CPU (`face.rs`) and GPU (`paint.wgsl`). Every face blinks, smiling ones in a gesture too (`PoseParams::blink`); cats and the owl blink slowly now and then (`Species::slow_blinks`), the others sometimes twice. Breathing shows: the belly rises and falls (`Mover::breathe`) and the body's breath is 2% instead of 1.2%. Lying about (melted, rolled over) the legs only twitch and the head lolls. Quick fidgets (an ear flick, a sniff, panting) draw at 30 fps, the rest at the idle 15 fps. Not done: the owl's one-eye-at-a-time blink (a face has one look for both eyes), the chick's beak opening (the beak is a part, not painted), reduced motion (there is no such setting yet; when it comes, fidgets get smaller, never none). Tests: `no_mesh_pet_ever_stops_dead`, `every_species_fidgets_its_own_way`, and `fidget.rs`'s own (gaps, quiet asleep, ears out of step). `--example mesh -- fidget <species> <dir>` renders 12 s of it, printing each frame's fidget.
- **I, voices** (`chonkgene-core` `sound.rs`, `chonkgene-platform` `audio.rs`, app `Sounds`):
  - Measured first: by a presence-weighted RMS (`sound::loudness`), the old beeps were not quiet (voices −8.9 dB, laughs −11.9, pokes −16, snoring −23); being nearly pure tones and short, they sounded thin. So the fix is mostly the sound itself, with louder targets as well.
  - Synthesis: a band-limited pulse with breath noise through three moving formants, each call with its own pitch and vowel contours: 汪 a noisy burst gliding "w"→"a", the fox's yips shorter and sharper, 喵 through "m", "i", "a", "u", 咕咕 low and breathy in a pair, 啾 a quick sweep 2.6→4.2→3.1 kHz, 呱 a rough nasal buzz. Three takes of every sound, played in turn; the pitch follows the pet's size and body (`voice_pitch`: bigger or chubbier is lower).
  - Loudness by class, after a soft limiter (peaks at most −1 dBFS): calls −8 dB, knocks (poke, landing, bell, wings) −12 dB, soft sounds (sigh, snore, eating, purr, panting) −17 dB, so each is 1–6 dB above the old.
  - New sounds: a purr when a cat is patted, greetings (hello, welcome back, made up, a level up), the Shiba panting with its tongue out, the fox's bell (a shake of the head, landing), wings (a bird's flutter or ruffle).
  - A mixer: up to four sounds at once (a fifth fades out the oldest over 10 ms), summed through the same limiter, written in 20 ms chunks: `waveOut` on Windows (four chunks queued, 80 ms), ALSA on Linux (60 ms). The rate limit is per class and pet (calls 6 a minute, knocks 10, soft 8).
  - Decided: the volume slider stays linear. A perceptual curve would have turned Roger's saved 70% down by about 3 dB, the opposite of what he asked.
  - The listening page (old and new for every pet): https://claude.ai/artifact/CYcH15WkAMzbRqdSfi1S51. It came after the change to the app, not before as planned; any sound Roger dislikes is a small change in `sound.rs`. `--example voices -- <dir>` writes every sound as WAV; `-- measure <file.wav>...` measures any WAV the same way.
  - Not done: Windows output is checked by clippy for the Windows target only. Whether `waveOut` plays, and how it sounds, is the first thing to try there.

### Roger's eighth pass (2026-11-04)

Roger ran the CI build of `feat/mesh-pets` 63ff852 on Windows.

1. **The taskbar icon had not changed.** The icon work was on `feat/app-icon`, which was not in that build; the taskbar entry is the settings window's (pet windows skip the taskbar). Both branches are now merged into `main` (770f352), so builds from here on carry the icon.
2. **Pets still looked frozen.** In renders driven by the app's own animator (`--example mesh -- life`), sitting toward you hides the tail behind the body. The tail had been the one thing always moving, and the fidgets were too small to read at desktop size. Fixed on `fix/fidgets-show` (aa989eb):
   - a slow drift under every held pose: the head tips, turns and nods, the body sways, each on its own beat, and the birds' wings breathe;
   - bigger ear flicks and swivels, head tilts, sniffs, paw shuffles and weight shifts, and breathing at 3%;
   - the tail's fidgets go on through gestures, and a fidget cut short fades out over 0.3 s instead of snapping back.

   In the renders, the stillest 1.5 s of a pet left alone now moves about 2,500 pixels (Shiba 232 → 2,496, Fold 297 → 2,392, owl 1,433 → 2,593). Side by side: https://claude.ai/artifact/8kx9Fgw9vm5Nn7JYojjoyQ. If Roger's screen still looks stiller than the "before" there, the cause is in drawing on his machine (frames dropped by the GPU budget, or the sprite fallback), not in the motion. Windows release builds have no console, so `--stats` cannot show which yet.

### Roger's ninth pass: smiles that move, and steps D–G (2026-11-05)

Roger asked for everything left in the fourth pass in one go, starting with a new note, and to test it all at once at the end. Built on `feat/play-and-scenes` and checked here with tests, llvmpipe renders and two Xvfb runs of the app driven by XTest (menus clicked, catnip put down, typing faked); everything still owes a look on Windows.

**0. A pet smiling at you held still too long.** Pleased (`Expression::Happy`: a pat, a snack, waking, making up), a pet sits facing you with ^ ^ eyes for `HAPPY_SECS` (3 s, renewed by every pat), and the animator started gestures only for `Expression::Neutral`: only the fidgets and an 8° head rock moved. Now a pleased pet does its species' joys (`Species::joys`, `Animator::with_joys`), the first within about a second, then one every 0.5–1.3 s: a **wiggle** (front down, bottom swinging, tail going hard; birds dance from foot to foot), **two hops** on the spot (`Act::Bounce`), a **spin** right round, cats **knead** sitting, and some of the old ones (a wave, a groom, a flap). In the renders (`--example mesh -- smile <species> <dir>`, `NOJOY=1` for the old behaviour) the stillest 1.5 s of a smiling pet moves Shiba 4,680 → 13,818 pixels, Fold 2,559 → 6,668, chick 2,174 → 13,176. Test `a_pleased_pet_never_just_sits_smiling` (checked to fail with the old rule).

**D. Commands and toys** (`chonkgene-core` `play.rs`; app `app/play.rs`):
- 「叫牠…」 in the pet menu (awake pets) turns the menu to a page of commands: 坐下 (sits and stays 7 s), 趴下 (lies down awake, head up, 10 s), 過來 (walks to under the cursor and sits), 轉圈 (`Act::Spin`), 握手 (`Act::Paw`: a front paw, or a wing, held out and shaking), and 返回. As decided: every pet does them from the start; `obey_delay` is 1.25–1.65 s at level 1 and 0.25–0.65 s from level 5; grumpy, sulking or fed up adds 3–7 s and the line 「……好啦」 in its voice (`Line::ObeyLater`); a mood passes when it gets round to it.
- 「丟球給牠們」 and 「丟飛盤給牠們」 (menu and tray) throw from the cursor toward the side with more room. The thing has a window of its own that lets clicks through (`Prop`, `PropWin`): a ball bounces (keeping 55%), rolls and comes to rest; a frisbee glides (held up by up to 86% of its weight in proportion to its speed), lands and slides. Every pet awake goes after it its own way (`Species::fetch`, `Chase`): the Shiba and the fox run (pace 2.2), jump for it once it is low enough or pick it up, carry it back to where it was thrown from (drawn in the mouth over the pet's picture, with sparkles) and drop it there, pleased, with 「撿回來了！再丟！」; cats run, bat it on two to four times and lose interest (sitting down to wash); the chick and the duckling chase it 2–4 s, peck at it if they get there and give up (「……呱？跑掉了。」); the owl watches. The first to grab it has it; the rest give up. One ball and one frisbee at a time; left lying about, a toy goes after a minute.
- Not done: flicking a toy with the mouse (it is thrown from the menu or the tray only), the race ending in a squabble.

**E. A still mouse, typing, scenes, effects** (`chonkgene-core` `activity.rs`):
- What you are doing is guessed from input timing only, sampled five times a second (`Watcher`): input with the cursor still is typing (four inputs in 4 s; fast at eight in 2 s); the cursor still 20 s with no input is reading; two minutes with no input is away. Nothing is hooked and no key is read; it can be turned off with 「陪你打字和看書」 (不打擾 tab, on by default; settings field `react_to_input`).
- Reading: now and then a pet lies down awake (`Posture::Lie`) for 20–40 s and sees to an itch (scratch, wash, preen) every 5–10 s; moving the mouse gets it up.
- Typing: pets tap their front paws along (`Act::Tap`) every 5–10 s; a fast burst tips a head right over (`Act::Tilt`); after 30 s of typing a cat walks to under the cursor (near where you type) and lies down by you; after two minutes the Shiba nods off to it (goes to sleep as if put to bed).
- Scenes, one at a time on the desktop, each pet every 2.5–5 minutes when it has nothing on (`Scene`, `Species::scenes`); their things show only while the scene plays: the owl flies up to a **branch** out from the nearer screen edge, perches 40 s (the branch is a ledge in the terrain, `Support::Perch`) and flies back down (`Motion::Fly`, `Body::fly_to`: an eased arc; on the mesh, wings beating and feet tucked, `Posture::Fly`); the duckling swims in a **pond** (`Posture::Swim`: sunk to the waterline, water with rings drawn over its lower half, paddling to and fro, a splash in and out and a shake); the chick pecks at **seeds** that dwindle; cats lie in a **sunbeam** or stalk and pounce on a **butterfly** (the grass-stalk play reused); the Shiba lies chewing a **bone** (`Act::Chew`); the fox spins after its tails among four **fox-fire** orbs, its bell ringing.
- Effects over a pet's picture (`chonkgene-render` `props.rs`, `Pet::compose`): hearts when pleased, sparkles at a catch and a level up, dust at a hard landing, splashes; water, fox-fire and a toy in the mouth. Every thing and effect is a small 2D drawing in the pets' soft style (`--example props -- <dir>` draws them all on one board).
- Not done: flight for the chick and the duckling (they have no scene that needs it), reduced motion (still no such setting).

**F. Playing together** (`skit.rs` `Kind::Play`, `Move`): with irrational mode on, half the director's skits are now play-fights instead of squabbles. Two pets come close enough for their bodies to touch (`CONTACT` 95 px between centres; the windows overlap, the nearer one on top, so no shared window was needed) and take six turns, each in its own way: the Shiba play-bows and does zoomies round and back, the Fold swats once and turns its back, the Munchkin rolls onto its back and cannot roll back, the chick pecks and hops, the duckling quacks and runs off, the owl puffs up and stares, and the fox swings its nine tails at the other (`Act::Swish`, the bell ringing) and tickles it into giggles; anyone else gets a hop of surprise. It ends with both pleased, no making up needed. A pet busy with a toy, a scene, catnip or a command is never pulled into a skit. Not done: the fox tucking a small pet under its tails.

**G. Catnip**: 「放一把貓薄荷」 (menu and tray; 「把貓薄荷收起來」 takes it away) drops a sprig onto whatever is under the cursor; its smell rises as wavy lines that thin out as it is used up over a minute. Cats in reach walk over, sniff, rub a cheek along the floor (`Act::Rub`) and roll onto their backs kicking with glazed eyes (`Act::Bliss`), in turn until it is gone, then are pleased; about one cat in three does not care (`Species::likes_catnip`, fixed by its id as if rolled at adoption) and says so (「……我不感興趣。」). The Shiba sniffs and sneezes, the fox sniffs and paws it, the chick and duckling peck at it, the owl watches. It is never a snack. Not done: two cats squabbling over one sprig.

**New lines** (every species, `assets/lines/zh-TW.ron`): `obey`, `obey_later`, `fetch`, `catnip`, `catnip_meh`, `skit_play`, `scene`.

**Tests**: `play.rs` (the ball, the frisbee, catnip dropping onto a window, retrieving, batting, birds giving up, the second pet giving up, obey delays, catnip liking, scenes), `activity.rs` (typing and fast typing, reading then away, a moving mouse is never typing, no idle clock means no guess), `motion.rs` `a_bird_flies_up_to_its_perch_and_stands_on_it`, `skit.rs` `a_play_fight_is_up_close_in_turns_and_ends_happy`, `props.rs` (every thing fits its window at any spin and moment, used things show less, effects fade out), and the mesh tests now run every species' joys. Renders of every new gesture and posture through `--example mesh` (`paw`, `tap`, `tilt`, `sniff`, `rub`, `chew`, `swish`, `rest`, `fly`, `swim`).

### Roger's tenth pass (feedback only, 2026-11-05)

Roger tested `feat/play-and-scenes` (the ninth pass) on Windows. Nothing below is built yet; first guesses at the causes, from the code:

1. **The ball and the frisbee cannot be played with by mouse.** As noted in the ninth pass, they are thrown from the menu or the tray only: their windows let every click through (`PropWin`, `set_ghost`). Plan: make the ball's and the frisbee's windows take the mouse (not catnip or scene things); pressing picks the toy up, dragging carries it (its speed kept, as `Body::hold_at` does for pets), letting go throws it with that speed; a click on a resting toy knocks it along. The pets already chase whatever is flying or rolling.
2. **The duckling's pond looks wrong, with an afterimage below.** Probably the water drawn over the duck's own picture (`props::water`): a band 0.84 of the window wide from the waterline down to the bottom of the picture, which reaches 8 px below the floor (room for the contact shadow), so a blue block hangs under the pond and moves with the duck while the pond stays put; the mesh's floor shadow is also still drawn under a swimming duck. Plan: draw the water over the duck as the pond's own front lip (an ellipse matching the pond, clipped to it), no shadow while swimming, and check against a screenshot from Roger if it still looks off.
3. **A smiling pet, and a pet in its scene, still look still.** The joys come as separate gestures with gaps between, and during a gesture the fidgets are cut down to light ones (`Hold::Busy`); held scene poses (perched, lying in the sun, chewing, swimming) only get the ordinary fidgets. Plan: while pleased or in a scene, the motion never stops: the tail wags all the time (a cat's tail curls and flicks, a bird's waggles), ears keep flicking and turning, birds' wings flutter and shift, even through gestures; then measure the stillest 1.5 s again (`--example mesh -- smile`, and a scene variant). If it still looks still on Windows while the renders move, suspect dropped frames (the GPU budget) and add a log file, since Windows builds have no console.

Roger's further notes the same day (also not built yet):

4. **The dog should stick its tongue out too.** The Shiba already pants (`Look::Pant`, the `Pant` fidget, a panting sound) but only now and then among its fidgets. Plan: tongue out and panting whenever it is pleased, has just run (a fetch, zoomies, a play-fight) or is playing, and a quick lick of the nose after eating.
5. **Games to play with the pets, next.** New area, to agree with Roger before building. Candidates: fetch with a score (how many throws in a row it brings back), 躲貓貓 (a pet hides behind a window edge and peeks out; click it to find it), a treat dropped from the cursor that pets race to catch, a chase where the pet follows the cursor and you try to get away, a rhythm game of pats. Recommended first: 躲貓貓, as it uses what pets already do on the terrain and needs no new art beyond a peeking pose.
6. **The Shiba chewing its bone holds still, which looks very odd.** `Act::Chew` moves the head only 4° and the chin a little, and while a gesture runs the fidgets are cut to light ones (`Hold::Busy`), so a pet lying with its bone is nearly a still picture (the same cause as item 3). Plan: a real gnaw (the head working side to side and tugging, the front paws holding the bone and shifting, the bone in its mouth turning with the head), the tail wagging all the while, ears moving; the same rule as item 3 for every scene.
7. **Crossing the terrain, when the next window is far away or high up, pets keep making anxious movements.** Likely causes in the code: walking toward something it cannot reach (`walk_to` in `app/play.rs`, the director's `Cmd::Walk`) keeps setting the facing toward the target every frame while the body turns back (a gap between monitors, the edge of the screen, a target on another ledge), so the pet flips back and forth on the spot; and the director's `Cmd::Walk` has no stopping distance at all, so two pets meeting at a point jitter there. Plan: give every walk toward a target a stopping distance and give up (or hop or jump down, if the terrain allows) when the target is on a ledge it cannot reach or it has turned back twice; a test that a pet sent to an unreachable spot settles within a few seconds instead of flipping. To confirm with Roger which moments he saw (fetching, catnip, a scene, a skit, or plain wandering).

**Built (2026-11-05, `feat/play-and-scenes`): items 1–4, 6 and 7**, checked here with tests, llvmpipe renders and an Xvfb run of the app driven by XTest; the look on Windows is still owed. Item 5 (躲貓貓) was built in the eleventh pass; item 7 waits until Roger sees the anxious moments again.

1. **Toys by mouse** (`app/play.rs` `PropWin`, `Grab`; `Thing::grabbed`): the ball's and the frisbee's windows now take the mouse (catnip and scene things still let every click through). Pressing picks the toy up (pets after it stop), dragging carries it with the cursor (its speed smoothed over the last frames), letting go throws it at that speed (at most 2,400 px/s at 100%; slower than 80 px/s it just drops); a click without a drag knocks it along away from the cursor. Either way every pet awake and free goes after it as after a throw from the menu, and a retriever brings it back to where it was let go. Under Xvfb: dragged and flung (`a Ball thrown by hand at (2325, -595) px/s`), clicked (`knocked along`), and the three pets went for it each time.
2. **The duckling's pond** (`props::pond_front`, `pond_oval`): the water over a swimming duck is now the pond's own water, clipped to the pond's oval from a lapping waterline a little in front of the pond's middle; whatever of the duck's picture hangs below the pond's near edge is wiped out, so no blue block (or duck) shows under the pond. The duck paddles 16% of the pond's width either side of the middle (was 25%), stopping and turning round at each end. The mesh's floor shadow is off while swimming or flying (`MeshCanvas::floor_shadow`). `--example props` draws a body sunk in the pond to check it. If it still looks off on Windows, a screenshot from Roger is the next step.
3. **Never still while pleased or in a scene** (`PoseParams::lively`, `Animator::lively`; `fidget.rs` `Out::lively`; `skin.rs` / `waddle.rs` `lively`): a pet is *lively* while pleased, or busy with a scene, a toy, catnip, the grass stalk or a play-fight (not in a mood). Lively, the ears twitch and turn about twice as often, the slow drift goes on through gestures, fidgets come sooner, and on top of everything, gestures or not: a dog's tail (and the fox's) wags hard all the while, a cat's sways quicker and curls up at the tip with a flick every few seconds, the fox's tails lift one after another, the ears keep turning, a bird's wings lift, settle and now and then flutter, its tail waggles and its head bobs. Lying in a scene the tail stirs nearly as much as standing. Test `a_lively_pet_keeps_tail_ears_and_wings_going_through_gestures`: in every second of a long gesture (gnawing, pecking) the tail, ears and wings now turn by (old → new) Shiba 1.5° → 12.4°, fox 4.5° → 34.5°, Munchkin 11.1° → 27.0°, Fold 11.0° → 22.8°, chick 8.0° → 17.4°, duckling 9.6° → 24.7°, owl 8.0° → 17.4°. In renders (`--example mesh -- smile|scene <species>`, `NOLIVE=1` for the old behaviour) the stillest 1.5 s changes, on average from one frame to the next (old → new, pixels a frame at 100%), smiling: Shiba 599 → 1,027, Fold 250 → 509, Munchkin 221 → 295, fox 1,898 → 5,162, owl 317 → 516, chick 496 → 554, duckling 399 → 600; in its scene: Shiba (gnawing) 709 → 940, Fold (sunbeam) 156 → 357, Munchkin 154 → 295, fox 9,782 → 10,630, owl (perched) 211 → 446, chick (seeds) 1,333 → 1,353, duckling (swimming, now also drifting round a little on the water) 295 → 347. The cats facing you hide their tails, so lively and sitting a cat's tail comes round to its side, and every four-legged pet's head tips and nods to its own beat. Smallest gains: the chick (already busy pecking or doing its joys), the duckling swimming and the Munchkin (its short tail mostly stays behind it). A pet lively draws at 30 fps (its motion never rests). Not done: the log file for dropped frames on Windows, if it still looks still there.
4. **The dog's tongue** (`Animator` `run`/`puffed_until`, `PoseParams::puffed`; `Out::pant`): running (pace 1.5 or more: a fetch, zoomies, skits) for over a second leaves a pet out of breath for 3 s after it stops plus as long as it ran (up to 6 s counted). Out of breath, or lively, the Shiba pants with its tongue out all the while, walking too (quick shallow breaths, chin up); not with its mouth full (eating, gnawing). Any four-legged pet licks its nose (tongue out) as a snack ends. Tests `a_run_leaves_it_out_of_breath_for_a_while`, `a_dog_pants_after_a_run_and_a_pet_licks_its_nose_after_a_snack`, `lively_pets_flick_their_ears_more_and_dogs_pant`.
6. **Gnawing the bone** (`skin.rs` `Act::Chew`, now 6 s a go): the jaw works quickly, the head swings slowly side to side and turns over to get the side teeth to it, tugs back now and then with the body rocking, and the front paws shift their grip in turn; the tail wags and the ears move all the while (item 3). The bone drawn in its mouth (and a carried ball or frisbee) turns with the head as it shows on the screen (`Renderer::head_roll`, `props::Look::turn`).
7. **No more flipping on the way somewhere** (`motion.rs` `Body::head_for`, `Heading`, `Terrain::span_at`): every walk toward a spot (toys, catnip, commands, scenes, the grass stalk, the skit director's `Cmd::Walk`) now goes through one function. It stops a set distance short (12% of the window's width; 20% at the grass stalk), and stood still it sets off again only once the spot is twice that far, so a spot moving a little never starts it off again. A spot past the edge of the screens or across a gap between monitors is clamped to where it can get: it walks there, stops and looks across (`Blocked`; a toy out of reach is given up, or put down where it got to if carried; catnip and scenes out of reach end). Turning round, it slows to a stop first and turns at most every 0.8 s. A pet in the air is left alone (before, a walk could be set while it fell). Tests: `a_walk_to_a_spot_stops_short_of_it_without_turning`, `a_pet_sent_somewhere_it_cannot_reach_settles_instead_of_flipping` (no turn at all toward a spot across a gap; one turn toward one past the left edge), `a_spot_darting_about_never_sets_it_flipping`. Not done: hopping up or jumping down toward a spot on another ledge (it stops and looks instead); zoomies and the grumpy spin still turn back and forth on purpose.

### Roger's eleventh pass (feedback only, 2026-11-05)

Roger's notes after the tenth pass (built 2026-11-06, see the end of this section). First guesses at the causes, from the code:

1. **The pond is too flat, its ripples hard to see, and a pet going across it shows its feet, not swimming.** The pond is a thin oval (0.3 of its 50 px height at 100%, so about 15 px tall) with faint white rings (alpha 170, 1.2 px). The duckling only takes `Posture::Swim` in phase 1 of its scene, so walking in and out (phases 0 and 2) its legs show over the water; any other pet walking past over the pond window is drawn standing on it. Plan: a deeper-looking pond (taller oval, a darker inner bank along the back edge, a lighter rim at the front, the water darker toward the back), bigger and brighter ripples, and rings spreading from whatever is in it; the duckling sinks to swim as soon as its feet are over the water and only stands up once out; other pets either go round (hop over it) or wade, drawn with the pond's front over their legs as the duckling is.
2. **The Shiba rolled over laughing: its body does not lie on the floor as its head does, and its speech bubble is hidden.** The roll turns the body about its middle (`react`, `body_h`) and `ground` settles the whole pet by one amount, so the head can touch the floor while the body hangs above it. The bubble hangs from the crown (`Renderer::anchor`), which rolled over is low down inside the picture, so the bubble lands over (or under) the pet's own window. Plan: settle the body by its lowest point while rolled over (measured from the posed mesh, not one number); anchor the bubble above the top of what is drawn while rolled over or lying, and make sure the bubble window stays above the pet's.
3. **The ball and the frisbee should be bigger and not swallowed into the mouth; the ball headed back with the nose, the frisbee caught in the mouth.** They are 26 px and 44 px at 100%, and carried they are drawn at 0.8 of that centred just under the nose (`Renderer::mouth`), so they sit over the muzzle and look swallowed. Plan: about 1.5× the size; carried, drawn in front of the muzzle, held between the front teeth and sticking out to the side the pet faces; a retriever goes after a ball and bumps it back toward you with its nose (a nod that bats it, `Prop::bat`) instead of carrying it, and leaps to catch a frisbee in its mouth in the air.
4. **Over a pet the cursor becomes a hand, and poking and scratching each get an animation of their own.** Now the cursor stays an arrow, and a poke and a scratch differ only in the press and the face. Plan: a hand cursor while over a pet's picture (winit `set_cursor`, `CursorIcon::Pointer` or `Grab` while held; checked on X11 and Windows layered windows); a poke: a startled hop back and a look at the spot; a scratch (held and rubbed): leaning into it, eyes shut, a hind leg kicking (dogs, now only on a good scratch), purring cats kneading.
5. **A smiling pet still stalls, and cannot be interacted with then.** Not reproduced yet. To check: while pleased (`Expression::Happy`, `HAPPY_SECS` renewed by every pat), whether clicks reach it (the X11 input shape is cut from the picture only every 4th frame, `SHAPE_EVERY`; on Windows a pet hopping in its joys may have moved off the spot), whether the mind ignores pokes while happy, and whether the joys' gaps or the GPU budget (dropped frames) make it look stuck. Ask Roger which pet and what he did, and add the log file for dropped frames on Windows so it can be seen.
6. **Pets should play-fight and squabble with each other.** They do with irrational mode on (the director's skits, one at a time), but it may be rare or not read as a fight. To check how often a skit starts and whether Roger's settings have irrational mode on; plan: more often, and a real tussle up close (rolling over each other, a dust cloud with paws sticking out, a chase round each other), still always ending in making up.

   Roger's addition the same day: **the fox casts spells in a fight**, for example freezing every pet in mid-air by magic, "use your imagination". Proposed (a menu of ideas for Roger to cut down; all only with irrational mode on, never while a pet is held, never in full screen or don't disturb, at most every few minutes, always cute and harmless, always ending in giggles or making up):
   - **定身術 (the freeze)**, Roger's own: the fox spins, its bell rings, fox-fire spirals out; every other pet awake rises a little off the floor in a soft bubble of light and hangs there frozen mid-pose (its animation paused, only a slow bob, eyes wide), for 3–4 s; then the bubbles pop with sparkles and they drop softly onto their feet (a landing squash, a little dust). A sleeping pet floats up too, still asleep, in a dream bubble, and is put back down without waking. A click on a floating pet pops its bubble early; a click on the fox mid-cast breaks the spell (as 勸架).
   - **Counters by personality**: the owl puffs up and stares, and the spell bounces back: the fox floats instead, paws paddling; a cat swats at the fox-fire orbs and pops them; the Shiba chases the orbs round and round.
   - **狐火追擊**: the orbs fly at one pet and chase it a few laps; it runs off squeaking, then turns and chases the orbs back.
   - **幻影分身**: two see-through copies of the fox appear and dash off different ways; the others chase the wrong one, and the real fox laughs from behind them.
   - **縮小術**: a puff of smoke and one pet is half its size for a few seconds, its voice higher, then pops back with a bounce (to check: whether the renderer can change scale without a rebake).
   - **變身術**: a puff and the fox is disguised as the pet it is teasing (that pet's shape in the fox's colours, nine tails sticking out behind); the real one stares; the disguise pops when the other pet bumps into it. Heavier: needs the other species' mesh at the fox's coat.
   - **魅惑**: hearts float from the fox to one pet, whose eyes turn to hearts and who follows the fox about for a while, then shakes it off.
   - Lines and sounds: the fox 「定！」/「變！」, the others 「欸欸欸？！」, a shimmering synth chime with the bell.
   - Pieces it needs: a floating motion (held up in place, then let fall), pausing a pet's animation, a bubble and spell-ring effect (`props::effect`), the fox's casting gesture (rising on its hind legs, tails fanned, a paw raised), skits that pick a spell by the cast and their personalities.
7. **Don't forget the tongue.** Roger, the same day: the tongue-out behaviour (tenth pass, item 4: the Shiba panting after a run, pleased or at play; a lick of the nose after a snack) must carry through this pass and show on Windows. Check it there first (if it does not show, suspect the face's `Look::Pant`/`Look::Blep` on the GPU path or the tongue hidden by a gesture), and keep it in the new animations: tongue out rolled over laughing, a blep or a lick leaning into a scratch, panting after a tussle or chasing fox-fire, a lick after a frisbee catch; none with a toy in the mouth.

Roger's answers (2026-11-06): item 5, the stall happens with every pet; item 6, build the plan as proposed (his cut of the spells left to me: 定身術 first); tenth pass item 5, 躲貓貓 it is; tenth pass item 7 (which anxious moments), set aside until he sees it again.

**Built (2026-11-06, `feat/play-and-scenes`): items 1–7, and 躲貓貓 (tenth pass, item 5)**, checked with tests, llvmpipe renders and Xvfb runs of the app driven by XTest (llvmpipe as the GPU); the look on Windows is still owed.

1. **The pond** (`props.rs` `Thing::Pond`, `pond_oval`, `water_paint`, `ripple`, `pond_stirs`; `app/play.rs` `Playground::pond_under`): 250 × 70 px at 100% (was 230 × 50), the oval 0.38 of that tall (was 0.3); the water deep blue under the far bank to light at the front, a dark inner bank along the back, a light rim along the near edge, grass at the ends, rings 2.2 px and bright. Whoever stands in a pond (feet within 0.85 of its half-width, at its level) is in it every frame, not only in the duckling's scene: the duckling swims (`Posture::Swim`) the moment its feet are over the water, going in and coming out too; any other pet wades, the pond's front drawn over its lower legs (`WADE`, 7% of its picture's height); a splash going in; rings spread on the pond from each pet in it. `pond_front` now wipes only within the pond's width, so a pet longer than the pond keeps its legs on the bank.
2. **Rolled over laughing** (`skin.rs` `react`, `ROLL_TUCK`, `ROLL_LIFT`, `UP_YAW`): the cause was the big head, not the settling: rolled over, the head hung about one world unit below the back (the neck is above the body's middle standing, so below it upside down), and the settle by the lowest point put the head on the floor with the body above it. Now, rolled over (laughing, a belly rub, asleep on its back) it lies side-on (`UP_YAW` 55° → 85°), the chin tucked 72° and the head lifted 0.3, so back and head both lie on the floor (llvmpipe renders, Shiba, Fold, Munchkin). The bubble points at the top of the whole picture where that is higher than the crown (`Renderer::bubble_at`), and the bubble's window is raised over the pet's each time it speaks (`Presenter::raise`: X11 restack, Windows `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE)`).
3. **The ball and the frisbee** (`play.rs` `Thing::size` 39 px and 66 × 24 px, 1.5×; `Chase::nose`, `Order::{Leap, Wait, Nose, Brought}`, `Body::spring`; `Act::Nudge`; `props::carried`): a retriever no longer carries the ball: it runs round behind it (or as near as the edge of the screens lets it) and bumps it toward where it was thrown from with its nose (`Act::Nudge`, the ball sent off 0.4 s in, as the nose gets there), again until it is back, then a cheer and a lick of the nose. A frisbee gliding by within reach is leapt for (up to three leaps a throw, the top of the leap where the frisbee will be by then) and caught in the air as it passes the mouth; carried, it hangs by its rim from the front teeth, tipped out the way the pet faces (0.85 of its size), the face clear; a bone is still held across the jaws. Under Xvfb: `noses the Ball on (bump)` ×3, `brought the Ball back`; `leaps for the Frisbee`, `got the Frisbee` with the feet 144 px off the floor. Bugs found there and fixed: the nose's arrival distance was inside a walk's restart distance, so the pet stood 53 px off and never moved; a leap aimed with the time to the top of the leap as the landing, so it always fell short; a frisbee landing while the pet was still in the air ended the chase as "gone far below".
4. **A hand and its own animations** (`app.rs` `LIFT_UP`, `LIFT_ANY`; `anim.rs` `Touch::scratch`, `SCRATCH_PX`; `skin.rs` `Levels::{scratch, startle}`, `kneads`): the cursor is a hand over a pet (the window takes the mouse only where it is drawn) and a closed hand while it is carried; over the ball and the frisbee an open hand, closed while held. A press used to become a lift after 4 px, so a scratch could never happen: now only a pull of 12 px up or 36 px any way lifts it, and rubbing about inside that is a scratch (`Touch::scratch`, 140 px of rubbing for a full one, fading at 1.2 a second). Scratched, it leans into the finger, head tipped against it, chin up, eyes shut (^ ^; a dog with its tongue out), a dog's hind leg kicking, a cat kneading, a cat purring and a dog panting every 2.6 s. A quick poke (under 0.3 s): it starts (head and body jerk back, ears up) and hops back from the finger (6% of its width, 14 px up), still facing the same way, and keeps looking at the spot as before. Under Xvfb: the cursor `pointer` over a pet, still `pointer` while rubbing (not lifted), the hop 14 px back.
5. **The stall** (`gpu.rs` `Budget::spend`, `mesh_frames.rs` `STARVED`; `diag.rs`): the first run of the new log under Xvfb showed pets in a gesture with their picture unchanged for 2.5–4.9 s, a frame held back by the GPU budget every time. The budget is one bucket shared by every pet, and a pet asking less often (or after the others) could lose every time while the rest kept it empty; lively pets ask 30 times a second, so pleased pets made it worse. Now a frame held back a quarter second goes anyway, the budget running into debt (at most a burst deep) that the others then wait out, so no pet ever goes more than about 0.3 s without a new picture; afterwards no stall at all in the log. The clicks themselves always arrived (the log shows every press and its answer while pleased): what looked like no answer was the picture not changing. Test `mesh_frames_wait_for_the_gpu_budget_but_never_starve`. The log (`chonkgene.log` in the data folder, new each run, at most 2 MB, every line also on the console): the GPU and its budget; the event loop woken over 0.3 s late or a turn taking over 0.3 s; a pet that should move (lively, in a gesture, walking) with its picture unchanged for 1.5 s, with what it was doing and why the frame had not come (`budget`, `gpu`); every press on a pet (zone, expression, gesture, motion) and what came of letting go; the director's tussles, casts and freezes. If Roger sees a stall again, that file says why.
6. **Play-fights, squabbles and the fox's spell** (`skit.rs` `mean_gap`, `PAIR_COOLDOWN`, `Phase::{Gather, Tussle, Cast, Float}`, `Kind::Spell`, `Cmd::{Tussle, Cast, Freeze}`; `props.rs` `Fx::{Brawl, Spell, Bubble}`; `app.rs` `Float`): skits come about three times as often (mean gaps 600 / 180 / 60 s, were 900 / 360 / 120) and the same pair rests 3 minutes (was 10). Before the fourth turn of a play-fight the two run at each other and tussle: one cloud of dust rolling about, paws, a tail and stars poking out of it, drawn over one of them while the other is not drawn, 2.6 s; they come out panting. With the fox free, a third of the skits are its 定身術: it swings its tails, the bell rings, a ring of fox-fire spirals up round its feet, 「定！」; 1.3 s on, every other pet awake and free rises 34 px in a bubble of light and hangs there, its animation stopped, bobbing 4 px, for 3.6 s (「欸欸欸？！」), then the bubble bursts in sparkles and it drops onto its feet; the fox giggles, the others are pleased. The owl stares it back: the fox floats instead. A click on a floating pet pops its bubble; a click on the fox (勸架) ends the spell for everyone. Not built from the idea list: sleeping pets in dream bubbles, the cats swatting the orbs, 狐火追擊, 幻影分身, 縮小術, 變身術, 魅惑. `CHONKGENE_SKIT=spell|play|squabble` starts that skit within seconds, for trying them out. Under Xvfb: the Shiba and the Fold held up in bubbles, then down; the tussle cloud cannot be seen there (with no compositor the vanished pet's empty window covers it), only in renders.
7. **The tongue**: laughing rolled over a dog's face is ^ ^ with the tongue out (`Look::Pant`); scratched, a dog's too; a lick of the nose (`Animator::lick`, `PoseParams::licks`) after a frisbee is put down or a ball nosed home; out of breath for a while after a tussle (`Animator::puff`); still none with a toy in the mouth. On Windows still to see.

**躲貓貓** (tenth pass, item 5; `app/play.rs` `Hide`, `Pet::start_hiding`, `hide_step`, `come_out`; menu 「玩躲貓貓」 with a peeking-eye icon, awake pets only): it says so (「我去躲好囉！來找我～」), vanishes in a puff and 1.2 s later sits just past the edge of the screens on the side away from the cursor, facing in, only an ear or the nose round the edge (10% of its width); every 2–3.5 s it leans out to 42% (the head and an eye) for 1.6 s and back. Everything past the edge is cut away, so across a monitor's edge nothing shows on the next screen; its bubbles stay quiet while it hides. A click on what shows finds it: it hops out into sight, 「被你找到了！」, hearts, and the affinity a pat gives. Not found in 40 s (or fallen asleep), it comes out by itself and giggles (「找不到我吧～」). Under Xvfb: hidden at window x −115 past the left edge, found by a click 10 px inside the edge. Not done: hiding behind other apps' windows (we cannot draw under them), a count of how fast you found it.

### Roger's twelfth pass (2026-11-06)

Roger's notes on the eleventh pass build: the stall looks gone but motion feels laggy (frames or the GPU?); scratching and fights lag too; the frisbee still disappears while a pet walks; making up straight after a squabble makes no sense, it needs an arc (起承轉合); a pet holding the frisbee or the ball should let go once scratched enough (he wants this added); every species should bring a thrown ball back (now only some do). Built the same day on `feat/play-and-scenes`:

1. **Lag** (`gpu.rs` `DEFAULT_BUDGET` 10% → 25%; `mesh_frames.rs` `URGENT_SECS`, `FrameStats`; `anim.rs` `fps`; `app.rs` `log_frames`): the new 10-second summaries in `chonkgene.log` showed the cause: a walking pet's frames go past the budget but are still paid for, so one walking pet emptied a 10% budget (one is about 12% of a UHD 770 at 30 fps) and every other pet fell to the 4 fps starvation floor. 25% keeps two pets moving and the rest at their idle rate. A pet held, rubbed or let go within a second draws past the budget (`urgent`) and at 60 fps (PLAN §5's rate for a poked pet; held still it was at 10–30), and the cursor rubbing about redraws it at once. Fights: the director's gestures were dropped whenever a pet had been walking that frame (the animator still counted it as walking), so swats and swishes came late or not at all; `Cmd::Act` and `Cmd::Cast` now stop it first. A scratch builds in 80 px of rubbing (was 140) and the lean follows faster.
2. **The frisbee while walking**: not reproduced (under Xvfb the Shiba carries it home in sight all the way, and renders of walking pets show it). Carried, it is now kept inside the pet's picture however far the head reaches; the log notes the mouth and the picture's size at every catch, for when Roger sees it again.
3. **A squabble's arc** (`skit.rs` `Phase::{Argue, Sulk, Sorry}`, lines `skit_sorry`, `skit_forgive`): glare; three rounds of huffing in turn, each a stamp of a hop and a fed-up face; the chase; both sulk back to back for 5 s; the one that chased comes over slowly and says sorry, is turned down once with a huff, offers a paw, and is forgiven: face to face, happy hops. Clicking still breaks it up at once (勸架). Giggles started by the app or the director now end after 3 s (`giggle_until`); before, one the mind had not started (the fox's after its spell, a pet that hid and was not found) never ended.
4. **Tickled into letting go** (`app/play.rs` `tickled_loose`, `LET_GO`): with the frisbee or its scene's bone in its mouth, scratched past 0.6 it laughs and drops it at its feet (a fetch ends there; the Shiba's bone scene ends too). A touch alone no longer ends a scene; being picked up does. Under Xvfb: `阿柴: tickled into dropping the Frisbee`.
5. **Every species brings the ball back** (`play.rs` `Chase::new(.., thing, ..)`, `Chase::nose`, `set_moving`; `app/play.rs` `PropWin::claimed`): dogs and the fox nose it, cats bat it home with a paw (`Act::Wave`), the chick and the duckling peck it along (half as far a push, more often), the owl watches it come to rest, flies over, picks it up in its beak and flies it back. The first pet to push it claims it and the others leave it to it. A push waits until the pet has stopped by the ball (the chick slid past it and back for ever). Test `every_species_brings_a_ball_back`; under Xvfb, one pet at a time: the Fold 2 bats, the chick 5 pecks, the owl in its beak, each `brought the Ball back`.

### Roger's thirteenth pass (2026-11-06)

Roger's notes on the twelfth pass build, each with what was found and done the same day on `feat/play-and-scenes`:

1. **The fox: its fire should not spin; casting, its forehead and eyes glow and the marks on its face grow deeper and longer, with more seal-like (咒印) lines; more spells on its right-click menu.** The spin was its fox-fire scene: orbs going round it while it spun on the spot (`Act::Spin`), and the cast's fire spiralling up. Now the orbs hang in their own places, bobbing and drifting a little (`props::foxfire`), the cast's flames rise where they are, swaying (`props::spell`), and in its scene the fox swings its tails and rings its bell now and then instead of spinning. Casting (and all through its fox-fire scene) its face glows (`PoseParams::glow`, `Pet::glow`, 0.5 s up, flickering, 0.6 s down; the uniform rides in `fwd.w`): the forehead flame blazes white-hot with a halo, the irises light up round dark pupils, the cheek stripes and the eyeliner's flicks draw out toward the ears, a ring with two beads appears round the flame and a dot under each eye (`paint.wgsl` `spell_marks`, `spell_glow`; deeper red under the light, a glow over it). GPU mesh path only; the sprite fallback has no glow. Its menu (awake) has 「施法…」 (`Page::Spells`, wand icon): 定身術, 狐火 (its scene), 祝福術 (hearts and a happy hop for everyone else), 招喚術 (everyone free runs to a place in a line either side of it, then is pleased) (`Director::cast`, `skit::Spell`, `Cmd::Bless`, `Phase::Called`). Asked while it is busy or another skit is on, nothing happens; alone, it casts all the same.
2. **The spell lifts the others higher**: 34 → 72 px at 100% (`FLOAT_RISE`).
3. **Pets seem to squabble with themselves.** Not seen here; two causes found in the director: a squabble's pair that could not reach each other in 7 s (an edge, a gap, other screens) still glared and huffed across the screens, and sulking "back to back" never turned them. Now a pair still more than twice the meeting distance apart when the approach runs out gives it up quietly (`TOO_FAR`), and sulking they turn away from each other. Ask Roger which pets and what he saw if it happens again.
4. **The chick and the duckling go out of shape when scratched and turning their heads.** A bird's head sits on its round body with no neck between, and turning to the finger, tipping with the touch and its own head tilts and foot-scratches all turned the head bone alone: up to 37° of roll, a crease under the head. The body now takes half of every touch turn and tip (`waddle.rs` `TOUCH_NECK`), and while touched its own head fidgets go at a fifth (the slow drift under them stays, so it never stops dead); at most about 20° now, no crease in renders.
5. **The pond problem is still not solved.** The likely cause, which no Xvfb run could show (no compositor): every thing on the floor is a window of its own made when it comes out, and on Windows a topmost window made later sits above the earlier ones, so the whole pond (far bank, dark inner bank and all) was drawn over the duckling. Now when a thing comes out or shows again its window has the pets' windows (and their bubbles) raised back over it (`Playground::restack`, `Pet::raise`); the butterfly stays in front. If the pond still looks wrong, a screenshot from Roger is the next step.
6. **After the spell the others should look a little scared.** Put down, each jumps back from the fox still facing it (12% of its width), starts (ears up, head back, looking at the fox), shock lines over its head and a drop of sweat (`Cmd::Scare`, `Animator::startle`, `Fx::Fright`), the one it picked on says so (`skit_scared`); the fox giggles. Stared back by the owl, the fox is the one put out and the owl pleased.
7. **Each species' special moments lag, such as the owl flying up from the floor to its branch.** Only a walking pet's frames were made to keep up with its moving window; flying, falling (a hop, a leap, thrown) or held up by the spell, its window moved every frame while its picture came at the idle rate, through the budget. Those now count as travelling (`PoseParams::travelling`, `Pet::travelling`): drawn at 60 fps, past the budget, each frame waited for as a walk's are.
8. **Catnip cannot be moved and seems to do nothing.** It was put down at the cursor and fell to the floor, but the pets decided whether it was in reach on the first frame, while it was still up at the cursor: more than 0.3 of their height above their feet, so out of reach, and nobody went again. Now nobody goes before it lands, any pet free to play within reach goes once it is down (`smell_nip`), a cat that likes it comes back 20 s after a visit (`NIP_AGAIN`), the rest leave that sprig be; and it can be picked up and moved with the mouse like the toys (`Thing::grabbed`), dropping where let go, the pets at it waiting and then coming over again. Under Xvfb: `catnip moved to (1294, 886)`, then the Fold `at the catnip`. Still by design about one cat in three does not care for it (`likes_catnip`, fixed by its id): such a cat sniffs, says so and leaves.
9. **A sleeping cat's tail looks cut off from some angles.** Renders of every sleeping pose both ways round found no picture edge reached by the tail; what looked cut off is the Munchkin curled up: its raised tail, twisted 68° with the hips, crumpled into a blob with a flat edge. Curled up, a cat's single tail now lies down and wraps round toward its nose (`CURL_TAIL_DOWN`, `CURL_TAIL_WRAP`).
10. **Asleep on its back, the belly should rise and fall as it breathes.** On its back the belly swells and sinks with each breath about three times as far (`BELLY_UP_BREATH`); plain in renders of the Munchkin and the Shiba.

Checked with tests (new: `the_fox_casts_what_its_menu_asks_for`, `pets_too_far_apart_give_a_squabble_up`, `the_fox_face_glows_while_it_casts_and_no_one_elses_does`; the spell test now expects a fright), clippy on Linux and the Windows target (stub C compiler), llvmpipe renders, and an Xvfb run driving the fox's menu: 定身術 (four pets held up, then `Scare` each), 祝福術 (`Bless` each), 招喚術, 狐火, and catnip dragged by the mouse. Owed on Windows: the glow and seal lines, the pets now above the pond, the owl's flight.

### Small things after the thirteenth pass (2026-11-06)

Roger said do the small leftovers too:

- **Asleep, held up by 定身術 in a dream bubble** (idea list of the eleventh pass): the director floats sleepers too (`Actor::asleep`: asleep, not held, not falling); its bubble is lilac with a crescent moon and stars drifting round (`Fx::Dream`), it sleeps on up there (breathing, its animation not stopped) and is put down without waking or a fright. Test `the_spell_floats_a_sleeper_and_lets_it_sleep_on`.
- **Cats bat at the fox-fire** (idea list): while the fox calls up its fire, a cat free to play within 700 px on its level runs to its side, faces it and paws at the orbs, a pounce now and then, until the fire is gone, then is pleased (`app/play.rs` `swat`, `Fun::swat`, `SWAT_REACH`). Under Xvfb: `芋圓: bats at the fox-fire`.
- **The Linux app-menu entry** (owed since the icon work, §1.9): `chonkgene --menu-entry` writes `chonkgene.desktop` to `$XDG_DATA_HOME/applications` and the 256 px icon to `icons/hicolor/256x256/apps` (never by itself: the program is portable); a moved folder is followed each start (`menu_entry::repair`, as the login entry is); `--no-menu-entry` and 「移除所有痕跡」 take it away. Windows has nothing to add (the exe carries its icon). Test `an_entry_is_added_repaired_and_removed`; tried here with a scratch `XDG_DATA_HOME`.

### A cuter hand (2026-11-06)

Roger asked for a cuter hand over the pets and picked, of three styles (a cat's paw, a chubby little hand, a mitten), the chubby hand. Drawn by the program (`chonkgene-render` `cursor.rs`, no pictures shipped): a plush peach hand with short round fingers, a dark brown outline that shows on light and dark alike, a blush on its back and a pink cuff; open over a pet or a toy, the index finger out while pressing or scratching, a fist while carrying a pet or a toy. 36 px at 100%, scaled by the screen; made once into the system's cursors (`make_hands`, winit `CustomCursor`); if the system refuses one, its own hand is used as before. Test `every_hand_is_drawn_round_its_hot_spot`; under Xvfb the cursor over a pet and while scratching is the 36 px drawn hand. Owed: the look on Windows.

### Roger's fourteenth pass (feedback only, 2026-11-06)

Roger's notes after the thirteenth pass, to do next time. First guesses at the causes, from the code:

1. **The pond flickers when a pet walks into it.** Likely the pet flipping in and out of the pond every few frames at its edge: `Playground::pond_under` needs the feet within 0.85 of the half-width and within 8 px of the pond's level, and only while `Idle` or `Walk` (a step's bob or a hop drops it), so the pond's front over the legs (`pond_front`) comes and goes, and each time back in it splashes again. Plan: in and out with some slack (in at 0.85, out at 0.95; a short hold before leaving), the splash only once per visit; also check the pond window itself redrawn every frame with its rings (`ALIVE_FPS`) and, on Windows, the pets' windows raised over it (`restack`) never fighting it.
2. **The fox's fire (written 黑火, read as 狐火) could be bigger when it casts; the pets about it react, some frightened, some excited.** Plan: a bigger cast (the ring wider, more and larger flames rising higher, the fox-fire scene's orbs larger and brighter); while the fox casts, each pet near it reacts by personality: the timid ones shrink back, ears flat, the bold or playful ones hop and bounce, cats crouch to pounce at the flames (as they bat at the fox-fire scene's orbs).
3. **Some pets' motion suddenly goes sluggish.** Roger suggests testing each action's frame rate. Plan: a test that runs every species through every action (walk, gestures, joys, scenes, touches, sleep, spells) on a lone renderer and records the pictures shown a second against the frames asked for (`FrameStats`), flagging any action under its rate (`Animator::fps`) or any gap over the starvation floor; and ask Roger for the `frames:` lines of `chonkgene.log` from a sluggish moment (which pet, doing what).
4. **The chick and the duckling, turning their heads after being scratched, look as if their heads might snap off.** The thirteenth pass halved the head's share of a touch's turn and tip (`TOUCH_NECK`) and damped its own head fidgets; still unnatural. Likely the turn to look at the finger (up to 40° yaw, `GAZE_TURN`) and its spring back when let go. Plan: for the round birds the body turns with the head almost entirely (a share of 0.2 or less for the head), a smaller gaze turn and tip, a slower, softer settle after the finger leaves, and a render check of the moment after a scratch.
5. **Feeding could be richer** (Roger, the same day). Now 「餵零食」 in the menu is all there is: up to 3 a day, the pet says its eating line, smiles, does the `Act::Eat` gesture (a few bites and chewing, no food drawn), gains affinity, and a grudge passes sooner; asleep it turns over, after three it is full. Proposed, for Roger to cut down:
   - **A snack you can see, its own for each species**: 肉乾 for the Shiba, 小魚乾 for the cats, 玉米粒 for the chick, 麵包屑 for the duckling, 油豆腐 for the fox (a kitsune's favourite), and the owl's still to pick (§10, now due). Drawn like the toys (`props.rs`), held in the mouth or the beak and eaten bite by bite, getting smaller, crumbs falling.
   - **How it gets there**: picked from the menu, the snack appears at the cursor and you drop it, or toss it, near the pet (a window of its own like the ball, `Thing::grabbed`); the pet runs over, sniffs, eats it on the spot. Dropped on its head or in front of its nose, it catches it.
   - **Eating that shows it is good**: species ways to eat (the Shiba wolfs it down, tail going; the cats nibble daintily, then wash their faces; the birds peck it in bits; the fox holds it in its paws), a happy wiggle or a hop after, hearts, a lick of the lips, a 「喀滋喀滋」 crunch.
   - **Favourites and moods**: each pet likes its own snack best (more hearts); a snack cheers a grumpy or sulking pet up faster, as now, and shows it (a grudging first bite, then forgiven).
   - **The others notice**: pets near by come over and sit and beg, eyes on the snack, a dog drooling; feed one and the rest look at you hopefully (each still has its own three a day). Two after the same dropped snack may squabble over it (irrational mode).
   - **Full**: after the third the belly is rounder for a while (`chonk` up a little, eased back over an hour), it lies down content, maybe dozes; offered more it turns its head away.
   - Never real-money or a store: snacks stay free, three a day (PLAN §1.12).

**Built 2026-11-10 on `feat/play-and-scenes`** (items 1–4; item 5 waits for Roger to cut the list down and pick the owl's snack):

1. **Pond flicker.** The guess held: a pet at the pond's edge flipped in and out every few frames, the pond's front over its legs coming and going and the rings on the water with it. Now in at 0.85 of the half-width and within 8 px of its level, out only past 0.95 and 14 px (`in_water`, `POND_IN`, `POND_OUT`), and only after 0.4 s out of the water (`Wade`, `POND_HOLD`; a hop or a flutter keeps it in, picked up it is out at once); one splash a visit, none within 2 s of leaving (`SPLASH_AGAIN`). Test `a_pet_at_the_ponds_edge_stays_in_and_splashes_once` (in and out every other frame for two seconds: in all along, one splash). The restacking of the thirteenth pass was already right.
2. **A bigger cast; the pets near it react.** The casting ring 70 → 100 px across, with a white flash going out at the start; ten flames instead of six, each a real flame (glow, white heart, flickering tip, `props::flame`) 1.4× the fox-fire scene's, rising 150 px instead of 110; 1.8 s instead of 1.4; the ring raised off the feet so the window's edge no longer cuts its near side. The fox-fire scene's orbs are 1.3× larger and brighter. As the fox casts, every pet free within 700 px of it sees the fire go up (`Cmd::Awe`, `skit::Awe::of`, by its own energy, now in `Actor::energy`): a cat with any go in it (energy ≥ 0.35) wiggles its bottom and pounces at the flames 0.9 s later (`Fun::stalk`); anyone lively (≥ 0.6) hops twice, happy, sparkles; the rest start where they stand (no hop: in the air a pet is not free, and the director calls the spell off if the one it picked on is not), a drop of sweat, and keep low 2.5 s (`Fun::cower`, its own timer, not "busy": as `hold_until` it left the frightened one out of the spell, and an Xvfb run showed the whole 定身術 called off when that one was the pet picked on). The owl the spell is aimed at still stares it back instead. Test `pets_near_the_fox_take_its_spell_each_in_their_own_way`. 黑火 still read as 狐火 (Roger to confirm).
3. **Sluggish motion.** The test asked for: every species through every action (rest, walk, 21 gestures, cheer, pounce, lively, five faces), asking for frames as the app does, the farthest any part moves between two frames (`MeshRenderer::pose_step`; the mesh never lands, so it runs on the CPU in seconds). It found two real faults: (a) a gesture's strength went as `sqrt(sin)`, steepest at both ends: its last frame dropped about a fifth of the pose at once (a yawn's ear jumped 18 px at 60 fps, in every species); now eased in and out over 0.35 s at each end, held between (`envelope`, `GESTURE_EASE`); (b) the rate asked for followed what the pet was doing, not how fast it moved: a gesture over, it fell to 15 fps while the tails were still swinging back (the fox's 22 px a frame), a Spin or a happy wiggle stayed at 30 with tail tips 30–49 px apart. Now the renderer asks for enough frames that nothing moves more than 4 px between two (`SMOOTH_PX`, up to 60), falling back slowly after a burst (`SMOOTH_FALL`, 60 → 15 in about half a second) so a flapping wing turning back does not drop the rate. Test `every_action_asks_for_frames_enough_to_move_smoothly` (without the change: over 40 jerks; with it: none). Still wanted from Roger: the `frames:` lines of `chonkgene.log` from a sluggish moment on Windows (the GPU budget's share is not in this test).
4. **The chick's and duckling's heads after a scratch.** Measured: the frame after the finger left, the head turned 3° on its own and the whole look 5° in one frame (the snap). The round birds' body now takes four fifths of every touch turn and tip (`TOUCH_NECK` 0.5 → 0.2), they look round at a finger 0.6 as far as a mammal (`TOUCH_GAZE`), turn to face you more slowly (`BIRD_ATTEND_HZ`), and once the finger leaves they settle back gently and never past: the look's springs damped and slower (`Moods::soft`), and for 2.5 s back to how they stood at 0.7 Hz (`SETTLE_SECS`, `SETTLE_HZ`). Now at most 1° on its own and 0.8° in all a frame. Test `a_scratched_bird_turns_with_its_body_and_settles_softly`.

### Roger's fifteenth pass (feedback only, 2026-11-10)

Roger's notes on the fourteenth pass build (f9e72b4, pushed and built by CI the same day; presumably the Windows test build), to do next time. Three of the four fixes did not show on his machine, so each starts with finding out why on Windows, not with more of the same:

1. **The pond still flickers when a pet walks in, and the pet's legs show where they should not** (漏出來). The edge hysteresis (`Wade`) did not cure it, so the flip in and out was not (or not the only) cause. Suspects: on Windows the pond's window and the pet's window being restacked against each other while it walks (`Playground::restack`, `Pet::raise`); the pond's front over the legs (`pond_front`) drawn at the pet's frame rate and position while the pond's own window redraws at 15 fps (`ALIVE_FPS`) with its own clock, so the two pictures disagree for a frame; the legs below the pond's near edge only wiped within the pond's width (`pond_front`'s strip). Ask Roger for a short clip (gdrive inbox, reviewed with the `gemini` skill) to see which.
2. **As the fox casts, the pets do not react (frightened or excited).** `Cmd::Awe` was seen sent under Xvfb, so either it does not reach the pets on his desktop (none free within `AWE_REACH` 700 px, or not `available` then: busy, sulking, asleep, falling) or it is sent and too slight to see (a start, a double hop, a wiggle). Check `chonkgene.log` for `Awe` lines from his run; make the reactions bigger and longer either way.
3. **Motion still suddenly goes sluggish.** The frame-rate fixes (gesture envelope, rate by pose speed) were about frames asked for; on his machine the limit is likely the GPU budget or the GPU itself. Needed: the `frames:` lines of `chonkgene.log` from such a moment (which pet, doing what). Consider showing the frame stats in the log more often, or a 流暢度 setting.
4. **The fox's fire (written 狐人, read as 狐火) could be bigger still; and after 招喚術 nothing happens.** Bigger cast again. 招喚術 (`Spell::Call`, `Phase::Called`, `call`): only pets `available` at the end of the cast are called, so check whether the new `Awe` reactions (or anything else) leave them not free then, and whether the call's walks are cut short; test it with three or four pets under Xvfb and on the log.
5. **Most of the animal sounds still sound odd; has Gemini been used on them?** Not yet: the sounds are still the synthesised placeholders (`chonkgene-core` `sound.rs`, §10). Gemini cannot make animal recordings, but it can listen to each sound and say what is off, compare takes, and help pick; per the working rules it gives opinions and material only, and any generated audio ships only after Roger approves it and Gemini's terms are checked. Next time: render every sound to WAV, have Gemini review them per species, then rework the synthesis (or look for properly licensed recordings).

6. **The belly rising and falling, especially asleep** (Roger, later the same day). Only asleep on its back does the belly swell visibly (`BELLY_UP_BREATH`, thirteenth pass); in every other pose it moves 0.05 along the belly bone, too little to see, and asleep frames come at the lowest rate (5 fps asked, and starved, above). Plan: a visible breath in every sleeping pose (the flank or the back rising on the side, the round birds' whole body swelling a little), slower and deeper asleep than awake, eased so it reads at a low frame rate; check in renders of each pose.
7. **定身術's bubble should be bigger: some pets' heads stick out of it** (the same note). The bubble is the same size for everyone (`props::bubble`, 74 × 70 px at 100%, its middle 0.95 world units up), so tall heads, ears and tufts (the fox's ears, the owl's tufts, a pet at a larger size, or held up mid-pose) poke out. Plan: size and place it from the pet's own picture (the opaque bounds or its head anchor and size, with a margin), and make sure its window has room for it.

8. **The cats' sunbeam looks weak and fake (虛)** (the same day). Rendered on dark, blue and light backgrounds: a flat trapezoid with hard straight sides and only a vertical gradient (alpha up to 110), which over a dark desktop (Roger's) reads as a muddy olive plate, since a layered window can only lay colour over the desktop, never brighten it; over a light one it nearly vanishes; and the cat in it is not lit at all. Proposed, for Roger to pick from mock-ups first: (1) the cat lit by it: warmer and brighter on the side toward the light, a warm rim (a uniform like the fox's glow); (2) a bright warm pool on the floor where it lies, soft-edged, bright enough over dark desktops; (3) soft feathered sides and two or three faint drifting streaks inside; (4) whiter at the top, golden low, brighter and more saturated overall; (5) motes larger, glowing, twinkling. An extra idea: the pool shaped like a window's panes, the cat curled in one of them. Order suggested: (2)(3)(4) first (the beam alone), then (1).

9. **A spotlight and a 肉肉舞** (Roger's idea, the same day; he chose to keep the sunbeam too). A new scene for every species, beside the cats' sunbeam (which is still improved as in item 8): a stage spotlight snaps on from above (a cone, a bright crisp pool on the floor, sparkling dust; a colour-cycling disco variant maybe), and the pet in it dances for about 6–8 s, then the light goes out. The dance, shared: bouncing to a beat with the belly jiggling (the point of 肉肉舞), the bottom wiggling side to side, front paws or wings waving in turn, a spin, a 「登登！」 pose at the end with hearts and sparkles, music notes ♪ floating up, a little synthesised tune with sound on. Each species' own touch: the Shiba's tail going round like a helicopter, the cats kneading the air, the chick and the duckling swaying foot to foot, the owl bobbing and tilting its head to the beat, the fox fanning its nine tails like a peacock, its bell ringing on the beat. Irrational mode: pets near by run into the light and join in. Menu (not decided by Roger; the default proposed, to confirm before building): both 「叫牠…」 › 「跳舞」 (the light comes down on that pet and it dances) and a placeable 「放聚光燈」 like the catnip (whoever walks into the pool dances, so several can dance together).

**Gemini's part in this pass** (through the `gemini` skill; it gives material and opinions, every line of code stays Claude's, nothing generated ships without Roger's yes and a check of Gemini's terms):

- **The sounds (item 5), first.** (a) Roger's clip `inbox/2026-11-10 15-03-23.mp4` (kept on the box, git-ignored) has the pets' voices in it: bursts at about 1.9, 6.0, 9.9, 12.3 (the duckling's 「游泳……呱～」), 14.3, 17.4, 22.3–27.9, 29–32, 35.3, 37.8 s. Have Gemini listen to each and say what sounds wrong; Claude tells from the frames which pet is speaking. (b) Render every sound of every species to WAV (all takes, `TAKES`) and have Gemini review them per species against what the animal really sounds like and against "cute"; it can compare takes and rank them. (c) Claude reworks the synthesis from that; if recordings are wanted instead, find properly licensed ones (Gemini cannot make animal recordings).
- **Clips of each fix, after.** For every Windows clip Roger sends (pond, sluggishness, cast reactions, 招喚術, the dance), a first pass by Gemini at `--fps 10–15`, then Claude checks the frames it points at itself (ffmpeg contact sheets): it mislabelled the owl as a cat in this clip.
- **Mock-ups (items 8, 9): Claude draws them** with the props code (no billing needed). Gemini's image models need billing (free tier 0 on 2026-11-09); only if Roger turns billing on, reference images for the dance poses and the spotlight. The dance's little tune is synthesised by Claude (Lyria needs billing too).
- **Optional second opinion** on the mock-ups and on the reworked sounds before showing Roger.

**Found from Roger's clip and log (2026-11-10, Windows, Intel UHD 770, seven pets; 46 s at 60 fps looked at frame by frame, with the `gemini` skill for a first pass):**

- **Pond flicker and legs showing: the plain picture is shown on a redraw.** In the clip a duckling swimming in the pond turns, every few frames, into its picture without the water: sunk lower, its feet showing below the pond's near rim (31.0–31.8 s: at 0.067, 0.233–0.267, 0.450, 0.517, 0.55–0.60, 0.70–0.72 s in). `WindowEvent::RedrawRequested` (`app.rs`) presents `renderer.pixmap()`, the picture before its overlays (the pond's water, a toy in its mouth, effects, the fox's fire), while `Pet::draw` presents the composed `fun.canvas`. Windows sends redraws whenever the windows are restacked (things coming out, `restack`, `raise`, bubbles), X11 hardly ever, which is why Xvfb never showed it. Fix: a redraw presents what was last presented (the canvas while `had_overlay`). Probably also the frisbee that "vanished while walking" (twelfth pass, never reproduced): a carried toy is an overlay too. The `Wade` hysteresis was not the cause, though harmless.
- **A new pond covers the pet in it for about 0.1 s** (12.25–12.35 s: drawn over the sleeping owl, then the owl raised above it): the restack comes a tick after the pond's window. Raise the pets in the same tick, or show the pond's window only after.
- **An owl asleep where the duckling's pond scene put the pond sits on top of it**: its round bottom over the pond's near rim. The pond came out under a pet that was not meant to be in it (the scene's own pet, the duckling, was off to the side, saying 「游泳……呱～」 before reaching it). Put the pond where nobody stands, and keep the wading line/near-rim wipe right for a sitting or sleeping pet.
- **Sluggish: idle pets starve while others walk.** `frames:` lines: walking pets' frames are forced past the budget and alone take 35–45% of the GPU (2–3.7 ms a frame on the UHD 770, seven pets, 40–58 frames a second asked each), against a 25% budget; a pet that stops walking falls to the starvation floor, e.g. `咕咕 3.5/s shown of 46.2/s asked; 0 walking`, `阿柴 3.8/s shown of 34.7/s asked`. That is "suddenly sluggish": the moment a pet stops. The fourteenth pass's rate-by-speed asked for even more frames. Fix the sharing, not the asking: forced frames count against the forcer's share, every pet gets a fair minimum (e.g. 15–20 fps) before anyone gets more, the total asked scaled to what the GPU gives; consider cheaper frames for idle pets.
- **Pictures stuck for 4.5 s with a frame "waiting gpu"** (`picture unchanged for 4562 ms ... frame waiting gpu` for two or three pets at once, 695 s and 737 s, around casts): a frame sent and not collected for seconds, not the budget. Find why the readback is not collected (polling of the device on Windows/Vulkan, the fence) — this hides the cast reactions too.
- **The cast reactions are sent** (`Awe` lines: 芋圓 Shrink, 奶茶 Crouch, 蛋黃/阿柴 Bounce, 湯圓 Shrink) and 定身術 takes everyone; they did not show because those pets' pictures were starved or stuck at the time (above), and they are small. Make them bigger once frames flow.
- **招喚術** cannot be judged from the log (the call's walks and the end are not logged): log `Phase::Called`'s walks and its end, then try again.
- **The log file starts with 31 KB of NUL bytes** after its first line: the file is truncated (rotated?) while still open at an offset. Look at `diag.rs`.

**Gemini's review of the sounds (item 5 (a) and (b), 2026-11-10).** Every sound rendered with `examples/voices.rs` and strung together per species (Shiba, fox, Fold, owl, chick, duckling: the three takes of each call, one of the rest, 0.8 s apart, with a timetable); the clip's sound track sent with who speaks when (told from the frames and from each burst's main frequency). Answered by Gemini 3.6/3.5 Flash only: 3.8/3.7 were busy or out of free quota, and `gemini-4-argon` and the Pro models need billing on the key's project (not Roger's AI Pro app subscription, which these calls do not touch). Flash contradicts itself at times (the Shiba's pant "a maraca" in the clip, "realistic, 5/5" alone), so only what it says again and again, and what the code bears out, is kept:

- In the clip: the owl's sleep-talk (`Snore`, 1.9, 31.1, 42.7 s) "a two-stroke lawnmower"; the Shiba's pant (6.0 s) "a spray can shaken"; its laugh while tickled (24–31 s) "a sped-up gremlin"; the duckling's 呱 (12.3 s) "a party horn"; a bird ruffling (`Flutter`, about every 7–8 s) "a static tap". The burst at 35.3 s is the fox's bell (2350 Hz), not a snore.
- **`Grumpy` is a human "uh"** in every species (the same formants 650/1300/2600 Hz for all, on a 160–300 Hz growl; the chick's 1800 Hz pitch through those formants comes out a buzzer, "a fart"): rated 1–2 everywhere. The worst sound.
- **`Snore` is the same 75 Hz growl for every species**, birds too: an engine, not a sleepy breath; a chick or a duckling cannot sound that low.
- **`Laugh` is five equal bursts at an even rate**: "a machine gun", "a modem"; no breaths between, no change of rhythm.
- **Calls are a clean buzz with smooth, slow glides**: the bark "a squeak" (a real one is 0.1–0.25 s, a noisy onset, the pitch spiking and dropping fast; ours 0.2–0.6 s, a slow rise); the chick's 啾 should be nearly a pure tone, 2.5–4.5 kHz, 40–150 ms, a quick up-down sweep; the duckling's 呱 too buzzy (Q 9, buzz 1.0). Wanted throughout: a breath or noise onset, pitch jitter, quicker decays.
- **Takes hardly differ** (only the pitch ×1.0/1.05/0.95 and the number of repeats): "virtually identical" for the cats.
- **`Poke` reads as a UI boop**; `Land`, `Eat`, `Sigh`, `Flutter`, `Bell` pass (3–5).
- Not a problem: `Purr` for the dog and the birds is never played (only the cats purr, `line_sound`, `fidget_sound`).
- Unconfirmed: clicks at the start and end of the noise bursts (pant, flutter) in the clip; the synthesised envelopes are eased, so look at playback (`audio.rs`) if it is heard again.

**Built 2026-11-10: recorded calls (item 5 (c)).** Roger chose a mix: real recordings for the calls, synthesis kept for the knocks. Candidates: Freesound searched with its CC0 filter (its 128 kbps previews, the same licence; no login needed) and Wikimedia Commons; about 115 clips, screened by Gemini 3.6 Flash and then measured here (noise floor, signal to noise, clipping, where each call is), because Flash's "cleanest" picks measured 12–19 dB above their noise. The best were cut to one call, pitched and levelled to the app's loudness, and put beside the old synthesis on a listening page (https://claude.ai/artifact/1oH4dcp3bM13Kk4jiNmThG); Roger picked one per row. Every pick was then checked CC0 on its own page.

- **Picked**: the Shiba a small dog's bark, a happy whimper for its sigh and its coaxing greeting, a Shih Tzu's growl when fed up; the fox a puppy's bark ×1.4 (no usable CC0 fox exists, and real foxes do not sound cute); the Fold and the Munchkin each their own kitten's meow; both cats a purr, a kitten's trill to greet and laugh with, a grumble; the owl a tawny owl's hoot; the chick a chick's peep (Commons); the duckling **ducklings' peeps, not a quack** (Roger's pick over 呱; the line text stays); one dog's snore for everyone (each pet's own pitch on playback).
- **Made from them** (`sound::recorded`): greetings, laughs and grumbles are chains of the picked calls, with uneven gaps and each a little lower and softer (the synthesis's even rate read as "a machine gun"); the birds grumble with their own call lower, short and sharp. Takes vary the pitch ×1.0/1.05/0.95 as before. Everything still goes through the class loudness and the limiter, so the mix is unchanged; the longest sound is now 2.6 s (`LONGEST`, the snore's whole breath).
- **Still synthesised**: poke, landing, eating, panting, the bell, wings, and the cats' and birds' sighs (Gemini found these passable, 3–5).
- **Changing one later** (Roger: keep these a while and see how they go): edit `tools/soundbake/sounds.json` (source, start, end, pitch, denoise), run `FFMPEG=… python3 tools/soundbake/bake.py`, which also rewrites `licenses/sounds.md`; how the clips are used is `recorded` in `sound.rs`.

**Built 2026-11-11: items 1–4, 6, 7 and the causes found from the clip and log** (ea31d25). Every one still needs Roger's eye on Windows: the Xvfb box cannot show the redraws or the GPU's real costs (llvmpipe takes 18–33 ms a frame).

- **The pond's flicker and the legs (item 1)**: `RedrawRequested` now presents the picture last shown (the composed canvas while it has overlays). A new thing's window raises the pets in the same go round as its first picture (the restack ran a tick late). Ponds and sunbeams come out where no other pet stands on that floor (`free_spot`: beside it, further out, the other side; none free, the scene waits and the log says `no room for its Pond`). Not done: the wading line and near-rim wipe for a pet sitting or asleep in the water (with ponds out of the way it should not come up; look again if it does).
- **Sluggish (item 3)**: the cause was the budget, not the asking. Walking frames went past the budget but their measured cost was still settled against the shared pool, with no floor to the debt, so the pool stayed empty and every idle pet waited for the 250 ms starvation rule (4 fps). Now (`gpu.rs` `Budget`, `Ask`): every pet drawing may use the budget over the number drawing before anyone gets more; a walker past that still gets a frame every 33 ms, or every `cost / WALKER_MOST` (10% of the GPU each) on a slow GPU; the debt is at most a burst deep; touched or starved frames go anyway as before. Simulated with Roger's numbers (seven pets, two walking at 60, five idle at 30, 2.2 ms a frame): walkers 30 fps, idle pets 16.5 fps (was 3.5), the GPU 31.5% (his log: 35–52%). Walkers get 30 rather than 45–58 when the GPU is that busy; on a GPU that can afford it everyone gets all they ask.
- **The 4.5 s "frame waiting gpu"**: found the next day (below, 招喚術). The log also says how long a frame has been on its way (`gpu, sent N ms ago`), whether the pet is held up by the spell, and the longest wait between two of its draws.
- **The log's NULs**: a second launch (the program started again while running) opened the log afresh before finding the first copy running; the log now opens after the single-instance claim.
- **Cast reactions (item 2)**: bigger and longer. Frightened: twice the start, a tremble (a quick lean to and fro) while it keeps low for 3.5 s, a yelp (its own call higher) or, half the time, its 「嚇死我了」 line. Thrilled: three high hops, hearts and sparkles, a happy call. Cats: a 1.4 s wiggle before the pounce.
- **The cast's fire (item 4)**: the ring 120 px wide (was 100), 14 flames (was 10) about 1.6 times the size, rising 175 px, a faint column of light; as big as the fox's window allows.
- **招喚術 (item 4)**: the called pets were not "in the skit", so their own wandering fought the director's walk. They are now (`Director::called`), and a pet still busy when the cast ends (cowering from it) is called once free. Every step of a skit goes to the log with where every pet is (`skit: Spell Call of 1 and 2, Called, called [7, 6, 2] (at 1 1020, 2 1246, …)`). Roger, the same day: still no visible effect. The real cause, found under Xvfb (`CHONKGENE_SKIT=call`, new; also `bless`): the director sends the walks again every go round, and `direct` set each pet's `next` to `Instant::now()`, always a hair after the loop's own `now`, so the called pets were never stepped for the whole 5 s (and the loop spun). Now a walk or a turn leaves `next` alone and other commands only bring it forward (`woken`). Under Xvfb the called pets now walk 140–530 px and line up beside the fox. This was also both 4.5 s "picture unchanged … frame waiting gpu" stalls in Roger's log (each began 1.3 s after a cast, at the start of 招喚術, and lasted its 5 s), and it held up a play-fight's running at each other (`Gather`) the same way.
- **The spell's bubble (item 7)**: sized from the opaque part of the pet's picture as it floats up (`props::bubble_fit`): a fifth more than its half sides plus a margin, never smaller than before, moved inside the window. Checked on renders of the fox, owl, Shiba and chick: all of them inside; tall ears and tufts reach the rim, since the window is no taller.
- **Breathing asleep (item 6)**: measured first: asleep, the whole body's breath already moves the top of the pet 9–15 px between breaths, so what Roger saw was mostly the starved frames (fixed above). The belly now also swells more lying down in every pose (`LYING_BREATH`), the birds' belly more asleep.

**Built 2026-11-11: the sunbeam (item 8).** Mock-ups of four (today's; a new shaft and a pool; the pool as window panes; the new one with the cat lit) over a dark, a blue and a light desktop; Roger picked the last. `props::sunbeam`: the shaft worked out pixel by pixel (layers left steps and bands): feathered sides a fifth of its width each, in from nothing at the top, white high up and golden by the floor, three faint streaks drifting across it; a bright warm pool where it lands (a radial gradient, cream in the middle, 210 alpha: the part that reads as light over a dark desktop); eleven motes, larger, glowing and twinkling. The thing is 260 wide now (was 200) for the pool. `props::sunlit`, from `compose` while a pet lies in its sunbeam scene (easing in and out over 1.5 s): a warm gradient over its own pixels from the upper right of its picture, and a warm rim on its edges facing the light. Over a light desktop the beam is still faint (a layered window cannot brighten what is under it). Test `the_sun_lights_only_the_pet_warmer_toward_it`.

**Built 2026-11-11: the spotlight and the 肉肉舞 (item 9).** Roger chose both menus.

- **The dance** (`Act::Dance`, 7 s; `anim::dance_at`, 120 a minute): four beats bouncing (a hop and a jolt of the belly on each beat, from the animator, so the belly jiggles), three wiggling the bottom out to a side on each beat, three waving the front paws in turn (out to the side as well as up: facing you, a paw lifted only forward hardly showed), a two-beat spin, two beats of 「登登！」 (a paw high, chin up, leaning back); the head bobs on the beat all through. Each species' own touch: the Shiba's tail goes round like a helicopter, the cats knead the air with both paws, the fox fans its tails out like a peacock and its bell swings on the beat, the chick and the duckling sway from foot to foot, the owl bobs and tilts its head (`skin.rs` and `waddle.rs` `Act::Dance`). Its own hops do not end it (`free()` would have), and no other gesture cuts in (a hello wave did, half way). Renders: `--example mesh -- dance <species> <dir>`.
- **The spotlight** (`Thing::Spotlight`, 240 × 380; `props::spotlight`): a cone of white light straight down, crisp sides, brighter in the middle; a bright pool with a sharp rim; the lamp's glare at the top; dust twinkling in it. Snaps on in a quarter second, goes out over its last one.
- **「叫牠…」 › 「跳舞」**: the light comes down on the pet where it stands, it dances, the light goes; **「放聚光燈」** (main menu, 「把聚光燈收起來」 takes it away): put down beside the pet for 45 s, out of other pets' way (`free_spot`); that pet walks in and dances; any pet free walking into the pool joins in, and with 非理性模式 on those within 700 px run over now and then (`Playground::stage_step`); side by side, the middle first; a pet dances again only 20 s after.
- **With it**: music notes floating up (`Fx::Notes`) every few beats, hearts and sparkles and the bell at 「登登！」; lines `dance` and `dance_end` for every species; a synthesised tune (`Sound::Tune`): three two-second phrases, each a take of its own, played by one dancer for everyone in the light on beats 0, 4 and 8, the ending's two chords on beats 11 and 12 (the second as it strikes its pose). `inbox/dance-tune.wav` for Roger to hear.
- **Checked** under Xvfb (`CHONKGENE_STAGE=1` puts a spotlight down by the first pet 3 s in, `CHONKGENE_STAGE=dance` asks it to dance): every pet joined within seconds and danced its whole 7 s, standing (a hold made it lie down: `max_hold`). The screenshots cannot show the light over the pets (no compositor). Not done: the disco variant (colours cycling), the fox's bell ringing aloud on every beat (once, at the pose, to keep the tune clear).

**The tune as kagura (Roger, the same day: 「像巫女神樂舞配樂，可以略多點古老的韻味」).** Rewritten (`sound.rs` `tune`): a bamboo flute (神樂笛: breathy, rising into each note from two semitones below, falling away at its end, a slow vibrato) in the old 呂 scale (D E G A B), the small drum (締太鼓) in a syncopated pattern, the big drum (大太鼓) on each phrase's first beat, the miko's bells (神樂鈴) shaken in a roll on the second and fourth beats, a quiet 笙 chord (two reeds a hair apart) under it all, in a small wooden room (a damped Schroeder reverb), every stroke a little early or late, harder or softer. 「登登！」 is now two strokes of the big drum with the bells and the flute's cry. Each phrase fades into the next (the reverb's cut clicked). Gemini (3.8, then 3.5/3.6 Flash) listened to each version: version 1 2/5 ("a 16-bit arcade level"), version 2 3.5/5; Flash's later scores contradicted themselves, so only what it said again and again was acted on (the bells too bright, like sleigh bells: lowered to 1.5–2.1 kHz and rolled); a crude treble index fell 0.125 → 0.047. `inbox/dance-tune.wav` (new) and `inbox/dance-tune-old.wav` for Roger to compare. Roger, the same day: keep this version for now.

**Built 2026-11-11: the poke sound (item 5), and Roger's pond note.** The poke is now a finger sinking into something soft (a muffled thump, a puff of air, a little rubbery give) instead of a clean falling tone; Gemini (3.6 Flash) heard it as "a soft, organic squish, not a UI boop". The other knocks had passed (3–5) and stay. `inbox/poke-and-splash.wav` for Roger.

Roger, the same day: **the pond borderless, its edge fading out, deeper, bigger, room for more pets, and a water fight.**

- **Look** (`props::water`, worked out pixel by pixel): no bank and no grass; light turquoise at the rim going deep blue toward the middle and the back, fading into the desktop over its outer quarter; a soft band of sky near the back; rings; two lily pads bobbing. 460 × 96 at 100% (was 250 × 70). Over a pet in it (`pond_front`) the same water from the waterline down, over the pet's own pixels only (a second layer of water over its whole window boxed it in), the body showing through a fifth, fading out with the pond's edge (it poked out past it). Wading is deeper (`WADE` 0.07 → 0.18 of the picture: up to the belly), the duckling swims lower.
- **Room for more**: with 非理性模式 on and a pond out, pets within 700 px get in now and then (`Playground::pond_step`), up to five, each to its own place along the water, for 18 s (again only after 40 s), paddling about it. The duckling's pond stays 45 s (was 30). Only a pet settled there (asleep, busy) keeps a pond from coming out; one awake is welcome in the water (the bigger pond found no room at all on a crowded desktop).
- **The water fight** (`water_fight`): with two or more in a pond, every 1.3–2.7 s one throws water at the nearest other: paws scooping (`Dig`), wings (`Flap`), the fox's tails (`Swish`); a stream of water with drops flying off it arcs over (`props::splash_arc`, drawn in the pond's window, which got 130 px of room above it, at 30 fps while it flies); `Sound::Splash`. Where it lands the other starts and is splashed, then laughs it off, shakes it off (sometimes 「哇！好冰喔！」), or throws some straight back. Lines `splash` and `splashed` for every species. Under Xvfb (`CHONKGENE_STAGE=pond`): all seven got in within half a minute, 29 throws in 45 s.

Still to do: Roger's look at all of it on Windows.

### Roger's sixteenth pass (feedback only, 2026-11-11)

Roger's notes after the fifteenth pass, to do next time:

1. **Seven pets on the desktop at once by default** (「一般同時在桌面上的寵物，預設 7 隻」). Now `Settings::on_desktop` defaults to 6 (`store.rs`); 7 is every species once. Check the settings page's limit control, the first-run adoption (two pets) and the §5 budget with seven (the fair GPU share above is what makes seven bearable on a UHD 770).
2. **In 「我的寵物」 (settings) the pet's motion lags.** Found while writing this down: the preview is ticked by a Slint timer at `PREVIEW_FPS` = 10 (`settings.rs`), the animator stepped a fixed 0.1 s a tick rather than the real time passed, and a mesh frame is collected a call late (`MeshRenderer`: sent, then picked up on the next render), so what shows is 10 fps at best and a tenth of a second behind. Plan: 30 fps or more (the renderer's own `min_fps`), step by the real time, collect the frame in the same tick (or wait for it: the settings window has nothing else to keep smooth); check the settings process's GPU budget (its own `Gpu`, one user).
3. **Coming out of the pond, a square afterimage shows under the pet for a moment** (「從水池出來的一瞬間，下面出現方塊殘影」). Suspects: the frame where `fun.pond.now` turns `None` — `compose` stops, `draw` presents `renderer.pixmap()`, while the last composed canvas (with the pond's water and the strip cleared below its near edge) may still be what a Windows redraw shows (`had_overlay` already false), or the plain picture's legs below the pond's rim (cleared until then) show for a frame against the pond window behind; or the pond's spray/stir drawing. Ask for a clip (60 fps) to see which; check the transition frame by frame under Xvfb too (`CHONKGENE_STAGE=pond`).
4. **Touching different parts gets different reactions, by species** (「摸動物不同部位時，會有其他反應，如貓摸下巴和肚子會很興奮，以此類推」). Now every zone (`Zone`: Head, Chin, Cheek, Belly, Paw, Tail, Bottom) gets much the same poke / pat / tickle routes (`Mind::poke`, `Mind::pat`, the tickle stages), the head following the finger and the chin lifting. Plan: a table per species of how much it likes each zone (loves / likes / does not mind / dislikes), and reactions to match: loved spots (Roger: a cat's chin and belly) excited — purring loud, eyes shut in bliss, kneading, rolling over for more, the tail curling, hearts; liked ones a lean into the hand; disliked ones (a cat's tail, a dog's paws, a bird's back) a flinch, a glance, ears back, stepping away, and with grudges as now. To propose the table to Roger before building (e.g. Shiba: loves chest/chin and belly, bottom-scratch at the tail base; cats: chin, cheeks, head, belly for some (a trap for others); owl: head and cheeks only; chick/duckling: head and back; fox: ears/head and tails).

#### Built 2026-11-12: items 1–3; item 4 proposed

1. **Seven by default.** `Settings::on_desktop` defaults to 7, the settings hint about resources now from 8 up. Settings files got `format: 2`: `Settings::upgrade` turns a format-1 file still on the old default of 6 into 7 (any other number was chosen and stays), saves it, and the limit's raise brings a pet that rested only for the old limit back out (`fit_to`, as changing it in settings would). The first run still adopts two.
2. **The settings preview** draws at 30 fps, faster when the pet's renderer asks for it (`min_fps`, up to 60: a timer at 60 Hz, a frame when one is due), and the animator steps by the real time since the last frame. Its renderer is **eager** (`Renderer::set_eager`: mesh and ray-marched GPU pets): each frame is sent past the budget and waited for in the same call, so what shows is never a call behind (the settings window shows one pet and has nothing else to keep smooth). Test: `an_eager_mesh_renderer_shows_each_frame_in_the_call_that_asks` (fails without it at the first frame).
3. **The square afterimage: found and fixed.** Seen frame by frame under Xvfb with a new development switch, `CHONKGENE_DUMP=<folder>` (`diag::dump`: every frame of a pond and of each pet in or just out of one, as `<ms>-<what>-<x>_<y>.png`, put together offline the way a compositor would). Two causes:
   - **The pond vanished in one frame** at the end of its time, while the pets in it still counted as in it for `POND_HOLD` (0.4 s): each kept the pond's water over its lower body with no pond behind it, a box of water under every pet. Now a pond fades out over its last second (`POND_FADE`, its `used`), the water on the pets in it sinks with it (`Fun::pond_left`), and a pet whose pond is gone is out at once (`Wade::leave`).
   - **Stepping out past the pond's end**, the strip between the pond's ends below the waterline was wiped wherever the water was not: a square bitten out of the body for the frames of the hold. Now only what hangs under the near rim is wiped (`props::clear_under_rim`), less toward the ends and not at all at them, with no hard edge (test `only_what_hangs_under_the_near_rim_is_wiped_and_softly`); and the waterline eases up over 0.3 s going in and back down once the feet are out (`WADE_EASE`), instead of jumping in one frame.
   Under Xvfb after the fix: eight exits and the pond drying up, no box and no bite in any frame. Still owed: Roger's look on Windows.
4. **Touch by zone: proposed to Roger 2026-11-12, not built.** What each species makes of a hand on each zone (❤️❤️ loves, ❤️ likes, · does not mind, ✗ dislikes):

   | | 頭 head | 下巴 chin | 臉頰 cheek | 肚子 belly | 腳 paw / wing | 背、屁股 back, bottom | 尾巴 tail |
   |---|---|---|---|---|---|---|---|
   | 柴犬 | ❤️ | ❤️❤️ | ❤️ | ❤️❤️ | ✗ | ❤️❤️ (scratch at the tail base: a hind leg kicks) | ✗ |
   | 曼赤肯貓 | ❤️❤️ | ❤️❤️ | ❤️❤️ | ❤️❤️, a trap after a while (grabs the hand, bunny kicks) | ✗ | ❤️ (bottom goes up) | ✗ |
   | 蘇格蘭折耳貓 | ❤️❤️ | ❤️❤️ | ❤️ | ❤️❤️ | ✗ | ❤️ | ✗ |
   | 貓頭鷹 | ❤️❤️ | ❤️ | ❤️❤️ (the face disc) | · | ✗ | · | ✗ |
   | 小雞 | ❤️❤️ | ❤️ | ❤️ | · (ticklish) | ✗ | ❤️❤️ | · |
   | 小鴨 | ❤️❤️ | ❤️ | · | · | ✗ (the webbed feet) | ❤️❤️ | ❤️ (a waggle) |
   | 九尾狐 | ❤️❤️ (ears) | ❤️ | ❤️ | · | ✗ | · | ❤️❤️ (holds one up to be stroked) |

   Reactions: **loves**: eyes shut in bliss, louder purr or contented sound, hearts, pushes into the hand, held on it rolls over for more or kneads (cats), kicks a hind leg (the Shiba at its tail base), fluffs up (birds); more 親密度. **Likes**: leans into the hand, tail or wings going, a small heart. **Does not mind**: as now. **Dislikes**: a flinch, a look back, ears flat, a step away; kept at it, the grudges there are now. Personality still shifts it (a 厭世 one loves less loudly). Model work it needs: the models give `head`, `belly`, `paw`, `bottom` (and the fox `cheek`); chin and tail come from the bones. A `back` zone apart from `bottom`, `wing` apart from `paw` on the birds, and cheeks on all would have to be added to the model files.

#### Roger's answers on item 4, and hugs (2026-11-12)

- **The table above as proposed**, with: the 曼赤肯's belly loved but **a trap after a while** (about 3 s of rubbing: it hugs the hand with its front paws and bunny-kicks, lets go and looks up pleased; play, not a grudge, 親密度 as for a loved spot); the 折耳貓 simply loves it.
- **Zones from the bones, not the model files** (as chin and tail already are, `mesh::refine_zone`): `wing` (skin moving with `wing_l`/`wing_r`) apart from `paw` on the birds; `back` (the top of the body between `chest` and `butt`) apart from `bottom`; `cheek` on every species (head skin beside the nose, below the eyes). A picture of every pet coloured by zone goes to Roger before the reactions are built.
- **Hugs, new** (Roger: 「再加個抱的動作，除了我可以抱，動物也可以互相抱」):
  - **You hug it**: 「抱抱」 at the top of the menu and a **double-click** on the pet (double-click is unused). The hand becomes two short arms round it; it shuts its eyes, nuzzles, hearts, squashed a little; 親密度 up.
  - **They hug each other**: with 非理性模式 on, two that get on hug now and then by themselves, and after a squabble or a tussle they make up with one; and 「叫牠… › 去抱抱…」 picks whom.
  - **How it looks**: face to face, cheek to cheek, swaying together, the one in front with a paw or wing over the other's shoulder; hearts, a nuzzle each after. **The fox instead stands up for a bear hug**: Roger is shaping her as a **霸道姊姊** (a bossy big sister): she pulls the other into a hug whether it wants one or not.
- **The fox's persona from now on** (Roger, 2026-11-12: 「這就當作以後九尾的人設」): **bossy big sister to the other pets, 傲嬌 to you.** To them she calls herself 姊姊, gives orders (「站好，不准動！」, 「小的們，集合！」) and hugs them whether they want it or not, with a soft core (a pat on the head after, nobody truly afraid of her). To you she is 傲嬌: 「哼」, says no while her tails say yes (「哼……就讓你摸一下。」, 「才不喜歡{you}呢。」). Every fox line was rewritten in this voice (`assets/lines/zh-TW.ron`, the header there says it); the playful 「嘻嘻」 trickster is gone. She no longer names herself 小九 in lines (a renamed fox said the wrong name).

#### Zones built 2026-11-12 (picture to Roger)

`ZONES` gained `back` and `wing` (`Zone::Back`, `Zone::Wing`), worked out at bake time in `mesh::refine_zone` from the bones and the surface normal (forward +z, up +y), no model file touched:
- **wing**: skin moving mostly with a `wing…` bone (the birds); it lifts when held, as a paw does.
- **The body** (the model's `bottom` skin): the **rump** stays `bottom` (round the `butt` soft spot, or a four-legged pet's hindquarters facing back); the rest is **back** where it faces up (a four-legged pet's: up to a little below the side's middle; an upright bird's: everywhere but the front and the underside) and **belly** elsewhere. Before, the whole body of a bird was `bottom`, and a four-legged pet's sides were too.
- **cheek** was already on every species (marks on the face; the fox's cheek tufts).
- Until the reactions are built, back and wing touches go as before: back like the bottom (the head turns to look, a flick), a back held a while counts as a pat (as head, chin, belly, cheek do); a wing like a paw.
- `examples/zones.rs` draws every pet sitting and walking with each pixel tinted by the zone a click there finds (`inbox/zones.png` for Roger). Test `every_mesh_pet_has_a_back_a_rump_and_birds_have_wings`.

#### Touch by zone built 2026-11-12

- **The table** is `Species::fondness(zone) -> Fondness` (`Dislikes`, `Fine`, `Likes`, `Loves`), Roger's rows as approved; `Species::belly_trap()` is 3 s for the 曼赤肯 only.
- **Held a while** (`HOLD_SECS`) anywhere it does not dislike, it is a pat (`Mind::pat(w, zone)`): loved, the line `pat_love`, 16 親密度 and `Out::bliss` (more hearts); liked, `pat` and 10; fine, `pat` and 6. Before, only head, chin, belly and cheek counted.
- **Held on a loved spot** (`App::touch_step`): eyes shut in bliss (a dog panting, `touched_look`), leaning into the hand and a cat treading (`Levels::bliss`), a dog's hind leg going at its loved bottom (the Shiba's tail base), a bird fluffing up, no looking back at a hand on its back or bottom; hearts every 1.6 s from 0.8 s on, a cat purring, a dog panting.
- **A spot it dislikes**: at once ears back, the head turned away, leaning off, eyes squeezed (`Levels::shy`, `Look::Pout`), a paw or wing held tucked instead of given; held 0.35 s (`DISLIKE_SECS`) it hops out from under the finger and says `dislike` (`Mind::dislike`, counted as a poke there, so kept at it builds the grudge as pokes do). A quick poke there says `dislike` too.
- **The 曼赤肯's belly trap**: rubbed 3 s, `Mind::trap` (line `trap`, 「抓到你了！喵！」, a laugh, hearts, 親密度 as a loved spot, no grudge) and `Animator::spring_trap` for 1.8 s (`Touch::trap`): on its back, front paws clutched in, hind legs kicking fast, wriggling, laughing.
- New lines `pat_love` and `dislike` for every species in its own voice, `trap` for the 曼赤肯 (`Line::only_for`).
- Seen: `examples/touch.rs` scenes `back`, `wing`, `trap` (and every scene now takes the species' fondness); `inbox/touch-zones.gif` for Roger. Tests: `each_species_has_spots_it_loves_and_ones_it_does_not`, `a_loved_spot_pleases_more_than_a_pat_and_a_disliked_one_is_said`. Still owed: Roger's look and feel on Windows; the trap reads weakly on the 曼赤肯's short legs in stills.

#### Hugs built 2026-11-12

- **`Act::Hug`** (`HUG_SECS` 3.2 s, sitting) with a role, `anim::Hug`: `Cuddle { front }`, `Bear`, `Squeezed`, `ByYou`, carried to the renderers in `PoseParams::hug` (`Mover::set_hug`). `Animator::hug(role)` starts it whatever else is on (not under a finger), and it lasts through the happy hops that come with it (they cut other gestures short).
- **Poses**: four-legged (`skin.rs`): cuddling, leaning in, head tipped against the other's cheek, swaying together, the one in front with its near front paw over the other's shoulder; the fox's bear hug sitting up tall, both front paws round the other, chin on its head, squeezing; squeezed, paws up and hind legs paddling; in your arms, rocked, nuzzling, front paws up. Birds (`waddle.rs`): wings out round the other, a small one in the fox's arms lifted off its feet, feet paddling. Hugging another pet it turns 58° toward it (`HUG_YAW`) instead of facing you: sitting down had turned them both to face you, side by side rather than in a hug.
- **Between pets** (`skit.rs`, `Kind::Hug`): they walk together until 90 px apart (`HUG_CONTACT`; still 1.8 × that apart when the time is up, they give up), `Cmd::Hug` to each with its role, the one in front (or in the fox's arms) raised over the other; the one who came says `hug` (the fox, whoever came: 「過來，給姊姊抱一下。」), the one the fox held says `bear_hugged` after. A quarter of the skits that start by themselves are hugs (`HUG_SHARE`); a squabble (or 雞同鴨講) ends in one once they have made up; 叫牠… › 去抱抱… lists the other pets (`Director::hug`); clicking a hugging pet does not break it up. `CHONKGENE_SKIT=hug` starts one.
- **You hug it**: 「抱抱」 at the top of the menu (`Icon::Hug`) or a double-click (`DOUBLE_CLICK` 0.4 s; the first click is still a poke). `Mind::hug` (line `hugged`, 14 親密度, more hearts; asleep it stirs; with a grudge it will not); awake and free it sits into your arms, and your arms are drawn round it (`props::hug_arms`, in the hand cursor's colours, coming in round its body and squeezing).
- Seen: `examples/touch.rs` scenes `hug-front`, `hug-back`, `hug-bear`, `hug-squeezed`, `hug-you`; `inbox/hugs.gif` for Roger; under Xvfb the fox bear-hugged the Fold, their windows overlapping as meant (frames from `CHONKGENE_DUMP`, which now takes hugging pets too). Tests: `two_pets_hug_face_to_face_and_the_fox_bear_hugs`, `a_hug_asked_for_goes_over_and_hugs_unless_busy`, `a_hug_lasts_through_its_cheer_and_ends_on_its_own`; the squabble test now ends in a hug.
- Not done: the fox's front paws show over the other only where they clear its body (the other is in front). (Her tails round you when you hug her: built the same day, below.)

#### Cursor curiosity, and small things after the hugs (2026-11-12)

- **Cursor curiosity** (§1.3; Roger's fourth pass, item 6: they hardly ever looked at him; `app/play.rs` `curious`, every frame): with the mouse moving and the cursor within 260 px of its head (`NOTICE_PX`, at 100%), a pet standing at rest may notice it, about `curiosity(species, energy)` a second (cats 0.75, the owl 0.8, Shiba and fox 0.6, chick and duckling 0.5, ± the pet's energy). Noticed, it turns to you and its head follows the cursor (the animator's `watch`, as with the grass stalk), and does not go off on a walk of its own; it looks away 1.5 s after the cursor leaves, or tires of it after 6–9 s and leaves it 8–16 s. Darting past within 130 px faster than 900 px/s, a lively one (by its energy) pounces at it, at most every 6 s; the owl and the duckling only look. Under Xvfb with the pointer swept and darted over the desktop (Gdk pointer warp): 17 looks in a minute, 5 pounces; 3 of 17 ended by the pet going off busy, 13 by the cursor leaving, 1 tired. Tests: `a_watched_spot_draws_its_look_like_a_finger`, `curiosity_goes_by_species_and_liveliness`. Not done: glancing at the cursor while walking, a greeting turn when you come back.
- **The fox's tails round you**: hugged by you she sweeps her nine tails out to both sides and round to the front, one curling under your arms (傲嬌: she says no, her tails say yes).
- **Sounds for the hug lines**: hugged, a cat purrs and the rest greet you; a hug offered is a greeting; caught in the fox's bear hug, a laugh.

### Roger's seventeenth pass (2026-11-12)

Roger's notes, and what was done the same day (all checked here; each owes a look on Windows):

1. **「我的寵物」 minimised and restored shows only the pet.** The software renderer redraws only what changed; the preview moves, so only it came back. The note in the code that Slint redraws everything on a restore was wrong on Windows. Now `repaint_when_restored` watches the window four times a second (`is_minimized`, and any change of size as a fallback) and repaints the whole of it at once and twice more as it settles (the `repaint` property, as `repaint_when_shown` does on opening).
2. **When the page opens, 「說再見」 should be in sight.** It was 280 px below the fold (the window opened at 620 px, the page is about 900). The 「在桌面上」 switch and 「說再見…」 (with its confirmation) moved under the pet's card in the left column, which was empty there, and the window opens 700 px tall (`preferred-height`; `min-height` stays 560): everything shows at once (Xvfb screenshot). The switch's hint is now 「關掉就在寵物窩休息。」 (the old one broke 寵/物 in the narrower card).
3. **The pond a bit bigger; after a while the pets are on top of it, not in the middle.** Seen under Xvfb: with five or six in, their bodies stood above a thin strip of water. Their feet were on the floor line, which is the pond's near edge, and 460 × 96 had no room for them. Now the pond is 640 × 136 (`Thing::Pond.size`) and a pet in it stands further in: its window goes up by a share of the pond's height (`Fun::pond_lift`, 0.24–0.40 by its place, `POND_DEEP` 0.30 by default), eased in and out with the water (`wet`), so its feet are in the water and the waterline at its belly; the places along the water are further apart (±0.36 and ±0.18 of the width). `inbox/pond.png` is a frame of it put together from the windows.
4. **Asleep on its back, the belly's breathing is too small.** `BELLY_UP_BREATH` 0.11 → 0.24: the belly swells 1.6 times as far as before (0.28 brought its top to a point). A measure of the silhouette was swamped by the 「z」 and the snot bubble; frames compared by eye at the top and bottom of a breath show it.
5. **Except the fox, every pet's belly jiggle could show more.** `JIGGLE` 1.6: the belly takes 1.6 times the push from the body's moves and from pokes and may go 0.18 instead of 0.12 (`JIGGLE_MOST`), on four-legged pets (`Walk::jiggle`, 1.0 for the fox) and birds alike.

`inbox/belly.gif` shows items 4 and 5 (two pets asleep on their backs, four poked).

### Roger's eighteenth pass (2026-11-12)

Roger's notes, with his answers to three questions the same day:

1. **Running, sliding and flying** (「寵物可以加入奔跑、滑行、飛行模式」). Sliding is both kinds: along the ground on its belly (after a run, like a penguin; kicking up sand on the beach) and gliding through the air (wings or tails spread, down from a window top). Flying is for everyone: the birds flap, the fox floats by magic, the four-legged ones go up in a funny way (a balloon, a bamboo-copter).
2. **Hugs are hard to set off.** A double-click: the first click is a poke, the pet hops away and the second click misses. Roger's pick: fix the double-click (the first click waits a moment before it counts as a poke; a looser time between clicks).
3. **The hugging arms look odd and ugly**: make them a baby's arms and hands.
4. **The hug itself does not work well**: try it as a small baby hugs a plush toy: arms right round it (環抱), held up in front (前抱), and so on.
5. **The sunbeam bigger.**
6. **A golden sand beach beside the pond** for pets to walk and run on.

#### Eighteenth pass, items 2–5 built (2026-11-12)

- **2. The double-click** (`Pet::poke_due`, `poke_now`): a quick click no longer hops the pet away at once; its poke waits 0.45 s (`DOUBLE_CLICK`, was 0.4) and is dropped if a second click comes, which hugs. A slow press, a rub, a drag are as before. Under Xvfb driven by XTest (libXtst through ctypes): a double-click on 芋圓 hugged it ("hugged (in arms)" 0.21 s after the first press), a single click on 阿柴 poked it 0.5 s later.
- **3, 4. A baby's arms, as a baby hugs a plush toy** (`props::baby_arm`, `hug_arms` with `HugHold`): chubby arms, thickest at the upper arm with a dip at the elbow, a crease at the wrist, a soft pink outline and a light along the top, pudgy fists with a thumb and rosy knuckles (the cursor's dark ink and straight bar are gone). Two holds, one picked at random for each hug: **環抱** (`Wrap`), round its middle from both sides, the hands meeting on its belly, squeezing; **前抱** (`Carry`), up its front from below with the hands under its arms, the pet held up (`hug_lift`: its window rises up to 25 px, bobbing, and comes down as the arms let go). `inbox/baby-hugs.gif` (`examples/touch.rs` scenes `hug-you`, `hug-carry`).
- **5. The sunbeam** 400 × 420 at 100% (was 260 × 300).

#### Eighteenth pass, item 1 built (2026-11-12): running, sliding, gliding, flying

- **Running** (奔跑): a walk is a run a share of the time (0.15 + 0.35 × its energy), at 2.6 times the pace (`RUN_PACE`) for the walk's length (`Pet::dash_until`). A four-legged pet's walk turns into a bound as it speeds up (`skin.rs`: front paws together and hind ones together, the back flexing, a bigger bounce, from tempo 1.7 to 2.5); a bird waddles faster.
- **Sliding on its belly** (滑行, `Motion::Slide`, `Body::slide`): at the end of a run, half the time, it flops down and slides on at 1.2 times its run's speed, slowing at 240 px/s² (`SLIDE_FRICTION`) until it stops by itself, with a puff of dust; off the end of a window it goes over the edge. A four-legged pet lies flat out, front paws reaching forward, hind legs trailing, chin up, ears back; a bird tips forward onto its belly like a penguin, wings back (`Posture::Slide`, `Travel::Slide`).
- **Gliding** (滑行 through the air, `Motion::Glide`, `Body::glide`): once in each fall from more than 160 px up (`Body::drop_below`), a bird nearly always (0.9), the fox mostly (0.8) and the rest half the time spread out and glide down instead: 180 px/s forward, sinking 120 px/s, turning at the edge of the screens, landing softly. Birds spread their wings still, the fox fans her tails, the four-legged ones spread their legs like a flying squirrel (`Posture::Glide`, `Travel::Glide`).
- **Flying, everyone** (`fly_chance`: owl 0.08, chick and duckling 0.05, fox 0.05, the rest 0.025 of the times it sets off): it flies (`Body::fly_to`) to a spot on a floor at least 400 px away (`Pet::flight_spot`). Birds flap as before; the fox floats among her fox-fire orbs; the Shiba and the Munchkin hang under a bunch of three balloons (`props::balloon`, colours by pet, small enough for the room above the head, strings tied at the shoulders), the Fold under a bamboo-copter spinning on its head (`props::copter`), legs dangling and paddling (`Travel::Fly`).
- While sliding, gliding or flying a pet stays side-on to the way it goes (it had turned to face you, the poses unreadable); the glide and flying poses are lifted off the bottom of the picture (the spread legs were cut off).
- **Seen**: `CHONKGENE_STAGE=move` makes runs, slides and flights much more common and (with `CHONKGENE_DUMP`) dumps every frame; under Xvfb in 30 s every pet ran, slid and flew. `inbox/moves.gif` is the desktop put together from those frames; `examples/touch.rs` scenes `slide`, `glide`, `fly` show the poses. Tests: `a_belly_slide_goes_on_a_way_and_stops`, `a_glide_comes_down_slower_and_further_than_a_fall`. Gliding could not be seen in the app here (no windows under Xvfb to step off).

#### Eighteenth pass, item 6 built (2026-11-12): a golden beach

- **`Thing::Beach`** (1180 × 150 at 100%): when a pond comes out, an oval of golden sand is laid under it first (`start_scene`), coming and going with it (the same end, fading out over the pond's last second, `used`; fading in over a second). Drawn pixel by pixel (`props::beach`): warm gold with grains speckled in it, the same every frame, fading into the desktop over its outer fifth (a third read as a haze round the pond), damp and darker in a band round the water; two shells and an orange starfish out on the sand. It lets every click through, and redraws only while fading in or out.
- **On the sand** (`Fun::on_sand`, `Playground::on_beach`): a pet setting off runs 70% of the time; a run's belly slide kicks up golden sand (`Fx::Sand`: puffs and flying grains) instead of grey dust; stopping there, a four-legged one digs now and then (`Act::Dig`, sand flying).
- `inbox/beach.png`: a frame of the pond scene under Xvfb, put together from the windows (`CHONKGENE_DUMP` now writes the beach's window too). Not done: footprints in the sand, a sandcastle.

### Roger's nineteenth pass (2026-11-12)

1. **Is the README a good place for the pets' everyday looks?** Answered in the conversation: yes (a short GIF at the top, a strip of all seven, a few moments); not built until Roger picks how (real Windows recordings or clean renders).
2. **One go for every pet's size** (「希望加入一鍵式調整寵物大小」), built: 大家的大小 with four choices, 小 70%, 中 100%, 大 130%, 特大 160% (`chonkgene_platform::tray::SIZES`, the one list), in three places: the pet's menu (「大家的大小…」, a page with the current one marked 「目前」), the tray (a submenu), and settings 一般 (a row of four buttons, the current one filled; none when the pets differ). `Msg::SizeAll`, `App::size_all`: every pet takes the size; those out on the desktop change where they stand, facing the same way, with a little hop, and without coming in and saying hello again (a single pet's size change still comes in afresh); resting ones keep it for when they come out. An open settings window takes sizes changed elsewhere unless an edit of its own is on its way. The Windows tray code was checked with clippy for the Windows target (stub C compiler); not run there.

### Roger's twentieth pass (feedback only, 2026-11-12; to do next)

1. **The fox, with all she does, still lags: not enough frames.** First suspects: her mesh is by far the heaviest (nine tails, most of her skin is tail), so each frame costs more GPU time and the fair-share budget (`gpu.rs` `Budget`) gives her fewer; her skinning on the CPU (nine tail chains) per frame; and her extra layers (fox-fire, glow, the tails' fan in gestures and hugs). Plan: read her `frames:` lines in `chonkgene.log` from Roger's Windows machine (shown vs asked per second, GPU ms a frame, held back), measure her CPU pose time here against the Shiba's, then pick: a cheaper tail mesh (fewer rings), per-species cost in the budget, or a floor on her frame rate while she moves.
2. **The hug's hands are still odd, and so is how they touch** (「手臂不用出現，只要手指和手腕，並改為透明，可以考慮小孩的手。手要碰到寵物身上，並有觸摸物理互動」). Plan, proposed to Roger the same day: no arms, only a child's hands from the wrist (fingers spread a little), see-through (about 60–70% opaque, soft edges, so the fur shows through); placed on the pet's own outline at its sides each frame (not fixed to the bounds at the start), so they rest on the body; and they press into it: the mesh dents under each hand (the finger dent, `mesh_gpu` `press`, takes one point now: two needed, so a shader change), the body squashes in a little between them and bulges, then springs back; the fingers stroke a little while they hold it.
3. **Scenes called up from the right-click menu too** (「接下來場景也可以用右鍵呼喚出來」). Now a pet's little scenes come by themselves (`maybe_scene`), except the dance (叫牠… › 跳舞, 放聚光燈) and the fox's fire (施法… › 狐火). Plan: a page 「場景…」 in the pet's menu listing its own scenes (the duckling's pond with its beach, the cats' sunbeam and butterfly, the Shiba's bone, the chick's seeds, the owl's branch, the fox's fire) and, for anyone, the shared ones (a pond and beach to get into, a sunbeam to lie in); a scene already out is offered as 「收起來」; asleep or busy, the page says so instead of doing nothing; and the same in the tray (for whichever pet is nearest the middle, or one picked).
4. **Running looks 同手同腳 (both legs of a side together).** Not meant: a bug of the eighteenth pass's run. The walk is a lateral sequence (phases: hind left 0, front left 0.25, hind right 0.5, front right 0.75); the run was to be a bound (front pair 0 and 0.07, hind pair 0.5 and 0.57), but `skin.rs` blends the two by moving each leg's *phase* linearly (`ph + offset + run * (bound - offset)`), and half way the left legs land at 0.25 and 0.125, the right at 0.535 and 0.41: a pace, same-side legs together, every time a run speeds up or slows down (`run` follows the walking speed `s`, which ramps). Plan: blend the joint angles of the two gaits, not their phases (the walk's legs and the gallop's computed each from its own phases, mixed by `run`); and run as a gallop rather than a bound (hind left, hind right a beat apart, then front left, front right, as a dog or a cat runs), checked frame by frame in a side-on render (`examples/touch.rs` gets a `run` scene).
5. **A bigger beach, with ups and downs, and more to do on it** (Roger, 2026-11-13: 「水池黃金沙灘可以再做大一點，有高低起伏。並與動物有更多互動，如狗會挖沙把自己埋起來只留給頭並對你吐舌頭傻笑，在水中有時會對其它同伴噴水，或突然游泳暴走…等」). Plan:
   - **Bigger, with dunes**: wider than 1180, a low ridge or two drawn in the sand (light on the side toward the sky, shade behind). The terrain is flat ledges, so pets will not really climb: a pet on a dune is lifted to follow its outline (as `Fun::pond_lift` lifts one in the water), which reads as walking over it.
   - **The dog buries itself**: it digs (`Act::Dig`, sand flying), lies down in the hole, and a mound of sand is drawn over its body in its own window (as the pond's water is, `compose`), only the head out; it looks at you with its tongue out, grinning (`Look::Pant`), for a while, then bursts out and shakes the sand off. The Shiba first; the fox perhaps, the cats not (they would rather not).
   - **Spraying the others in the water**: a water fight is already built (`water_fight`, sixteenth pass); make it come oftener and show more, and add a spout from the duckling's bill or a dog's mouth at whoever is nearest.
   - **Swimming zoomies**: now and then one in the pond suddenly paddles round it at full speed, a wake and splashes behind it, then stops as if nothing happened.
   - Ideas to offer Roger with it: shaking off water over whoever is near when it gets out, a dog rolling in the sand after a swim (and coming out sandy), footprints that fade, a little sandcastle the chick pecks down.

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

- HiDPI across monitors with different scales (window tops and multiple monitors are in, see "Desktop terrain" in §8).
- English translation of the UI.
- Performance tuning on real machines until every §5 target is met.
- Release pipeline (§2.1), `SHA256SUMS`, code-signing decision (§10).

**Done when**: the manual checklist passes in every environment; the archives on GitHub Releases unpack and run on clean machines.

---

## 9. Later (not in v1)

- **Native Wayland**: `wlr-layer-shell` + foreign-toplevel for KDE and wlroots compositors to close the §4 gaps; a GNOME Shell extension for GNOME.
- **Community skin packs**: data only, never code; validated by the model loader, with size and part-count caps.
- **Reminders**: knowing your routine (telling you to go to bed), water, pomodoro (all optional).
- **More interactions**: none left here: ball / frisbee throwing moved into v1 on 2026-11-03 (§8 "Roger's fourth pass", item 2), the feather-wand / grass-stalk cursor too (§8 "Roger's sixth pass", item 2).
- **Japanese**, more species.
- **小九's human form** (Roger, 2026-11-05): the fox can take the form of a 九尾 fox girl, a grown young woman with a great figure: fox ears, nine tails and the bell kept, in the pets' soft round style (a miko or kimono-style outfit in the fox's red and cream). Planned as its own later step, after the eleventh pass. Design rules: clearly an adult, never a child's proportions; cute and stylish, never explicit. In this form the touch zones change: no belly or bottom pokes, no tickling; a head pat, holding hands or a high five, a poke on the cheek. What it needs: a humanoid mesh and rig (two legs, arms, hair; nothing the four-legged or bird rigs share beyond the tails and ears), a walk and idle set of its own, gestures (a wave, a twirl, casting with a paw-like hand, sitting on a window's edge swinging its legs), a face with more expression, its own lines (still in 小九's voice, §1.11), and the switch itself: a puff of fox-fire and smoke, either on the fox's own (rarely, an 變身術 from the fox's spells, §8 "Roger's eleventh pass", item 6) or from the menu, turning back after a while or when picked up. Open: whether the human form is unlocked by affinity, and whether it uses all of the fox's play (fetch, catnip) or only some.

---

## 10. Open questions

| Question | Affects | Decide by |
|---|---|---|
| **Who draws the art** (Roger, a commission, or my placeholders first) | critical path; cuteness depends on it | end of Phase 1 |
| Sound sources (self-recorded, commissioned, CC0). Decided 2026-11-04: synthesis (built in step I); 2026-11-10: the calls replaced by CC0 recordings Roger picked by ear (fifteenth pass, item 5), the knocks still synthesised | licensing | done |
| The next release's number, now that the `v0.5.0-beta.*` releases are removed. Decided 2026-11-04: `v0.6.0-beta.1`, since the mesh pets, terrain, touch and fidgets replace most of what the 0.5 betas had | tags, README | done |
| Owl's favourite snack | snack content | Phase 5 |
| What 「沒有 ui/ux icon」 means. Roger, 2026-11-03: the 招牌 icon, read as the program's own signature icon (exe, taskbar, Alt-Tab, tray, settings window), to be confirmed when the designs are shown; whether the settings window also gets icons is still open | icon work in §8 "Roger's fourth pass", item 1: the next round | before that work starts |
| Commands (坐下, 趴下, 過來…): available from the start, or still unlocked by affinity like §1.13's tricks. Decided 2026-11-05 (Roger: do it all, as proposed): from the start; affinity only makes a pet quicker about it, a bad mood makes it take its time, never refuse | menu design, nurturing | done |
| Typing reactions on by default? Decided 2026-11-05: on, with a switch 「陪你打字和看書」 under 不打擾; they use input timing only (§1.3, §6) | settings default | done |
| Scene props (branch, pond): appear only while their scene plays, or stay as desktop decorations. Decided 2026-11-05: only while their scene plays | look, screen clutter | done |
| Windows code signing (unsigned triggers SmartScreen warnings; cloud signing is about USD 10/month, to be rechecked). Roger, 2026-11-06: Chrome sometimes blocks the download as dangerous: Safe Browsing rates each file by its hash, so every build starts with no reputation and the verdict varies with the build, how often it has been downloaded, the browser's protection level and whether the lookup answered in time. Meanwhile CI's Windows test build is the zipped portable folder, not a bare `.exe` (a little less likely to be blocked). Plan when it is decided: sign `chonkGene.exe` in `release.yml` with Azure Artifact Signing (formerly Trusted Signing), so reputation builds on the publisher instead of starting over each release | first-run experience, downloads | Phase 6 |
| Whether the GNOME Wayland degradation is acceptable | whether §9's native Wayland moves earlier | after Phase 0 |
| Open-source licence and repo visibility | art, font and sound licences must agree | before going public |
