# MisterT (Ms. T) - Development Notes

## Build & Run

- Always build with `cargo build` from `src-tauri/`, NOT npm or `cargo tauri dev`.
- Always kill the running process before rebuilding: `taskkill //F //IM mst.exe` then `cargo build`.
- Frontend changes (JS/CSS/HTML in `src/`) don't require a rebuild — just restart the app.

## Project Structure

- **Tauri v2** desktop app (Rust backend + HTML/JS frontend)
- `src/` — Frontend (HTML, CSS, JS)
- `src-tauri/` — Rust backend (Tauri commands, platform code, config)
- `src-tauri/src/platform/` — Platform-specific code (windows.rs, macos.rs, linux.rs)
