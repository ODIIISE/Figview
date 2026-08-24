# FIGVIEW — Rebuild Roadmap & Technical Plan

> Version 1.0 · August 2026 · Owner: @ODIIISE
> Status: DRAFT for approval — see §10 for decisions needed before Step 1

---

## 0. TL;DR

Figview stalled because we built the hardest thing first: a custom GPU renderer
with pixel readback, on top of a parser validated against a single old file.
The rebuild inverts that order:

1. **Parse reliably** (the actual moat — nobody has a polished offline .fig viewer)
2. **Render correctly** by leaning on proven engines (browser Canvas2D + resvg as test oracle)
3. **Only then optimize** with GPU if measurements demand it

Target: a read-only, offline, native-feeling `.fig` viewer for Windows that
opens real-world 2026 files (kiwi v100+) in < 2 s and renders common content
faithfully.

---

## 1. Post-mortem: why v1 failed to open your file

| # | Root cause | Evidence |
|---|------------|----------|
| 1 | **Parser validated against one old sample** (kiwi v35, 268 schema defs). Modern files are v100+ with 500+ schema defs, zstd chunks, new node types. Untested paths = hangs/misdecodes. | README sample output vs. fig-kiwi ecosystem docs |
| 2 | **No timeouts or progress anywhere.** One slow/hung step (kiwi decode, image decode, scene build) blocks forever with UI stuck on "Opening…". | commands.rs load path |
| 3 | **Renderer was rebuilt before it was measurable.** GPU readback over JSON IPC, alignment bugs, gradient layouts mismatched — all found by review, none caught by tests because there was nothing to test against. | commit ef036dd → af87683 fixes |
| 4 | **No corpus.** Zero real-world .fig files in CI. Every fix was speculative. | repo has no .fig fixtures |
| 5 | **Monolithic vertical slices.** Parser/renderer/UI changed together; a bug anywhere invalidated everything downstream. | git history |

Lesson encoded into this plan: **each layer must be independently runnable,
testable, and debuggable from the CLI before the next layer consumes it.**

---

## 2. Product vision & positioning

**One-liner:** *"Open any Figma .fig file offline, instantly, without uploading
it to anyone."*

**Non-goals (until at least v2):** editing, collaboration, cloud sync,
plugin system, prototype playback.

### Competitive landscape (researched Aug 2026)

| Tool | Opens .fig directly? | Platform | Model | Gap we exploit |
|------|---------------------|----------|-------|----------------|
| **Figma** (official) | Yes (its own) | Web/desktop | SaaS | Requires account + upload; privacy concern is our reason to exist |
| **Lunacy** (Icons8) | **No** — confirmed 2026; migration docs route users through SVG export | Win/mac/Linux native | Free (asset upsell) | The #1 free desktop design tool cannot open .fig — validates demand |
| **Penpot** | No native import | Web/self-host | OSS | Collaborative editor, not a viewer; heavy for quick inspection |
| **Grida** | Yes — browser parser/viewer, local processing | Web | OSS | Closest competitor; web-only, inspection-oriented; no polished offline desktop app |
| **fig2sketch** (sketch-hq) | Converts .fig→.sketch (then open in Lunacy/Sketch) | CLI | MIT, active (v0.6.0 Jul 2026) | Two-step friction; proves format decodability; **best reference implementation to study** |
| **sunyui/figma-parser**, fig-kiwi npm | Parse-only scripts/libs | Node | OSS | Dev-handoff data extraction; no rendering |

**Positioning statement:** Figview = *Lunacy's offline simplicity* applied to
.fig, *Grida's local parsing* shipped as a native desktop app, with
*fig2sketch-grade parsing rigor*.

**Differentiators to protect:** privacy (zero upload), speed (<2 s open),
works air-gapped, single portable exe.

---

## 3. Requirements

### Functional (v1)

| ID | Requirement |
|----|-------------|
| F1 | Open `.fig` from dialog, drag-drop, CLI arg, file association |
| F2 | Multi-page navigation; page thumbnails |
| F3 | Infinite-canvas pan/zoom (wheel-at-pointer, space-drag, pinch); fit page/selection; 50%–6400% |
| F4 | Layer tree: expand/collapse, visibility icons, click-select, canvas click-select |
| F5 | Properties inspector (position, size, fills, strokes, type, effects) |
| F6 | Render fidelity: rects/rounded/ellipses/vectors/booleans, solid+gradient fills (linear/radial/angular), images, strokes, opacity & blend modes, clips & masks, drop shadows; text with correct family/size/weight/color/wrapping (approximate metrics acceptable, flagged in UI) |
| F7 | Multi-tab documents; recent files list |
| F8 | Export selection/page to PNG @1x/2x and SVG |
| F9 | Graceful degradation report per file ("rendered 94% — unsupported: angular gradients on text") |

### Non-functional

| ID | Requirement |
|----|-------------|
| N1 | Open ≤ 2 s for ≤ 50 MB file on mid-range 2020 hardware; UI never blocked > 100 ms without progress feedback |
| N2 | Pan/zoom ≥ 60 fps on a 10,000-node page (with viewport culling) |
| N3 | Memory ≤ 3× file size while viewing |
| N4 | Windows 10+ x64 first; macOS/Linux same-core later (architecture must not preclude) |
| N5 | Crash-proof parsing: any input either renders, partially renders with a report, or fails with a precise error — never hangs, never crashes the app |
| N6 | Portable exe ≤ 30 MB; optional NSIS installer with .fig association |
| N7 | All parsing/rendering offline; no telemetry by default |

---

## 4. Architecture decision

### Options considered

**A. Keep v1 shape: Rust wgpu offscreen render → CPU readback → IPC blit**
- ✗ Slowest path to correctness: reimplements text, gradients, clipping, blend modes
- ✗ Per-frame IPC bandwidth; GPU init complexity; v1 proved this is a tar pit
- ✓ Full control, single language
- Verdict: **rejected**

**B. Rust parses & serves data; WebView renders (Canvas2D now, WebGPU later)**
- ✓ Browser gives us text shaping w/ OS fonts, gradients, images, compositing, hit-testing primitives — all free and correct
- ✓ 90% less rendering code; iteration in JS is minutes not compiles
- ✓ Pan/zoom = transform on a cached raster/scene → trivially 60 fps
- ⚠ Large-scene transfer: mitigate with per-page lazy loads + typed-array binary chunks (not JSON megabytes)
- ⚠ Canvas2D ceiling on pathological pages: mitigate with culling + tiling; WebGPU escape hatch later
- Verdict: **recommended primary**

**C. Convert scene → SVG → rasterize with resvg (Rust)**
- ✓ resvg is battle-tested: masks, clips, blends, text all correct; ideal **golden-reference renderer** for tests
- ✗ Re-raster per frame too slow for interactive pan/zoom unless tiled
- Verdict: **adopted as the test oracle + SVG export engine (F8), not the live renderer**

**D. Fully native window (winit + wgpu/Vello), no WebView**
- ✓ Max perf, tiny binary
- ✗ Throws away HTML UI productivity; months of work; Vello still maturing
- Verdict: **rejected for v1**

### Chosen architecture (B + C)

```
┌─────────────────────────────── Tauri app ────────────────────────────────┐
│                                                                          │
│  Rust core                          WebView (system WebView2)            │
│  ┌─────────────────────┐            ┌──────────────────────────────┐     │
│  │ fig-parse crate     │  scene     │ ui (TS, no framework)        │     │
│  │  zip/zstd/kiwi      │──chunks──▶ │  Canvas2D scene renderer     │     │
│  │  → SceneDocument    │  (typed    │  overlay: text/selection     │     │
│  │ fig-render crate    │   arrays)  │  camera, culling, tiles      │     │
│  │  layout/bounds      │◀─commands──│  panels: pages/layers/props  │     │
│  │  culling/tiles      │            └──────────────────────────────┘     │
│  │ fig-golden (CLI)    │                                                 │
│  │  scene → SVG→resvg  │── reference PNGs for visual-diff tests ──┐      │
│  └─────────────────────┘                                          ▼      │
│                                    corpus/*.fig → golden/*.png (CI)     │
└───────────────────────────────────────────────────────────────────────────┘
```

Crate boundaries (enforced by workspace):

| Crate | Depends on | Contains |
|-------|-----------|----------|
| `fig-kiwi` (rename of parser) | kiwi-schema, zip, zstd | Archive+binary+schema decode → raw node JSON-ish model. **No rendering types.** |
| `fig-model` | serde | Stable intermediate document model (versioned, serde+bincode). Decouples parse from render forever. |
| `fig-scene` | fig-model | Flattened draw list, bounds, culling structures, tile graph. |
| `fig-golden` (bin) | fig-model, resvg, fontdb | CLI: `inspect`, `render`, `svg`, `bench`. The developer/CI swiss-army knife. |
| `src-tauri` | all above | Commands, state, IPC chunking. Thin. |
| `ui/` (vite-less TS) | — | Renderer, overlay, panels. |

Key rule: **every crate is usable headlessly from the command line.**

### Data transfer protocol (fixing v1's biggest sin)

- Page scene serialized as bincode → transferred as ArrayBuffer (raw IPC),
  decompressed client-side; per-page lazy.
- Text stays in the scene as items rendered by the browser using OS fonts
  (same approach that already worked in v1's original Canvas2D pipeline).
- Camera transforms are computed in JS; Rust holds no render loop at all.
  Rust only serves scenes/images and does export rasterization via resvg.

---

## 4b. How Figma actually renders — and where an offline native viewer can beat it

Primary source: Evan Wallace (Figma co-founder/CTO),
*"Building a professional design tool on the web"* (Figma Engineering blog).

### What Figma built

| Decision | Detail | Why they did it |
|----------|--------|-----------------|
| **C++ core compiled to WASM** (emscripten/asm.js originally) | Own DOM, own compositor, own text layout engine — *"a browser inside a browser"* | Full control of memory layout & allocation; no GC pauses; ~2× native perf |
| **Custom WebGL tile-based renderer** | Retained mode; tiles cached as GPU textures; supports masking, blur, dithered gradients, blend modes, nested opacity; fully anti-aliased | Rejected HTML/SVG/Canvas2D outright |
| **Why NOT Canvas2D** | *"Immediate mode… all geometry has to be re-uploaded to the graphics card every frame. This is needlessly wasteful."* Also: no angular gradients, inconsistent text metrics across browsers | This exact trap is what v1's readback pipeline fell into |
| **Why NOT SVG/DOM** | DOM baggage, geometry re-tessellated after every scale change, inconsistent mask/blend/blur, poor HiDPI AA | |
| Biggest pain point | **No access to glyph outlines/kerning tables in browsers** — fonts must be bundled & shaped inside their WASM sandbox | Browsers hide OS font data |

Modern Figma keeps this architecture (C++/WASM engine + WebGL/WebGPU tiles);
the desktop app is literally the web app in a shell.

### The structural insight

Every Figma constraint exists because **they run inside someone else's
sandbox**. We don't. As a native Windows viewer we can invert each one:

| Figma's constraint (browser sandbox) | Our native advantage |
|--------------------------------------|----------------------|
| WebGL2 only, no reliable compute shaders | **wgpu → Vulkan/DX12**: compute-shader 2D rasterization (**Vello**) — correct fills, clips, blends on GPU at huge path counts |
| Composites through the browser compositor (extra copies) | Direct swapchain: **zero readback, zero IPC pixels** — the v1 sin becomes structurally impossible |
| Workers + SharedArrayBuffer threading limits | Real threads: parallel tessellation & tile rendering across all cores |
| Can't read OS fonts (their #1 pain) | **cosmic-text / Parley** shape with actual system fonts — *better text fidelity than Figma itself* |
| Files live in cloud; cold open = network + wasm compile | Local file + native code; plus **disk-persistent tile cache** per file version → instant reopen, works air-gapped |
| Must stay compatible with every GPU/browser | We control the floor: require DX12/Vulkan-capable HW (2026 baseline), fall back gracefully |

### Renderer strategy (decided at the M2 gate, designed for now)

All three candidates consume the same `fig-scene` draw list, so the choice is
swappable and benchmark-driven — not faith-driven:

1. **Bootstrap — Canvas2D in the WebView** *(M2)*: ships correctness immediately; browser text/gradients/images; known ceiling.
2. **Fallback — WebGPU canvas inside WebView2** (`navigator.gpu`, tile engine in WGSL): big speedup, still one window; auto-selected when available.
3. **Target — Native Vello viewport embedded as a child surface** in the Tauri window (canvas area only; panels stay HTML): true swapchain + compute rasterizer + system-font shaping via cosmic-text. This is the *better-than-Figma* configuration, and it is realistic precisely because we're Windows-first.

M2 exit criteria therefore include a **renderer bake-off**: same corpus page,
same machine, measure cold-open ms, pan/zoom fps at 10k nodes, and visual diff
score vs the resvg oracle. Data picks the track; ambition doesn't.

> Rule preserved from §4: Rust never ships pixels over IPC in any track.
> In tracks 1–2 the *browser* rasterizes; in track 3 the *swapchain* presents.
> Only scene data crosses the boundary, once, lazily per page.

---

## 4c. PIVOT (supersedes §4/§4b tracks 1–2): FULL-NATIVE — zero web technology

**Decision (user, Aug 2026):** drop Tauri/WebView/JS entirely. Figview becomes
a pure-native Rust application. Figma's own blog validates the instinct — they
built *"a browser inside a browser"* because browsers forced them to; we simply
don't have that constraint.

### What we gain
- Direct swapchain presentation (wgpu → DX12/Vulkan) — the readback/IPC class of bugs is structurally impossible
- One process, one memory space: scenes are shared, not serialized across boundaries
- Vello compute-shader rasterization + cosmic-text system-font shaping (§4b advantages, now unconditionally)
- Smaller attack surface, no WebView2 runtime dependency, faster cold start

### What we take on (and the plan for each)

| New responsibility | Choice | Notes |
|--------------------|--------|-------|
| **UI toolkit for panels** (pages/layers/inspector/tabs) | **egui + egui-wgpu + egui_dock**, custom Figma-dark theme | Immediate-mode = fastest iteration; docking/tabs included. Rejected: iced (slower iteration), custom-Vello UI (months of work), WinUI (COM interop pain) |
| **Canvas rasterizer** | **tiny-skia (CPU) primary** · **skia-safe (GPU) escape hatch** · Vello demoted to watchlist | Evidence-based revision (Aug 2026): Vello's own README declares **"alpha state"** — blur/filters unimplemented (#476), glyph caching immature (#204). Unacceptable for a market-quality product. tiny-skia is the engine inside resvg → live renderer and test oracle share engine DNA, making golden-diff tests pixel-meaningful. If profiling demands GPU later, upgrade to Skia (the actual engine inside Figma/Chrome), not an alpha project |
| Canvas composition | Rendered scene presented each frame; egui paints chrome around it; selection handles drawn in-scene | One window, one event loop |
| Windowing / input / DPI | winit; per-monitor DPI scale applied to camera + UI scale factor | Pen/stylus via winit pointer types |
| Renderer duality | Same `Renderer` trait: `tiny_skia` (default) ↔ `skia_gpu` (opt-in, later) | Runtime auto-select with user override in settings |
| Single-instance file open | Named-pipe server; second launch forwards argv then exits | Standard Windows viewer behavior |
| .fig association | Registry entries written by installer; portable mode manual | M5 |
| Logging & crash reports | `tracing` + rolling file appender; panic hook writes minidump + last-log dialog | Replaces webview console |
| Settings & recents | `%APPDATA%\Figview\config.toml` + MRU list | `dirs` crate |
| UI testing without DOM | Headless frames via the `Renderer` trait in CI → golden-image diff | Deterministic; no display or WARP needed |
| Accessibility | Known v1 gap: GPU UI invisible to screen readers. Mitigation: complete keyboard navigation; revisit after 1.0 | Honest limitation, documented |

### Updated crate map (replaces §4 table)

```
figview/                     (bin)   winit loop, egui chrome, Vello canvas, state
├── fig-kiwi/                (lib)   archive/binary/schema decode  ← survives from v1
├── fig-model/               (lib)   stable document model (serde/bincode)
├── fig-scene/               (lib)   flattened draw list, bounds, culling, tiles
├── fig-render-vello/        (lib)   GPU backend: Vello + cosmic-text
├── fig-render-cpu/          (lib)   fallback: tiny-skia backend, same trait
└── fig-golden/              (bin)   inspect/render/svg/gallery/bench CLI + test oracle (resvg)
```

---

## 5. Tech stack

| Layer | Choice | Why |
|-------|--------|-----|
| Shell | **winit (native window)** — Tauri/WebView dropped per §4c | One process, direct presentation, no browser dependency |
| UI toolkit | egui + egui-wgpu + egui_dock, custom theme | Fast iteration, docking panels, mature |
| Canvas raster | **tiny-skia** primary · skia-safe GPU later · Vello watchlist | Vello is alpha (see §4c); tiny-skia = resvg's engine = oracle-consistent; Skia = Figma's own engine class if GPU needed |
| Text | `cosmic-text` + fontdb for shaping/layout; glyphs rasterized by the active backend | System fonts with real metrics — beats Figma's bundled-font sandbox |
| Kiwi decode | keep `kiwi-schema` crate | Self-describing schema per file handles version drift |
| Reference/export | `resvg` (`fig-golden`) | Test oracle + PNG/SVG export engine |
| Tests | cargo test + headless frame diffs via `Renderer` trait + golden images | Deterministic, CI-runnable without a display |

---

## 6. Milestones

Each milestone ends with something runnable and demonstrable. Estimates assume evenings/weekends pace.

### M0 — Diagnosis & corpus *(≈ 1 week)* — **THE CURRENT STEP**
- [ ] `fig-golden inspect <file.fig> --json`: stage-by-stage timing (zip → chunks → zstd → schema defs count → message decode → node count), hard timeout per stage (default 10 s), structured error taxonomy (`E-ZIP`, `E-SCHEMA`, `E-MESSAGE-TIMEOUT`, …)
- [ ] Run against **your stuck file** → root cause named, fixed or ticketed
- [ ] `corpus/` assembled: 20–50 real files (yours + community samples; versions v35 → current). Manifest with sha256, source, version, size. Files stored out-of-repo if private; hashes in-repo.
- [ ] CI job: `inspect --all` green (parse-or-clean-fail within budget)
- **Exit criteria:** no silent hangs possible; every corpus file has a verdict; hang reproduced & explained.

### M1 — Correctness baseline: golden renderer *(≈ 1–2 weeks)*
- [ ] `fig-model`: stable document model + version-tolerant extraction of: shapes, paths (baked fill/stroke geometry), paints (solid/gradients/images), opacity, clips, text runs, images-by-hash
- [ ] `fig-golden render`: model → SVG → resvg → PNG (per page/frame)
- [ ] Side-by-side gallery generator (`fig-golden gallery`) for human eyeballing vs Figma screenshots
- **Exit criteria:** top-10 corpus pages visually match Figma exports ≥ "recognizably identical" on a written checklist; gaps itemized.

### M2 — Native viewer shell (MVP) *(≈ 2–3 weeks)*
- [ ] winit window + egui chrome: tabs, pages panel, layers tree, inspector, status bar (Figma-dark theme)
- [ ] tiny-skia canvas: draw-list execution, camera, culling, per-monitor DPI; dirty-rect repainting; selection handles
- [ ] cosmic-text integration: document text runs with system fonts
- [ ] Open paths: dialog, drag-drop onto window, CLI arg; progress states with cancel
- [ ] **Renderer benchmark** (not bake-off): tiny-skia vs corpus — cold-open ms, pan fps @10k nodes, visual diff vs resvg oracle. Record in `docs/DECISIONS.md`; GPU track (Skia) only if budgets fail.
- **Exit criteria:** open any corpus file < 2 s; 60 fps pan/zoom on largest page; F1–F5 demoable.

### M3 — Fidelity pass *(≈ 2 weeks)*
- [ ] Blend modes, clips/masks, booleans, shadows (Canvas2D equivalents), gradient transform edge cases
- [ ] Degradation report (F9) wired to real capability flags
- [ ] Golden-diff harness: renderer output vs `fig-golden` reference on corpus (perceptual hash threshold)
- **Exit criteria:** F6 checklist signed off on corpus; diff report clean or waived-with-note.

### M4 — Robustness *(≈ 1 week)*
- [ ] Version matrix sweep v35→current; fuzz kiwi decode (cargo-fuzz) 24h clean
- [ ] Memory/time budgets asserted in CI (N1–N3)
- **Exit criteria:** zero hangs/fuzz crashes; budgets met on corpus.

### M5 — Polish & release 0.9 beta *(≈ 1–2 weeks)*
- [ ] Installer (NSIS) with .fig association; portable exe; **single-instance named-pipe open**
- [ ] Recent files MRU + `%APPDATA%` settings; logging + panic-hook crash dialog
- [ ] Export PNG @1x/2x and SVG via `fig-golden` engine in-process
- [ ] Keyboard-complete navigation; error UX; guide refresh
- **Exit criteria:** dogfood daily on your real files for a week; tag v0.9.0.

### M6 — Performance phase (only if needed) *(≈ 1–2 weeks)*
- Triggers: any corpus file violating N1/N2 in M4 runs.
- Options in order: worker OffscreenCanvas tiling → WebGPU backend behind the same draw-list trait.

### M7 — 1.0
- Signed binaries, auto-update check (opt-in), website/README, announcement post targeting the Lunacy-can't-open-fig gap.

---

## 7. Testing strategy

1. **Corpus-first.** Every bug gets a minimal .fig fixture added to the corpus; regressions impossible to reintroduce silently.
2. **Golden images.** `fig-golden` (resvg) is ground truth; the live renderer is diffed against it perceptually (dhash/SSIM) with human-reviewed waivers.
3. **Stage timers + timeouts everywhere.** Any pipeline stage > 10 s logs a warning with counts; > budget fails CI.
4. **Property/fuzz tests** on kiwi chunk parsing (random truncation, bit flips).
5. **UI smoke**: launch app in CI (Windows runner), scripted open of 3 corpus files, assert pixels painted (screenshot diff) — catches "stuck at Opening" class bugs permanently.

---

## 8. Risks & mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| .fig format drift breaks parsing | High (Figma ships constantly) | High | Version detection + tolerant decode + corpus CI + degrade-with-report philosophy (never hard-fail whole file) |
| Text metric drift vs Figma | Certain | Medium | Accept approximate; flag "text approximated" in report; bundle fallback fonts for missing families |
| Huge files (>200 MB) blow memory | Medium | Medium | Streaming zip entries, per-page scenes, image decode LRU cache, budget asserts |
| Scope creep toward editor | High (historical) | High | Hard rule: read-only until 1.0 ships; new ideas go to backlog/issues |
| WebView2 Canvas2D ceiling on giant pages | Low-Medium | Medium | Culling + tiling designed in from M2; WebGPU swap isolated behind draw-list trait |
| Single-maintainer burnout | Real | High | Milestones sized ≤ 2 weeks; every milestone shippable; CLI tools reduce debugging pain |

---

## 9. What we keep from v1 (salvage list)

- `fig-parser` archive/binary/zstd layers — survives as `fig-kiwi` (M0 instruments, doesn't rewrite)
- CI skeleton + MSVC toolchain setup ✔
- Camera math + unit tests ✔
- GitHub CLI workflow, commit/push cadence ✔
- ~~Tauri shell~~ dropped (§4c) · ~~lyon tessellation~~ dropped (draw-list paths consumed natively)

## 10. Decisions needed before coding starts

| # | Question | Status |
|---|----------|--------|
| D1 | Architecture | **ANSWERED: full-native, zero web (§4c)** |
| D2 | Platform scope | **ANSWERED: Windows first** |
| D3 | Provide 10–30 real .fig files (incl. the one that hung) for the corpus? | ⬜ Open — public samples only until provided |
| D4 | Fidelity vs speed priority | **ANSWERED: fidelity first** |
| D5 | Distribution: portable exe only, or also signed installer? | Default: portable first |
| D6 | Product name — cannot ship as "Figma v2" or use "Figma" in the product name (trademark) | ⬜ Open — needs naming before M5 marketing/website |

---

## 11. From viewer to "Figma v2": quality system & go-to-market reality

> Full long-term vision (pillars: design, motion, 3D, shaders, local-first
> collab, on-device AI, plugin platform — with phase ladder and decade-scale
> architecture) lives in **VISION.md**. This section covers only what gates
> market-readiness for Phase A.

The ambition is to build something that changes the market the way Figma
changed it. Here is the honest engineering path to that.

### 11.1 The lesson of Figma vs Sketch/XD

Figma did not beat Sketch by matching it feature-for-feature. It won by
changing **one dimension they structurally couldn't copy** (real-time browser
collaboration), entering through a wedge (free tier, design hand-off), and only
then expanding into a full editor over ~9 years with a large funded team.

Our equivalent wedge — dimensions Figma structurally can't offer:

1. **True offline & local-first** — files never leave the machine; works air-gapped
2. **Native performance** — instant cold start, no browser sandbox ceiling
3. **Privacy by architecture** — nothing to upload, nothing to leak
4. **Open file philosophy** — treat .fig (and later .sketch/.penpot) as first-class citizens

### 11.2 Phased product strategy

| Phase | Product | Milestones | Market role |
|-------|---------|-----------|-------------|
| A | **Best-in-class .fig viewer** | M0–M5 (this roadmap) | Wedge: earn trust, users, and crash-free reputation |
| B | **Viewer++**: compare mode, dev-handoff (measure/spec/export CSS), presentation mode | M6–M8 | Daily-use tool; the app designers keep open next to Figma |
| C | **Editor ambitions** (vector editing, components, prototyping) | Post-1.0, own roadmap | Only after Phase B adoption proves demand |

Hard rule from §8 stands: no editing features before 1.0 ships.

### 11.3 Engineering quality system ("solid, fast, smooth, bug-free")

Market-ready is an operational bar, not a vibe. Gates every release must pass:

| Gate | Target |
|------|--------|
| Crash-free sessions | ≥ 99.5% across corpus dogfooding week |
| Corpus verdicts | 100% parse-or-clean-fail; zero hangs (timeout guards everywhere) |
| Golden-image diffs | Zero unexplained deltas vs resvg oracle on corpus |
| Performance budgets | §3 N1–N3 verified per release on reference machine |
| Pan/zoom smoothness | Frame-time p95 < 16.6 ms on largest corpus page |
| Fuzzing | 24h clean on kiwi decode before each minor release |
| Release train | Patch releases any time; minor features in weekly/biweekly cuts; every commit green in CI |

Process: every bug becomes a corpus fixture (§7); dogfood daily during M5;
opt-in anonymous telemetry (crash counts, feature usage) added at Phase B to
steer priorities — never by default.

### 11.4 Naming & legal guardrails

- Do **not** market as "Figma v2" or include "Figma" in the product name (Figma, Inc. trademark). Positioning language: *"Opens Figma® .fig files offline"* with trademark notice — same pattern fig2sketch uses.
- Choose product name before M5 (website/installer depend on it).

