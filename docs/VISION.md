# FIGVIEW — Vision: From Viewer to the Design Platform

> Version 0.1 · August 2026 · companion to ROADMAP.md (execution) and DECISIONS.md (log)
> This document is the dream with an engineering spine.

---

## 1. The Dream

**A design platform that opens instantly, runs entirely on your machine, never
holds your work hostage, and out-renders the cloud — then grows into the best
designing, prototyping, animating, 3D-capable, AI-assisted editor ever built.**

Where Figma says *"your files live in our cloud"*, we say *"your files live in
Git."* Where Figma lags on a 200-frame file, we hold 120 fps. Where Figma
bolted AI onto a subscription, we run assistive intelligence locally by default.
And unlike every walled garden before it, the format is open, diffable, and
forever yours.

We don't out-Figma Figma. We make Figma's model look like the fax machine of
design tools.

---

## 2. What people actually wish these tools were

Community pain points (forums, Reddit, HN threads, migration guides), mapped to
our answers:

| # | What people say | Our answer |
|---|-----------------|------------|
| 1 | *"Figma chokes on big files — 10k frames and it's a slideshow"* | Native renderer, tile cache, culling; performance budgets enforced per release |
| 2 | *"I can't work on a plane / client site with no WiFi"* | Fully offline core; optional sync only |
| 3 | *"$15–45/editor/month forever, even for viewers"* | Free viewer forever; one-time or cheap tiers later; no seat tax for looking |
| 4 | *"My life's work lives in someone else's database"* | Open, text-based, Git-native file format; export always free |
| 5 | *"Smart Animate is cute, not real animation"* | Timeline keyframes, springs, state machines (Pillar P2) |
| 6 | *"No real 3D anywhere in my UI tool"* | glTF assets in-canvas with materials & lighting (P3) |
| 7 | *"I want custom shaders/effects, not 8 canned blurs"* | Node-based shader/material system (P4) |
| 8 | *"Branching & merging designs is a nightmare"* | Files designed for Git from day one; visual branch/merge in-app (P5) |
| 9 | *"Dev handoff got paywalled and worse"* | Handoff is free and best-in-class (Phase B wedge) |
| 10 | *"AI features feel bolted-on and send my work to their servers"* | On-device models by default; BYO-key cloud opt-in (P6) |

---

## 3. The 2026 battlefield (researched Aug 2026)

### 3.1 Incumbents & challengers

| Player | Strengths | Exposed flank |
|--------|-----------|---------------|
| **Figma** (+AI, Make, Sites, Buzz) | Network effects, ecosystem, Dev Mode | Cloud-only, perf ceiling, pricing resentment, AI bolted on |
| **Google Stitch** | Free vibe-design, voice canvas, infinite canvas; **its March 2026 update dropped Figma's stock ~12%** | Ideation only; Material bias; not a production tool |
| **v0 / Bolt.new / Lovable** | Prompt→working app; devs love them | Design precision, traceability, vendor lock-in |
| **Penpot** | OSS, self-host, standards-based | Web-only perf, no .fig import, weaker polish |
| **Sketch + fig2sketch** | Native mac, .fig import via converter | Mac-only, shrinking |
| **Lunacy** | Free native offline, Sketch-native | No .fig support at all |
| **Rive / Jitter / LottieFiles** | Real motion/interactive runtime | Not general design tools |
| **Spline / Womp** | Accessible 3D design | Not UI design tools |

### 3.2 The two strategic currents that matter

1. **Design ↔ code convergence.** Figma Make, Stitch-export-to-code, Bolt's
   Figma import — the boundary is dissolving. Whoever owns the *interchange
   point* between intent and code owns the next decade.
2. **The "Stitch moment".** A giant gave away ideation-grade design for free
   and moved a public stock. Expect more free generation. **Generation is not a
   moat; refinement is.** The unsolved problem is what happens *after*
   generation: precision editing, design systems, versioning, handoff — exactly
   where local-first native performance wins.

### 3.3 Honest threat assessment

The existential risk isn't Figma. It's that **generated apps skip design files
entirely**, making `.fig` irrelevant the way `.psd` receded. Counter-strategy:

- Become **the universal design-file hub**: open/import/export .fig, .sketch,
  .penpot, SVG; be the tool generated artifacts flow *into* for refinement.
- Own the **open local format** so Git-native workflows have a standard.
- Ship the **handoff/dev experience** free while incumbents paywall it.

---

## 4. Product pillars (the big dream, concretized)

| Pillar | Promise | Key capabilities |
|--------|---------|------------------|
| **P1 Design Engine** | Precision editing at native speed | Vectors/pens/booleans, auto-layout², components+variants+props, styles/tokens (W3C DTCG), constraints, grids |
| **P2 Prototyping & Motion** | "Rive-class animation inside a design tool" | Timeline keyframes with easing curves, spring physics, smart-animate++, interactive state machines, variables/expressions, scroll & input events, Lottie import/export |
| **P3 3D Pipeline** | "Place real 3D in your UI designs" | Import glTF/GLB/USDZ; orbit/place/light in canvas; PBR-lite materials; render-to-layer compositing with 2D scenes; export stills & turntables |
| **P4 Shaders & Materials** | "Infinite effects, not eight canned blurs" | Node-based material editor; WGSL/GLSL shader layers with live preview through the same raster pipeline; safe sandboxed execution; shareable material packs |
| **P5 Local-first Collaboration** | "Multiplayer when you want it, Git when you don't" | CRDT sync (optional relay, self-hostable); plain-text chunked format → branch/diff/merge in-app; comment threads anchored to geometry |
| **P6 AI Copilot (private)** | "A scenario-based assistant in every corner" | **Deferred integration, reserved hooks (D-011).** Principle: the assistant is *contextual* — on canvas it assists drawing, in layer tree it assists organizing, in inspector it explains properties, in export dialog it fixes web-output issues, per page it reports problems. Architecture ships the seams now: every surface exposes a context snapshot + all actions flow through the serializable command bus, so any future agent (local model or BYO-key API) plugs in per-scenario without refactor. Local-first always; cloud calls opt-in and labeled |
| **P7 Platform** | "An ecosystem, not an app" | WASM plugin API (wasmtime sandbox), CLI (`fig` command), headless render farm mode |
| **P8 Publish to Web** | "Every canvas becomes a real website" | Framer/Webflow-class export & publishing (see §4b) |

### 4b. Publish to Web — the export-first contract

Framer and Webflow proved designers will adopt a tool whose canvas *is* the
website. Our contract: **everything drawn can become clean, semantic,
responsive web output — never a screenshot.**

| Canvas concept | Web translation |
|----------------|-----------------|
| Frames with auto-layout | Flexbox/Grid containers (not absolutely-positioned div soup) |
| Text layers | Real selectable HTML text with webfont loading, correct hierarchy tags |
| Vectors/booleans/icons | Inline `<svg>` symbols with `<use>` deduplication |
| Solid/gradient fills, shadows, radii, opacity, blend modes | Native CSS where expressible (most cases); fallback to inline SVG filters only when needed |
| Images | Optimized AVIF/WebP + `srcset`, lazy loading |
| Components/variants | Web Components or framework components (React/Vue/Svelte targets) |
| Interactions (hover, click states from P2 later) | CSS transitions/state classes; prototypes become clickable sites |
| Pages | Routes or single-page anchors; sitemap from page tree |
| 3D layers (P3) | Embedded lightweight WebGL viewer component |

Delivery modes, in order:
1. **Export folder** — static HTML/CSS/JS + assets zip (works offline, Git-friendly)
2. **Publish** — one-click deploy of static bundle to user's own host (Netlify/Vercel/GH Pages/SFTP); we stay out of the hosting business
3. **Inspect mode** — click any element on canvas → see (and copy) the exact HTML/CSS it would produce

Quality gate: exported markup must pass a linter and be readable by a
developer — "View Source should not embarrass us."

---

## 5. Architecture for a decade

### 5.1 Crate topology (grows, never rewrites)

```
                         ┌────────────────────────────┐
                         │        figview (bin)       │  winit + egui shell
                         └──────────┬─────────────────┘
              ┌─────────────────────┼─────────────────────────┐
      ┌───────▼──────┐      ┌───────▼───────┐         ┌───────▼────────┐
      │  fig-edit    │      │  fig-render-* │         │   fig-golden   │
      │ commands,    │      │ tiny-skia CPU │         │ inspect/render/│
      │ transactions │      │ skia GPU (P4) │         │ svg/bench CLI  │
      │ undo, history│      └───────┬───────┘         └───────┬────────┘
      └───────┬──────┘              │                          │
      ┌───────▼──────┐      ┌───────▼───────┐          ┌───────▼────────┐
      │  fig-scene   │◀─────│  fig-3d (P3)  │          │ fig-plugins(P7)│
      │ draw lists,  │      │ glTF, PBR     │          │ wasmtime sandbox│
      │ culling,tiles│      └───────────────┘          └────────────────┘
      └───────┬──────┘
      ┌───────▼──────┐   ┌───────────────┐   ┌────────────────┐
      │  fig-model   │   │ fig-motion(P2)│   │ fig-shader (P4)│
      │ doc model    │   │ timelines,    │   │ node graph,    │
      └───────┬──────┘   │ state machines│   │ WGSL eval      │
      ┌───────▼──────┐   └───────────────┘   └────────────────┘
      │  fig-kiwi    │   ← .fig import (exists today)
      ├──────────────┤
      │ fig-format   │   ← OUR open format: text chunks, schema-versioned,
      └──────────────┘     Git-diffable, lossless round-trip
      ┌──────────────┐
      │ fig-collab   │   ← CRDT sync, optional self-host relay (P5)
      └──────────────┘
```

Non-negotiables:
- **Every crate headless-runnable.** If it can't be CLI-tested, it's wrong.
- **Format before features.** `fig-format` ships stable in Phase B; losing the format means losing the moat.
- **Editing = commands.** All mutations are serializable, replayable commands (property-testable, gives undo for free, later drives collab — **and makes any future AI agent just another command producer**, see D-011).
- **Renderer trait stays narrow.** CPU/GPU swap remains a config choice forever.
- **Dual representation (web-export contract, §4b).** The scene exists twice: a *render-optimized* draw list (what the canvas rasterizes) and the *semantic document tree* (layout, text runs, styles, hierarchy). Renderers consume only the former; **P8 export consumes only the latter.** Baking text to paths or flattening layout in the model is forbidden — it would make Framer-class web export impossible later.
- **AI seams without AI code (D-011).** Reserve a `fig-assist` crate slot defining two traits from Phase A onward: `ContextProvider` (what each surface knows: selection, page diagnostics, layer metadata) and `ScenarioRegistry` (per-surface assistant scenarios). No model, no API calls ship until Phase 7 — but no refactor either.

### 5.2 Lifecycle & support policy

- Semver everywhere; format versions carry explicit stability guarantees (readers support N-2).
- LTS releases yearly; security patches 18 months.
- Format spec published as its own repo with golden corpus — the ".sketch docs" play that made Sketch's format an ecosystem asset.
- Crash-free ≥ 99.5% gate per release (ROADMAP §11.3).

---

## 6. Phase ladder (quarters are honest estimates for a small team)

| Phase | Name | Ships | Exit criteria | Est. |
|-------|------|-------|---------------|------|
| **0** | Foundation | Diagnostics, corpus, golden oracle | Zero-hang guarantee on corpus | ✅ current |
| **1** | Viewer GA 1.0 | Full-fidelity read-only viewer, installer, associations | §11.3 quality gates green; daily dogfooding | Q4 2026 |
| **2** | Viewer++ | Measure/spec/CSS-Swift-tokens handoff, compare diffing, presentation mode, **first web export: frame → clean HTML/CSS/SVG (P8 contract)** | Designers use it *next to* Figma daily; exported frames look right in a browser | Q1–Q2 2027 |
| **3** | Editor Alpha | Select/move/edit vectors/text, auto-layout¹, **command system + undo lands here** | Edit own templates end-to-end; crash-free under edit load | Q3 2027 → |
| **4** | Collab & Format 1.0 | fig-format 1.0 spec, Git workflow UX, optional CRDT multiplayer | Branch/merge demo beats Figma branching | 2028 |
| **5** | Motion | Pillar P2 timeline + state machines | Interactive prototype exported to web/runtime | 2028 |
| **6** | 3D & Shaders | P3 + P4 | 3D-in-UI demo renders through same pipeline | 2029 |
| **7** | Copilot & Platform | P6 on-device AI, P7 plugin API GA | Third-party plugin; AI works air-gapped | 2029+ |

Phases overlap where teams allow; order protects the moat sequence:
**trust → utility → creation → ecosystem.**

---

## 7. Long-game risks (beyond ROADMAP §8)

| Risk | Mitigation |
|------|-----------|
| Generated-apps bypass design files (existential) | Hub strategy §3.3; import everything; handoff excellence |
| Giants give away refinement too | Local-first + privacy + format ownership are structurally unavailable to ad/cloud businesses |
| Solo/small-team bandwidth across 7 pillars | Strict phase gates; pillars unlocked only after previous exit criteria; community RFCs once format ships |
| egui/dependency churn over years | Thin wrapper layer around UI toolkit; renderer/UI swaps isolated by traits (proven by this pivot already) |
| Trademark/ecosystem politics around .fig | Interop ≠ infringement; follow fig2sketch precedent; name/branding early (D-008) |

---

## 8. What this changes today

Nothing in the immediate plan — deliberately. Phase 0/1 (M0–M5) is the
foundation every pillar stands on. The vision's job is to ensure each near-term
decision keeps future doors open (format-first, commands-not-mutations,
narrow traits).

**Next concrete step remains M0:** diagnostic CLI + corpus — starting with
your hung .fig file.
