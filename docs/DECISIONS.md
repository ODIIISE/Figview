# Figview — Decision Log

Every significant architectural/product decision gets a row here.
Format: ID · Date · Decision · Rationale · Evidence · Status

| ID | Date | Decision | Rationale | Evidence / Links | Status |
|----|------|----------|-----------|------------------|--------|
| D-001 | 2026-08-24 | Rebuild around layered crates; every layer runnable headlessly from CLI | v1 failed from monolithic vertical slices with no test seams | ROADMAP §1 post-mortem | Accepted |
| D-002 | 2026-08-24 | Full-native app: winit + egui + tiny-skia; zero web tech (no Tauri/WebView/JS) | User directive ("not web related"); removes IPC-pixel class of bugs entirely; native perf ceiling advantage | ROADMAP §4c | Accepted |
| D-003 | 2026-08-24 | Canvas rasterizer = tiny-skia (CPU) primary; skia-safe GPU only if budgets fail; Vello rejected for now | Vello self-declared alpha (no blur #476, glyph cache #204) — incompatible with market-quality bar; tiny-skia shares engine DNA with resvg oracle → pixel-meaningful tests | github.com/linebender/vello README (Aug 2026) | Accepted |
| D-004 | 2026-08-24 | UI toolkit = egui + egui-wgpu + egui_dock, custom dark theme | Fastest iteration for tool-class panels; docking built-in; mature | ROADMAP §4c table | Accepted |
| D-005 | 2026-08-24 | Windows-first; macOS/Linux later via same core crates | Focus; user confirmed | Chat decision | Accepted |
| D-006 | 2026-08-24 | Fidelity over speed when they conflict | User confirmed | Chat decision | Accepted |
| D-007 | 2026-08-24 | Product phases A (viewer) → B (viewer++/handoff) → C (editor); no editing before 1.0 | Figma beat Sketch via one wedge dimension, not feature parity; solo-dev scope control | ROADMAP §11.2 | Accepted |
| D-008 | — | Product name (cannot use "Figma" in name; trademark) | Needed before installer/website (M5) | ROADMAP §11.4 | **OPEN** |
| D-009 | — | Corpus files: user to provide 10–30 real .fig incl. the hang repro | Public-only corpus is much weaker | ROADMAP D3 | **OPEN** |
| D-010 | 2026-08-24 | Web-export contract (VISION §4b): dual representation — semantic document tree preserved alongside render draw lists; text never baked to paths; layout info retained for CSS translation. Phase B ships frame→HTML/CSS/SVG export; Framer/Webflow-class publishing is a long-term pillar (P8) | User directive: "everything in the canvas should be easy to export for web"; design↔code convergence is the decade's strategic current | VISION §4b, §5.1 | Accepted |
