# Flutter Widgets TUI

[![Stars](https://img.shields.io/github/stars/gettoknowdavid/flutter-widgets-tui)](https://github.com/gettoknowdavid/flutter-widgets-tui)
[![License](https://img.shields.io/github/license/gettoknowdavid/flutter-widgets-tui)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Ratatui](https://img.shields.io/badge/Ratatui-000000?logo=terminal&logoColor=white)](https://ratatui.rs/)

**An offline-first terminal application (TUI) built in Rust for Flutter developers. Instantly browse the full Flutter
widget catalog, view code samples, and get AI-assisted guidance directly from your terminal.**

Instant access to every Flutter widget with rich documentation, fuzzy search, live code builder, favorites, and an
integrated AI assistant (with local Ollama support).

## Features

- ⚡ **Fully offline** widget catalog with properties, methods, and code samples
- 🔎 High-performance **fuzzy search**
- 🛠️ **Dynamic Code Parameter Builder** — interactively build Dart snippets
- ❤️ Favorites with personal notes and optional GitHub sync
- 💬 AI Chat (online and local Ollama fallback)
- 🎨 Multiple beautiful themes (Catppuccin, Gruvbox, Nord, Dracula, Monochrome)
- ⌨️ Excellent keyboard-first UX with navigation history
- 📋 Global yank hotkey (`y`) to copy code snippets

## Quick Start

```bash
# Install (once released)
cargo install flutter-widgets-tui