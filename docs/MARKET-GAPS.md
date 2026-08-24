# The Big Dreams & The Missed Spots

> Companion to VISION.md · Research snapshot Aug 2026
> Sources: UX tooling press (Muzli, UXTools), Figma State of the Designer /
> AI reports, migration guides, community wish-lists, competitor roadmaps.

---

## Part 1 — Every big dream people are talking about

| # | Dream | What believers envision | Who's chasing it | Maturity |
|---|-------|------------------------|------------------|----------|
| 1 | **AI-native design** | Describe intent → production design; AI as creative director | Stitch, Figma Make/AI, Lovable, v0, Bolt, Uizard/Miro, Flowstep | Real products; precision still weak |
| 2 | **Design↔code unification** | One flow from canvas to shipping code; code becomes source of truth | Figma Make, Onlook, Builder.io, Locofy, Cursor+Figma-MCP | Momentum leader ("the trend with the most momentum" — Muzli) |
| 3 | **Vibe design → refine pipeline** | Generate 10 directions, then *craft* the winner | Stitch (ideation) → Figma (refinement); nobody owns both ends | The seam is the opportunity |
| 4 | **Real motion & interactive runtime** | Designs that behave, not mockups that flip | Rive, Jitter, LottieFiles, Figma Motion (new) | Rive owns runtime; design-tool integration shallow |
| 5 | **Shaders/materials in UI tools** | Custom GLSL/WGSL effects as first-class layers | **Figma Shaders (Beta, 2026)**, niche plugins | Just validated by Figma itself — table stakes incoming |
| 6 | **3D-native product design** | Place real 3D in UI; spatial/visionOS era | Spline, Womp, Unity adjacency | Fragmented; no design-tool integration |
| 7 | **Local-first ownership** | Files on disk, Git-native, offline forever, CRDT sync later | Penpot (partially), Ink&Switch research lineage | Almost nobody ships it well |
| 8 | **Open formats & interop** | No walled gardens; .fig/.sketch/.penpot freely translated | fig2sketch, Grida, us | Underserved; fidelity guarantees rare |
| 9 | **Git for designs done right** | Branch, diff visually, merge like code | Abstract (RIP, was Sketch-only), Figma branching (weak) | **Graveyard = opportunity** |
| 10 | **Accessibility-first authoring** | Contrast/screen-reader/keyboard issues caught while designing | Almost nobody (linters bolt onto dev side) | Wide open |
| 11 | **Real-data design** | Mockups bound to live APIs/CMS content | Framer (partial), data-plugins | Missed broadly |
| 12 | **Parametric/generative systems** | Rule-driven layouts, procedural components | Early plugins, research demos | Pre-product |
| 13 | **Agentic file operations** | AI agent opens files, executes multi-step edits, reports back | Figma agent (Beta), Claude-Code-to-Canvas bridges | Nascent |
| 14 | **Publish-from-canvas websites** | Design IS the website | Framer, Webflow | Proven market; not served for imported files |

---

## Part 2 — The spots the market missed

Ranked by (demand evidence × weakness of incumbents × fit for us):

### 🕳️ Gap 1 — Precise refinement of *existing* complex files
Every AI tool generates new things fast but fails at surgical edits in real
production files (EPAM's hands-on: iterations, visual bugs, code cleanup).
Figma's own data: designer AI satisfaction 69% vs developers 82%; only 54%
say AI improves their *quality*. **The refinement gap is the decade's product
hole** — and it requires exactly what we're building: perfect file parsing,
native speed, deterministic rendering.
→ *Our play:* Phase A–C. The viewer that knows your file better than Figma does.

### 🕳️ Gap 2 — Git-grade version control for design
Abstract died trying (Sketch-locked, pre-CRDT era). Figma branching is
tolerated, not loved. Visual diff/merge of design files remains unsolved —
blocked precisely by proprietary binary formats. An **open, text-chunked,
diffable format + merge UI** has never existed.
→ *Our play:* Phase D (fig-format + branch/merge). This is a moat, not a feature.

### 🕳️ Gap 3 — Offline/private professional grade
Lunacy proved demand (free, native, offline) but can't read .fig. Enterprises
with air-gapped security requirements have literally no option. Privacy
regret about cloud design files keeps growing.
→ *Our play:* Phase A wedge. Already our entire thesis.

### 🕳️ Gap 4 — Accessibility-by-construction
Contrast, focus order, touch targets, screen-reader trees get checked *after*
design, if ever, by separate tools. An editor that flags/fixes a11y **while
you design** doesn't exist. Regulatory pressure (EAA 2025+, ADA lawsuits)
keeps rising.
→ *Our play:* Phase B add-on; cheap to build once the semantic tree exists (D-010 dual representation).

### 🕳️ Gap 5 — Free, excellent viewer/handoff tier
Dev Mode paywall backlash is real and ongoing. Watching/inspecting/measuring
should be free — monetize creation and publishing.
→ *Our play:* Permanent free viewer = distribution engine.

### 🕳️ Gap 6 — True web-export parity for *imported* files
Framer/Webflow export their own canvases beautifully; every tool that imports
foreign files exports junk. Clean HTML/CSS/SVG from .fig imports: unsolved
(our P8/D-010 contract attacks exactly this).

### Honorable mentions (real gaps, wrong time for us)
- Live-data-bound mockups · Parametric layout engines · Cross-platform app
  generation (FlutterFlow lane) · Print/multi-output · Voice/spatial interface
  design

---

## Part 3 — What we claim vs deliberately skip

| Claim (phases) | Deliberately skip |
|----------------|-------------------|
| G1 refinement engine (A→C) | Front-line image generation (saturated; integrate later) |
| G2 Git-for-design via open format (D) | Full-stack app generation (Lovable/Bolt lane) |
| G3 offline/private native core (A) | Whiteboarding (FigJam/tldraw lane) |
| G4 a11y authoring (B add-on) | Usability testing/research AI (Maze lane) |
| G5 free viewer+handoff forever (A/B) | Hosting business (use user's own infra) |
| G6 web export from foreign files (P8) | |
| Shaders & 3D pillars (P3/P4) — now validated by Figma Shaders beta | |

---

## Bottom line

The market missed the **plumbing**: file fidelity, version truth, privacy,
accessibility, and honest web output. Everyone is racing to generate shiny
first drafts; nobody owns trustworthy refinement of real work. That plumbing —
unglamorous, engineering-heavy, moat-deep — is exactly what a small team with
a native Rust core can win.
