# FIG Viewer

**Offline Figma `.fig` file viewer** — built with Rust + Tauri + Canvas 2D.

Open `.fig` files locally, inspect pages and layers, and view designs on a canvas — without uploading anything to Figma.

## Features (v1)

- 📂 Open `.fig` files (double-click, Ctrl+O, drag & drop)
- 📑 Multi-tab support for multiple documents
- 📄 Pages panel with page switching
- 🌳 Layers tree with expand/collapse and type icons
- 🎨 Canvas 2D rendering: shapes, fills, strokes, text, transforms
- 🔍 Zoom (mouse wheel, buttons, fit-to-screen, 100%)
- 📏 Properties inspector for selected objects
- 🖱️ Layer ↔ canvas selection synchronization
- 🌑 Dark mode UI inspired by Figma

## Architecture

```
.fig file (ZIP) → Rust parser (kiwi-schema) → Document model → Canvas 2D renderer
```

| Component | Technology |
|-----------|-----------|
| `.fig` parser | Rust (`fig-parser` crate) |
| Binary protocol | `kiwi-schema` (Evan Wallace, MIT) |
| Desktop shell | Tauri 2.x |
| UI rendering | HTML5 Canvas 2D + Vanilla TypeScript |
| Installer | Tauri NSIS bundler (Windows) |

## Project Structure

```
├── crates/
│   ├── fig-document/     # Platform-independent document types
│   └── fig-parser/       # .fig → FigDocument parsing pipeline
├── src-tauri/            # Tauri backend (commands, state, platform)
├── src/                  # Frontend (TypeScript + HTML + CSS)
├── Cargo.toml            # Workspace root
└── package.json
```

## Build

### Prerequisites

- Rust 1.80+ (`rustup`)
- Node.js 18+
- npm

### Development

```bash
# Install dependencies
npm install

# Build Rust crates
cargo build

# Run parser test (inspect a .fig file)
cargo run --example inspect -- path/to/your/file.fig

# Run Tauri dev server
cargo tauri dev
```

### Production build (Windows)

```bash
cargo tauri build
```

## Test with the sample file

```bash
cargo run --example inspect -- "Clothing Store App _ Fashion E-Commerce App.fig"
```

Output:
```
Parsing: Clothing Store App _ Fashion E-Commerce App.fig
  Prelude: fig-kiwi
  Version: 35
  Schema defs: 268
  File: Clothing Store App / Fashion E-Commerce App
  Pages: 1
    Page: Page 1 (0:1)
      Children: 64
        0:999: Splash (Frame)
        0:1025: Welcome Screen (Frame)
        ...
  Total nodes: 4298
  Images: 10
```

## Milestones

| Milestone | Status |
|-----------|--------|
| M1 — Foundation (scaffold + parser) | ✅ Done |
| M2 — Navigation (pages, layers, tabs) | ✅ Done |
| M3 — Canvas rendering | ✅ Done |
| M4 — Advanced (vectors, effects, components) | 🔜 In progress |
| M5 — Polish (installer, auto-update, perf) | 🔜 Planned |

## License

MIT

## Disclaimer

Figma is a trademark of Figma, Inc. This project is not affiliated with, endorsed by, or sponsored by Figma, Inc.