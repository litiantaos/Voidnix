# Voidnix

**English** | [简体中文](./README.zh-CN.md)

A launcher for macOS. Modular architecture, minimal design, native performance.

## Tech Stack

Tauri + Rust | Vue 3 + Vite + Bun | UnoCSS | Pinia | SQLite

## User Guide

### Installation

Download the `.dmg` from the [latest Release](https://github.com/litiantaos/Voidnix/releases/latest) and drag the app to `/Applications`. If Gatekeeper blocks the first launch: open **System Settings → Privacy & Security** and click **Open Anyway**. The app is signed but not notarized; this is expected.

### Basics

- Runs in the background after launch, no Dock icon
- No menu bar icon by default; one appears while the proxy is connected or keep-awake is on
- `Option+Space` toggles the main window; it hides automatically on focus loss
- `↑↓` move through the list, `Enter` run, `Escape` back out one level (subview → module → hide window)
- `Cmd+Enter` shows the result action menu (apps, files/folders)
- When a right-side accessory area is present, `Tab` moves focus between the search bar and its controls

### Global Search

- The default list shows apps ranked by usage frequency
- As you type, apps, files/folders and extensions are searched; results are grouped apps → extensions → files → clipboard → quick actions
- Type `/` to list extensions
- `//query` searches Google, `//b query` Bing, `//example.com` opens a link

### Calculator

Type a math expression anywhere for an instant answer (e.g. `1+2*3`); `Enter` copies the result; open the module for history.

### Currency

Type an amount and currency for instant conversion (e.g. `100 USD`, `20000 JPY`); the module shows all currencies and reference rates.

### IP

Inside the module: an empty query shows your public IP; type an IP to look up its location.

### Timestamp

Inside the module: an empty query shows the current time; enter a Unix timestamp or a date to convert both ways.

### UUID

Generates UUID v4 / NanoID inside the module.

### Base64

Encode and decode text inside the module.

### Notes

`Option+N` opens a scratchpad. Content is saved as you type and restored on relaunch; it can be cleared in Settings. Every character is animated — typed characters pop in, deleted ones drift away, reflows slide into place, and the cursor trails elastically — including during IME composition.

### Clipboard

Text, images and files are recorded automatically in the background. `Option+C` opens the history with search, favorites, multi-select and editing; `Enter` pastes; retention is configurable in days.

### AI Providers

Manage OpenAI-compatible URL / key / model entries in one place, shared by Translate, Agent and external tools (OpenCode, Grok Build, etc.). Multiple keys per provider; Zhipu shows its 5h / 7d / 30d quota curves, DeepSeek shows the account balance. Saving writes `ai.env` and idempotently injects it into your shell rc files under a fully private namespace (`VOIDNIX_ZHIPU_*` / `VOIDNIX_DEEPSEEK_*`), never taking over the generic variable names other tools rely on.

### Translate

`Option+T` grabs the current selection and translates it, or you can type directly inside the module. Chinese↔English direction flips automatically; results can be spoken aloud; Youdao or AI engines can be configured, with multiple engines side by side.

### AI Agent

`Option+A` opens chat with tool calling (web search, command execution). Sessions are saved and restored across restarts; provider and model are configurable; commands run without confirmation by default.

### Screenshot

`Option+S` captures a region, then annotate, run OCR and QR recognition, pin to screen, or take a scrolling screenshot. In the selection stage, press `F` for full screen or `C` to pick and copy a color. Saves to Downloads by default; the path is configurable. Requires Screen Recording permission.

### Finder Tools

`Option+F` copies paths, opens in Terminal, creates new files, and toggles hidden files — inside Finder. Requires Accessibility permission.

### Window Management

Once enabled, moving the pointer to the top center of any screen summons the snap panel with custom sizes; in multi-screen setups the last group of the panel migrates windows across screens, with layouts computed relative to the window's screen.

### Proxy

Download the mihomo core → add subscriptions → enable (TUN mode by default; the first enable asks for the admin password, after which it runs without re-prompting). Switch nodes, test latency, and pick rule modes; connections / rules / logs are available from the search-bar accessory; while connected, the menu bar offers a quick disconnect.

### Video Processing

Compress, convert, or extract audio; a static FFmpeg core is downloaded on demand when the system has none.

### Image Processing

Remove backgrounds (macOS Vision foreground segmentation — the same engine behind Photos' Lift Subject) and stitch images into long strips (horizontal / vertical, with spacing and overlap, unified sizing); copy, save, or reveal results in Finder.

### Terminal Autosuggestions

Once enabled, zsh shows frecency-ranked suggestions from your history as you type: `→` accepts, `Tab` cycles alternatives, `Ctrl+X` toggles, `Ctrl+C` clears.

### Keep Awake

A virtual external display keeps a MacBook awake with the lid closed and the display off. Available only on AC power.

### System Status

An overview of CPU / memory / disk / network.

### Clean Mode

Blanks the screen and locks keyboard and mouse. **Press and hold the left mouse button for 2 seconds** to exit.

### Homebrew

A package panel: browse installed formulae / casks and available upgrades, update with one click and automatic cleanup, start/stop services, view package details (dependencies / dependents), and uninstall.

### Settings

Appearance and UI language, launch shortcut, launch at login, check for updates, quit; privacy permission entries (Screen Recording, Accessibility, Full Disk Access) with one-click jumps into System Settings.

## License

[MIT](LICENSE)
