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

## 5. Tech stack

| Layer | Choice | Why |
|-------|--------|-----|
| Shell | Tauri 2.x (keep) | Already working; small binaries; WebView2 present on Win10+ |
| Language | Rust + TypeScript | Existing skill set; kiwi crates are Rust-native |
| Kiwi decode | keep `kiwi-schema` crate | Self-describing schema per file handles version drift |
| Reference raster | `resvg` + `tiny-skia` + `fontdb` | Industry-standard fidelity incl. text/shadows/masks |
| Live renderer | Canvas2D (phase 1) → optional WebGPU (phase 3) | Correctness first, speed second |
| Tests | cargo test + playwright-free DOM smoke + golden-image diffs | Deterministic, CI-runnable |
| Serialization | bincode + flate2 for scene chunks | Compact, fast, typed |

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

### M2 — New viewer shell (MVP) *(≈ 2 weeks)*
- [ ] Scene chunk serialization + lazy per-page IPC
- [ ] Canvas2D renderer: draw-list execution, camera, culling, DPR handling
- [ ] Overlay: text, selection outline; panels: tabs/pages/layers/properties
- [ ] Open paths: dialog/drag-drop/CLI arg; progress states with cancel
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

### M5 — Polish & release 0.9 beta *(≈ 1 week)*
- [ ] Installer + .fig association + recent files + export PNG/SVG (F7, F8)
- [ ] Icon/branding, error UX, guide refresh
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

- Tauri shell, capabilities, CSP, CI skeleton, MSVC toolchain setup ✔
- `fig-parser` archive/binary/zstd layers (M0 will instrument rather than rewrite them)
- lyon-based tessellation learnings (dropped for now; Canvas2D paths natively consume path data)
- Camera math + unit tests ✔ (ported into ui/)
- GitHub CLI workflow, commit/push cadence ✔

## 10. Decisions needed before coding starts

| # | Question | Default if no answer |
|---|----------|---------------------|
| D1 | Architecture B+C sign-off (WebView renders; resvg oracle)? | Proceed with B+C |
| D2 | Windows-only first, macOS/Linux later? | Yes — Windows first |
| D3 | Can you provide 10–30 real .fig files (incl. the one that hung) for the corpus? | Public samples only — much weaker |
| D4 | Priority when fidelity conflicts with speed: correct-but-slower, or fast-but-approximate? | Correct first, optimize in M6 |
| D5 | Distribution: portable exe only, or also signed installer? | Portable first (matches v1) |
