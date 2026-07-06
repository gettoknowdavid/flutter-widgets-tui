# Technical Requirements Document (TRD)

## Flutter Widgets TUI — A Beautiful, Offline-First Rust Terminal Application

**Document status:** Draft v1.0 — For Review
**Author:** Principal Rust Systems Architect (AI-assisted)
**Scope:** Architecture and planning only. No functional Rust code is included or implied by this document.

---

## 1. Project Overview

### 1.1 Purpose

Flutter Widgets TUI is a terminal application, built in Rust with Ratatui, that gives Flutter developers instant,
offline, richly-detailed access to the entire catalog of Flutter widgets — their descriptions, code samples, properties,
methods, and usage guidance — without needing a browser, an IDE plugin, or a network connection. An integrated AI chat
assistant (online-first, with a future local Ollama fallback) helps developers reason about *which* widget to use and
*how* to use it correctly, directly from the terminal.

The product philosophy is **"delightful offline-first developer tooling"**: the app must be instantly useful with zero
network access, and progressively enhanced (AI chat, cloud favorites sync) when connectivity and credentials are
available.

### 1.2 Guiding Principles

1. **Offline-first, not offline-only.** Every core feature (browsing, search, favorites, code builder, themes) must work
   with no network at all. Online features (AI chat, GitHub sync) are additive layers, never blocking dependencies.
2. **Instant and responsive.** A TUI lives or dies on input latency. Target sub-16ms frame render budget and sub-50ms
   perceived response to any keystroke, including fuzzy search across the full widget corpus.
3. **Delightful terminal aesthetics.** The application should feel hand-crafted — thoughtful spacing, consistent
   iconography (Unicode/Nerd Font-aware with ASCII fallback), smooth theme switching, and a coherent visual language
   borrowed from and improved upon the reference HTML wireframe (`flutter_widget_catalog_tui.html`).
4. **Clean architecture over cleverness.** Strict separation between domain logic, data access, and presentation (
   rendering/input). This is what makes a solo/small-team project maintainable over years, and what makes AI-assisted,
   ticket-by-ticket development (see Section 12 and the companion Epic/Ticket documents) tractable without regressions.
5. **Privacy-respecting by default.** No telemetry, no background network calls without explicit user opt-in, and clear
   boundaries around what data ever leaves the device.
6. **Disciplined incrementalism.** The system is designed so that each Epic/Ticket can be implemented, reviewed, and
   merged in isolation without destabilizing unrelated modules — enforced through the module boundaries described in
   Section 8.

### 1.3 Non-Functional Requirements (NFRs)

| ID     | Requirement                | Target / Acceptance Signal                                                                                                                                                                           |
|--------|----------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| NFR-1  | **Offline-first**          | 100% of MVP features except "AI chat" and "GitHub sync" function correctly with network disabled.                                                                                                    |
| NFR-2  | **Startup performance**    | Cold start to interactive catalog view in < 150ms on a warmed SQLite DB (SSD, typical dev laptop).                                                                                                   |
| NFR-3  | **Search latency**         | Fuzzy search across full widget corpus (~350–500 widgets incl. Material/Cupertino) returns first-paint results in < 30ms per keystroke.                                                              |
| NFR-4  | **Render performance**     | Sustains ≥ 60 "frames" per second equivalent redraw budget during scroll/navigation; no visible tearing or flicker on resize.                                                                        |
| NFR-5  | **Memory footprint**       | Steady-state RSS < 60MB with full catalog loaded and one theme active.                                                                                                                               |
| NFR-6  | **Terminal compatibility** | Correct rendering on: iTerm2, Alacritty, Kitty, Windows Terminal, GNOME Terminal, tmux/screen multiplexed sessions. Graceful degradation on terminals without true-color or Nerd Font glyph support. |
| NFR-7  | **Crash resilience**       | No panic should be able to corrupt the SQLite database or leave the terminal in a broken (raw-mode-stuck) state; a panic hook must always restore the terminal.                                      |
| NFR-8  | **Accessibility**          | All information conveyed by color must also be conveyed by text/symbol (colorblind-safe); keyboard-only operation is the *only* operation mode (no mouse dependency).                                |
| NFR-9  | **Data durability**        | Favorites/history/settings persist across crashes; writes are transactional.                                                                                                                         |
| NFR-10 | **Portability**            | Single static-ish binary (or minimal dynamic deps) buildable for macOS (arm64/x86_64), Linux (x86_64/arm64), and Windows (x86_64).                                                                   |
| NFR-11 | **Configurability**        | All themes, keybindings, and AI provider settings are user-configurable via a TOML config file, with sane defaults requiring zero configuration.                                                     |
| NFR-12 | **Testability**            | Core domain and data layers must be unit-testable without spawning a terminal; rendering must be snapshot-testable via Ratatui's `TestBackend`.                                                      |

---

## 2. System Architecture Overview

### 2.1 Architectural Style

The application follows a **layered, hexagonal-ish ("ports and adapters") architecture**, adapted for a TUI event-loop
context. This is intentionally conservative and boring — the goal is long-term maintainability by a solo developer or
small team working ticket-by-ticket with AI assistance, not architectural novelty.

```
┌─────────────────────────────────────────────────────────────┐
│                        Presentation Layer                    │
│   (Ratatui widgets, layout, input handling, themes, TUI      │
│    event loop — "the app shell")                             │
└───────────────────────────┬───────────────────────────────────┘
                             │ calls into (trait-bound ports)
┌───────────────────────────▼───────────────────────────────────┐
│                       Application Layer                       │
│   (Use-cases / services: SearchService, FavoritesService,     │
│    ChatService, CodeBuilderService, NavigationController)     │
└───────────────────────────┬───────────────────────────────────┘
                             │ depends on (trait interfaces = ports)
┌───────────────────────────▼───────────────────────────────────┐
│                          Domain Layer                          │
│   (Pure data types: Widget, Property, CodeSample, Favorite,   │
│    Theme, ChatMessage — zero I/O, zero framework dependency)   │
└───────────────────────────┬───────────────────────────────────┘
                             │ implemented by (adapters)
┌───────────────────────────▼───────────────────────────────────┐
│                     Infrastructure Layer                       │
│  SQLite repo impls · Fuzzy search index · HTTP AI client ·     │
│  Ollama client · GitHub OAuth client · Clipboard (copypasta) · │
│  Config/theme file I/O · Sync engine                           │
└─────────────────────────────────────────────────────────────┘
```

**Dependency rule:** Arrows point inward. Presentation depends on Application; Application depends on Domain (via trait
ports defined *in* the domain or an adjacent `ports` module); Infrastructure depends on Domain (implementing its traits)
but Domain never depends on Infrastructure or Presentation. This is what allows, e.g., swapping SQLite for an in-memory
store in tests, or swapping the AI HTTP client for a mock, without touching rendering code.

### 2.2 State Management

- **Single source of truth:** One root `AppState` struct owned by the event loop, containing sub-states for each
  screen/feature (`CatalogState`, `SearchState`, `DetailState`, `ChatState`, `FavoritesState`, `SettingsState`) plus
  cross-cutting state (`NavigationStack`, `ActiveTheme`, `ConnectivityStatus`).
- **Unidirectional data flow, Elm-inspired:** Input events and async task completions are converted into a closed
  `Message` enum. A single `update(state, message) -> Command` function mutates state and optionally emits `Command`s (
  side effects: DB queries, HTTP calls, clipboard writes) that are executed by a dedicated command executor, whose
  results re-enter the loop as new `Message`s.
    - This keeps rendering pure: `view(state) -> Frame` never performs I/O.
    - This is the same pattern used successfully in production Ratatui apps (e.g., inspired by `redux`/
      `Elm architecture`), and it directly supports NFR-7 (crash resilience) and NFR-12 (testability), since `update` is
      a pure, synchronously-testable function.
- **Async work** (AI chat calls, GitHub sync, DB writes that might block) is dispatched to a `tokio` task pool; results
  are sent back via an `mpsc` channel that the event loop polls alongside terminal input events using `tokio::select!`
  or `crossterm`'s async event stream.

### 2.3 Rendering Loop

1. Enter raw mode + alternate screen (via a **terminal guard** RAII type that *always* restores the terminal on drop,
   including on panic — registered via `std::panic::set_hook`).
2. Loop:
   a. Poll for: terminal input events (key/resize/paste), async channel messages (AI response chunks, DB query results,
   sync status), and a tick timer (for animations/spinners, e.g., "AI is thinking…").
   b. Convert raw events into domain `Message`s.
   c. Call `update(&mut state, message)`, collect any `Command`s.
   d. Dispatch `Command`s to the async executor (non-blocking).
   e. Call `view(&state)` to build the Ratatui `Frame` and render, **but only if state changed** (dirty-flag
   optimization to avoid needless re-render on inert ticks) — supports NFR-4.
3. On quit signal: flush any pending SQLite writes, persist session (breadcrumb history, last-open widget) if enabled,
   restore terminal.

### 2.4 Offline / Online Handling

- A lightweight `ConnectivityMonitor` (infrastructure layer) periodically (and lazily, on-demand before an AI/sync call)
  checks reachability of the configured AI endpoint / GitHub API, updating `ConnectivityStatus` in `AppState` (
  `Offline`, `Online`, `Degraded`).
- The **AI Chat** and **GitHub Sync** features are the *only* two features gated by this status. All widget browsing,
  search, favorites (local), code builder, and theming are always available regardless of `ConnectivityStatus`.
- UI must clearly and non-intrusively surface connectivity state (a small status glyph in the footer/status bar — see
  Section 8 theming) rather than blocking dialogs.
- AI Chat request path: attempt configured cloud provider → on failure/timeout, if Ollama fallback is configured and
  reachable (Future Feature), attempt local model → otherwise present a friendly inline error in the chat log with a
  retry affordance.

### 2.5 Error Handling Strategy

- `thiserror`-based domain-specific error enums per crate/module boundary (e.g., `RepositoryError`, `SearchError`,
  `AiClientError`).
- A top-level `anyhow`-based error boundary only at the outermost command-executor/event-loop level, where errors are
  converted into user-facing `Message::Error(String)` variants surfaced non-destructively in the status bar or a
  dismissible toast/log pane — never a hard crash for recoverable errors (network failures, malformed AI responses,
  etc.).
- Panics are reserved strictly for programmer errors (invariant violations); the panic hook guarantees terminal
  restoration before the process exits so the user's shell is never left broken.

---

## 3. Core Dependencies and Recommended Rust Crates

| Concern                                 | Crate                                                                                                      | Notes                                                                                                                                                                                                                |
|-----------------------------------------|------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Terminal backend                        | `ratatui`                                                                                                  | Core TUI rendering/widget framework.                                                                                                                                                                                 |
| Terminal I/O backend                    | `crossterm`                                                                                                | Cross-platform terminal manipulation; pairs natively with Ratatui.                                                                                                                                                   |
| Async runtime                           | `tokio` (multi-thread, limited worker count)                                                               | Powers AI HTTP calls, DB write offloading, sync, connectivity checks.                                                                                                                                                |
| SQLite driver                           | `rusqlite` (bundled feature) with `r2d2` or `deadpool-sqlite` for pooling                                  | `bundled` avoids system libsqlite version issues; pooling avoids lock contention between UI thread reads and background writes.                                                                                      |
| Migrations                              | `refinery` or `rusqlite_migration`                                                                         | Versioned, embedded SQL migrations shipped inside the binary.                                                                                                                                                        |
| Fuzzy search                            | `nucleo` (the matcher engine behind `helix`/modern fuzzy pickers) or `fuzzy-matcher` (skim's algorithm)    | `nucleo` recommended for best-in-class performance and match-highlighting; fallback `fuzzy-matcher` is simpler to integrate if `nucleo`'s API proves too low-level. Decision to be finalized in Epic 2 spike ticket. |
| Serialization                           | `serde`, `serde_json`, `toml`                                                                              | Config files, AI request/response payloads, theme definitions.                                                                                                                                                       |
| HTTP client                             | `reqwest` (rustls-tls feature, no native-tls to ease cross-compilation)                                    | AI provider calls, GitHub OAuth/API, Gist export (future).                                                                                                                                                           |
| Error handling                          | `thiserror` (library errors), `anyhow` (application boundary)                                              | Per Section 2.5.                                                                                                                                                                                                     |
| Clipboard                               | `copypasta`                                                                                                | Explicitly required by the feature spec for yank support.                                                                                                                                                            |
| Logging                                 | `tracing` + `tracing-subscriber` (file-appender, non-interfering with alt-screen)                          | Must write to a rotating log file, never stdout/stderr while in raw/alt-screen mode.                                                                                                                                 |
| CLI arg parsing                         | `clap` (derive)                                                                                            | For launch flags: `--theme`, `--db-path`, `--config`, `--no-ai`, `--reset`.                                                                                                                                          |
| Config directories                      | `directories` (or `etcetera`)                                                                              | Cross-platform XDG/AppData-correct config, data, and cache paths.                                                                                                                                                    |
| Date/time                               | `chrono` or `time`                                                                                         | History timestamps, "last updated" metadata, Flutter version tagging (future).                                                                                                                                       |
| Syntax highlighting (Dart code samples) | `syntect` (with a bundled minimal syntax set) or a lighter hand-rolled Dart tokenizer                      | `syntect` is heavier; a hand-rolled highlighter may better serve NFR-2/NFR-5 startup/memory budgets. Decision deferred to a spike ticket in Epic 3.                                                                  |
| Unique IDs                              | `uuid` (v4)                                                                                                | Favorite IDs, chat session IDs, sync conflict resolution.                                                                                                                                                            |
| Testing (snapshot)                      | `insta`                                                                                                    | Snapshot testing of Ratatui `TestBackend` buffers and domain outputs.                                                                                                                                                |
| Testing (assertions)                    | `pretty_assertions`, `rstest` (parameterized cases)                                                        | Cleaner diff output, table-driven tests.                                                                                                                                                                             |
| OAuth (GitHub sign-in)                  | `oauth2` crate + system browser launch (`open`/`webbrowser` crate) with a local loopback redirect listener | Standard PKCE device/loopback flow appropriate for a CLI tool (no client secret embedded).                                                                                                                           |

**Crate selection philosophy:** prefer crates that are (a) pure-Rust or have vendored/bundled native deps (to preserve
NFR-10 portability), (b) actively maintained, and (c) have minimal transitive dependency bloat, given the memory/startup
NFRs.

---

## 4. SQLite Data Models and Schema Design

### 4.1 Design Notes

- SQLite is the **single local source of truth**. The shipped widget catalog is seeded via embedded migration(s) at
  first run (or shipped as a pre-built read-only "catalog.db" merged logically with a user-writable "user.db" — see
  ADR-1 below) so that catalog updates can be distributed independently of user data.
- **ADR-1 (Decision to confirm in Epic 1):** Use **two SQLite files** — `catalog.db` (read-only, versioned,
  replaceable/updatable asset shipped with or downloaded by the app) and `user.db` (favorites, history, settings, chat
  sessions — never touched by catalog updates). This cleanly avoids migration conflicts between "
  Anthropic/maintainer-shipped content" and "user's personal data," and makes future catalog updates (Future Feature:
  version tagging) a simple file swap rather than a data migration.
- All tables use `INTEGER PRIMARY KEY` (SQLite rowid aliasing) for performance, plus a `TEXT` UUID column where
  cross-device sync identity is required (favorites).
- Foreign keys enforced (`PRAGMA foreign_keys = ON`).
- Full-text search accelerated via an SQLite `FTS5` virtual table as a *coarse* pre-filter feeding the in-memory fuzzy
  matcher (`nucleo`/`fuzzy-matcher`) for final ranking — see Section 6.

### 4.2 Schema — `catalog.db` (read-only content)

```sql
-- Widgets: the core catalog entity
CREATE TABLE widgets
(
    id                   INTEGER PRIMARY KEY,
    name                 TEXT NOT NULL UNIQUE,            -- e.g. "ListView"
    category             TEXT NOT NULL,                   -- e.g. "Scrolling"
    design_system        TEXT NOT NULL DEFAULT 'base',    -- 'material' | 'cupertino' | 'base'
    summary              TEXT NOT NULL,                   -- one-line description
    overview             TEXT NOT NULL,                   -- full overview/body markdown
    use_when             TEXT,                            -- guidance text
    avoid_when           TEXT,                            -- guidance text
    related_widget_id    INTEGER REFERENCES widgets (id), -- e.g. "use GridView instead"
    flutter_stable_since TEXT,                            -- future: version tagging
    flutter_channel      TEXT          DEFAULT 'stable',  -- future: 'stable'|'beta'|'legacy'
    created_at           TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_widgets_category ON widgets (category);
CREATE INDEX idx_widgets_design_system ON widgets (design_system);

-- Full text search index over widget name/category/summary
CREATE
VIRTUAL TABLE widgets_fts USING fts5(
    name, category, summary, content='widgets', content_rowid='id'
);

-- Code samples (a widget may have multiple, e.g. .builder vs default constructor)
CREATE TABLE code_samples
(
    id         INTEGER PRIMARY KEY,
    widget_id  INTEGER NOT NULL REFERENCES widgets (id) ON DELETE CASCADE,
    label      TEXT    NOT NULL, -- "Basic usage", "With separators"
    code       TEXT    NOT NULL, -- raw Dart source
    sort_order INTEGER NOT NULL DEFAULT 0
);

-- Properties table drives BOTH the "properties" detail pane AND the
-- Dynamic Code Parameter Builder feature.
CREATE TABLE properties
(
    id            INTEGER PRIMARY KEY,
    widget_id     INTEGER NOT NULL REFERENCES widgets (id) ON DELETE CASCADE,
    name          TEXT    NOT NULL,                -- "scrollDirection"
    type          TEXT    NOT NULL,                -- "Axis", "bool", "double?"
    default_value TEXT,                            -- "Axis.vertical"
    description   TEXT,
    is_required   INTEGER NOT NULL DEFAULT 0,      -- boolean
    input_kind    TEXT    NOT NULL DEFAULT 'text', -- 'enum'|'bool'|'text'|'number'
    enum_options  TEXT,                            -- JSON array, only if input_kind='enum'
    sort_order    INTEGER NOT NULL DEFAULT 0
);

-- Methods table (static + instance)
CREATE TABLE methods
(
    id          INTEGER PRIMARY KEY,
    widget_id   INTEGER NOT NULL REFERENCES widgets (id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL, -- 'static' | 'instance'
    signature   TEXT    NOT NULL, -- "ListView.builder(...)"
    description TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0
);

-- Catalog metadata / versioning (supports future catalog updates)
CREATE TABLE catalog_meta
(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
); -- e.g. ('schema_version', '1'), ('catalog_version', '2026.07.01'), ('flutter_sdk_version', '3.29.0')
```

### 4.3 Schema — `user.db` (writable, personal data)

```sql
CREATE TABLE favorites
(
    id          TEXT PRIMARY KEY,          -- UUID, stable across sync
    widget_id   INTEGER NOT NULL,          -- FK logically to catalog.db widgets.id (cross-DB, enforced in app layer, not SQL FK)
    widget_name TEXT    NOT NULL,          -- denormalized for resilience to catalog re-seeding
    note        TEXT,                      -- user's personal annotation ("edit" feature)
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    synced_at   TEXT,                      -- null until first successful cloud sync
    dirty       INTEGER NOT NULL DEFAULT 1 -- 1 = needs sync, 0 = in sync with remote
);

CREATE TABLE history
(
    id          INTEGER PRIMARY KEY,
    widget_id   INTEGER NOT NULL,
    widget_name TEXT    NOT NULL,
    visited_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_history_visited_at ON history (visited_at DESC);

CREATE TABLE settings
(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
); -- e.g. ('active_theme','catppuccin-mocha'), ('ai_provider','anthropic'), ('keybind_profile','default')

CREATE TABLE chat_sessions
(
    id         TEXT PRIMARY KEY, -- UUID
    title      TEXT,             -- derived from first user message
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE chat_messages
(
    id         INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES chat_sessions (id) ON DELETE CASCADE,
    role       TEXT NOT NULL, -- 'user' | 'assistant' | 'system'
    content    TEXT NOT NULL,
    widget_ref TEXT,          -- optional: widget name the message concerned, for linking
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE auth_tokens
(
    provider          TEXT PRIMARY KEY, -- 'github'
    access_token_enc  BLOB NOT NULL,    -- encrypted at rest, see Section 9
    refresh_token_enc BLOB,
    expires_at        TEXT
);

CREATE TABLE sync_meta
(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
); -- e.g. ('last_sync_at', ...), ('device_id', uuid)
```

### 4.4 Cross-Database Access Pattern

The Infrastructure layer exposes a `CatalogRepository` trait (backed by `catalog.db`) and a `UserDataRepository` trait (
backed by `user.db`) as separate ports. The Application layer composes them (e.g., `FavoritesService` needs both: to
validate a `widget_id` exists in the catalog, and to persist the favorite in user data). SQLite's `ATTACH DATABASE` is
deliberately **avoided** for the primary write path to keep failure domains isolated (a corrupt/missing catalog file
must never risk user data integrity); simple in-app joins across two connection pools are preferred.

---

## 5. AI Integration Strategy

### 5.1 Goals

- Provide two natural-language capabilities: **(a)** "which widget should I use for X" (recommendation, may reference
  multiple widgets), and **(b)** "how do I use widget Y for Z" (usage guidance grounded in the local catalog).
- Keep the feature strictly additive to the offline experience (Section 2.4).
- Ground responses in the local SQLite catalog where possible to reduce hallucination and keep answers
  Flutter-version-aware — i.e., this is a **retrieval-augmented** chat feature, not a raw pass-through chatbot.

### 5.2 Architecture

```
ChatService (application layer)
   │
   ├─ 1. Classify/parse user query (lightweight local heuristic; no ML)
   ├─ 2. Retrieve candidate widgets via SearchService (fuzzy + FTS)
   ├─ 3. Build a grounded prompt: system instructions + top-N candidate
   │      widget summaries/properties (from SQLite) + chat history + user query
   ├─ 4. Dispatch to AiClient port:
   │        AnthropicAiClient (reqwest, streaming SSE) — MVP default
   │        OllamaAiClient (local HTTP, streaming) — Future fallback
   ├─ 5. Stream tokens back into ChatState incrementally (Message::ChatChunk)
   └─ 6. Persist final message pair into chat_sessions/chat_messages (user.db)
```

- **Provider abstraction:** a single
  `trait AiClient { async fn stream_chat(&self, req: ChatRequest) -> impl Stream<Item = Result<ChatChunk, AiClientError>>; }`
  implemented per-provider. `ChatService` is provider-agnostic.
- **Grounding strategy:** rather than fine-tuning or embeddings (overkill for a bounded, well-structured catalog of a
  few hundred widgets), retrieval is done via the existing fuzzy/FTS search over widget
  name/category/summary/properties, injecting the top 3–6 matches' structured data into the system/context prompt. This
  keeps the MVP simple, fast, deterministic, and fully explainable, while leaving room to upgrade to a local embedding
  index later if catalog size or answer quality demands it.
- **Streaming UX:** chat responses render incrementally into the `chatLog` pane (mirroring the reference wireframe's
  chat tab) with a subtle "thinking" spinner state, never a blocking modal.
- **Failure handling:** timeouts, rate limits, and auth errors surface as a distinguishable inline system message in the
  chat log (not a crash, not a silent failure), with an explicit retry keybinding.

### 5.3 Local Ollama Fallback (Future Feature — designed for now, built later)

- `OllamaAiClient` implements the same `AiClient` trait against `http://localhost:11434` by default (configurable).
- Fallback order is configurable: `AiProviderPolicy::{CloudOnly, LocalOnly, CloudThenLocal, LocalThenCloud}`, default
  `CloudThenLocal` when both are configured, `CloudOnly` otherwise.
- Model selection for Ollama is user-configured (e.g., `llama3.1`, `qwen2.5-coder`); the app makes no assumption about
  which local models are installed and must fail gracefully with a clear "model not found, run `ollama pull <model>`"
  style message.

### 5.4 Privacy Considerations for AI

- Only the minimum necessary context (retrieved widget snippets + user's own message + recent chat history for the
  session) is sent to the cloud provider — never the user's favorites list, history, or any file system data outside
  this app's scope.
- API keys are read from config/environment/OS keychain (Section 9) and never logged (`tracing` filters must redact any
  field named `*token*`, `*key*`, `*secret*`).

---

## 6. Search Architecture (Supplementary Detail)

Because fuzzy search quality is an explicit MVP requirement, it merits its own brief section:

1. **Coarse filter:** SQLite `FTS5` MATCH query narrows the corpus (useful once catalog grows to include deep
   property/method text, keeping the in-memory matcher's working set small).
2. **Fine ranking:** The candidate set is fed into the chosen in-memory fuzzy matcher (`nucleo` preferred) against the
   raw query string for subsequence/typo-tolerant scoring and match-position highlighting (so the UI can bold matched
   characters, à la fzf/telescope.nvim).
3. **Index residency:** The full widget corpus (name, category, summary — small in aggregate, well under NFR-5's memory
   budget) is loaded into the in-memory matcher's index once at startup (async, off the render thread) rather than
   re-querying SQLite per keystroke, to satisfy NFR-3.
4. **Multi-field weighting:** name matches rank above category matches, which rank above summary/description matches,
   with configurable weights.
5. **Natural-language queries** (e.g., `"scrollable list"` from the wireframe's search placeholder) are handled by the
   same fuzzy layer against summaries/use-case text; more sophisticated intent parsing is explicitly deferred to the
   `ChatService`/AI path rather than over-engineering the deterministic search feature.

---

## 7. Proposed Directory / Module Structure

```
flutter-widgets-tui/
├── Cargo.toml                     # workspace root
├── crates/
│   ├── fwt-domain/                # pure domain types & port traits, zero I/O
│   │   └── src/
│   │       ├── widget.rs
│   │       ├── favorite.rs
│   │       ├── chat.rs
│   │       ├── theme.rs
│   │       ├── history.rs
│   │       └── ports/              # trait definitions: Repository, AiClient, Clipboard, etc.
│   │
│   ├── fwt-app/                   # application/use-case layer
│   │   └── src/
│   │       ├── search_service.rs
│   │       ├── favorites_service.rs
│   │       ├── chat_service.rs
│   │       ├── code_builder_service.rs
│   │       ├── navigation.rs        # breadcrumb / history stack controller
│   │       ├── sync_service.rs      # GitHub favorites sync orchestration
│   │       └── state.rs             # AppState, Message, update()
│   │
│   ├── fwt-infra/                 # adapters implementing fwt-domain ports
│   │   └── src/
│   │       ├── db/
│   │       │   ├── catalog_repo.rs
│   │       │   ├── user_repo.rs
│   │       │   └── migrations/
│   │       ├── search/
│   │       │   └── fuzzy_index.rs
│   │       ├── ai/
│   │       │   ├── anthropic_client.rs
│   │       │   └── ollama_client.rs
│   │       ├── auth/
│   │       │   └── github_oauth.rs
│   │       ├── clipboard/
│   │       │   └── copypasta_adapter.rs
│   │       └── config/
│   │           ├── settings.rs
│   │           └── secrets.rs        # OS keychain / encrypted-at-rest handling
│   │
│   ├── fwt-tui/                   # presentation layer: Ratatui + crossterm
│   │   └── src/
│   │       ├── app.rs               # event loop, terminal guard, panic hook
│   │       ├── theme/
│   │       │   ├── mod.rs
│   │       │   ├── catppuccin.rs
│   │       │   ├── gruvbox.rs
│   │       │   ├── nord.rs
│   │       │   ├── dracula.rs
│   │       │   └── monochrome.rs
│   │       ├── views/
│   │       │   ├── catalog.rs
│   │       │   ├── search.rs
│   │       │   ├── detail.rs         # overview/code/properties/methods sub-tabs
│   │       │   ├── code_builder.rs
│   │       │   ├── chat.rs
│   │       │   ├── favorites.rs
│   │       │   └── settings.rs
│   │       ├── widgets/              # reusable Ratatui components (status bar, breadcrumb trail, tab bar)
│   │       └── keymap.rs
│   │
│   └── fwt-cli/                   # binary entrypoint
│       └── src/main.rs             # clap parsing → wires infra impls into app → hands to fwt-tui
│
├── assets/
│   └── catalog_seed/               # source-of-truth SQL/JSON used to build catalog.db at build/release time
│
├── migrations/
│   ├── catalog/                    # versioned migrations for catalog.db
│   └── user/                       # versioned migrations for user.db
│
├── docs/
│   ├── TRD.md
│   ├── epics/
│   │   ├── epic-01-foundation.md
│   │   ├── epic-02-catalog-and-search.md
│   │   ├── epic-03-detail-and-code-builder.md
│   │   ├── epic-04-favorites-and-sync.md
│   │   ├── epic-05-ai-chat.md
│   │   └── epic-06-theming-and-polish.md
│   └── adr/                        # Architecture Decision Records
│
└── tests/
    ├── integration/                 # cross-crate integration tests
    └── snapshots/                   # insta snapshot baselines for TUI views
```

**Rationale for a Cargo workspace with 5 crates:** enforces the dependency rule from Section 2.1 *at compile time* —
`fwt-domain` cannot physically depend on `ratatui` or `rusqlite`, because those crates simply aren't in its
`Cargo.toml`. This is a stronger guarantee than convention alone and is especially valuable when tickets are implemented
in isolated AI-assisted sessions (Section 12), since a misplaced dependency causes an immediate compile error rather
than a silent architectural violation.

---

## 8. Theming System Design

### 8.1 Inspiration and Departure from the Reference Wireframe

The provided `flutter_widget_catalog_tui.html` wireframe establishes the correct *information architecture* and
interaction model that this TRD adopts almost directly:

- A persistent top tab bar (`[1] Catalog`, `[2] Search`, `[3] Favorites`, `[4] AI chat`) mapped to number-key hotkeys.
- A search affordance surfaced prominently above the catalog grid, accepting both literal names and natural-language
  phrases.
- A categorized catalog grid (design systems — Cupertino/Material — as a distinct, visually promoted group above the
  general base-widget categories).
- A detail view with breadcrumb (`layout > scrolling >`), a sub-tab bar (`overview | code | properties | methods`), and
  an `esc back` affordance.
- A bottom-anchored, always-visible keybinding legend and an AI chat input bar with a send affordance.

**Where the TUI must improve on the HTML mock**, given the actual product needs:

1. **Breadcrumbs must be real, not decorative.** The HTML hardcodes `layout > scrolling >`; the TUI's
   `NavigationController` (Section 2.2, 7) must maintain a genuine navigation stack driving this breadcrumb, supporting
   Backspace/Ctrl+O history traversal per the MVP feature list — not just a static label.
2. **A 5th tab/mode for Settings/Themes** is needed (not present in the mock) to support live theme switching and
   keybinding configuration.
3. **A persistent connectivity/status indicator** (Section 2.4) in the header or footer — absent from the static mock —
   to non-intrusively convey AI/sync availability.
4. **The Dynamic Code Parameter Builder** needs a dedicated interactive pane (an evolution of the mock's static `code`
   sub-tab) where toggling a property (via the `properties` table) live-updates the rendered Dart snippet — the mock's
   code pane is currently read-only.
5. **Yank/clipboard affordance** and a brief confirmation toast (e.g., `"✓ copied ListView.builder(...) to clipboard"`)
   should appear in the status bar, mirroring the mock's clean, unobtrusive bottom info bar style.
6. **Favorites need inline edit affordances** (note-taking, per MVP) — the mock does not depict a favorites screen; it
   will follow the same list+detail pattern as the catalog for consistency.

### 8.2 Theme Architecture

- A `Theme` domain struct is a pure data value: a structured palette (background, surface, primary/accent, success,
  warning, danger, muted-text, border, and semantic roles like `focus_ring`, `selection_bg`) plus typography-adjacent
  choices that make sense in a terminal (bold/italic usage conventions, border-style:
  `Plain | Rounded | Thick | Double`, matching the mock's rounded-corner, boxed-panel aesthetic where the terminal
  supports it).
- Shipped themes: **Catppuccin** (Mocha/Latte variants), **Gruvbox** (dark/light), **Nord**, **Dracula**, **Monochrome
  ** (a true fallback for terminals without 256-color/true-color support, using only default 16 ANSI colors and text
  attributes for differentiation — critical for NFR-6/NFR-8).
- Each theme is defined as a `.toml` (or embedded `const`) mapping semantic roles to colors, never widget code
  hardcoding raw colors — all `fwt-tui/src/views/*` code references `theme.accent`, `theme.border`, etc., never literal
  `Color::Rgb(...)`.
- **Live switching:** selecting a theme in Settings updates `AppState.active_theme` and triggers a full re-render; no
  restart required. The chosen theme persists to `user.db` settings.
- **Terminal capability detection:** at startup, `crossterm`'s color-support detection (or an env-var heuristic,
  `COLORTERM`/`TERM`) determines whether true-color, 256-color, or basic-16 rendering is used, automatically
  down-mapping any theme's palette rather than forcing the user to manually pick Monochrome — Monochrome remains
  available as an explicit opinionated choice, not merely a fallback.
- **Iconography:** Nerd Font glyphs (mirroring the mock's `ti ti-*` icon usage) are used when a Nerd Font is
  detected/configured, with a plain-ASCII/Unicode-box-drawing fallback set (e.g., `→` instead of an arrow glyph, `[?]`
  instead of a search icon) so the app never renders tofu boxes.

### 8.3 Layout System

- A consistent **shell** (top tab bar → contextual sub-header/breadcrumb → main content pane → bottom status/keybinding
  legend) is shared across all views, implemented as a single reusable `AppShell` Ratatui composite widget that each
  view renders *into*, guaranteeing visual consistency (border style, padding, tab bar look) without duplication — a
  direct structural lift from the mock's consistent outer `#term` chrome.

---

## 9. Security, Privacy, and Configuration Considerations

### 9.1 Secrets Management

- **AI API keys** (Anthropic, etc.) and **GitHub OAuth tokens**: never stored in plaintext config files. Preferred
  storage: OS-native credential store via a crate such as `keyring`, with an explicit, clearly-labeled fallback to an
  encrypted-at-rest local file (`age`/`chacha20poly1305`-based envelope, key derived from an OS keychain-stored master
  key or, as a last resort, a user-supplied passphrase) for headless/Linux-without-keyring environments. This fallback
  path must be opt-in with a visible warning, not a silent default.
- The `auth_tokens` table (Section 4.3) stores only the encrypted blob; the encryption key itself is never persisted in
  SQLite.

### 9.2 GitHub Sign-In & Sync

- Standard OAuth 2.0 **Authorization Code + PKCE** flow (no embedded client secret, appropriate for a public/native CLI
  client): app opens the system browser to GitHub's authorize URL, spins up a short-lived local loopback HTTP listener
  to capture the redirect/code, exchanges it for tokens via `reqwest`.
- Sync scope is minimal — request the narrowest GitHub OAuth scope sufficient for the sync mechanism (e.g., a private
  Gist or a dedicated repo used purely as a favorites-sync backend; **decision deferred to Epic 4** — a Gist-based store
  is simplest and aligns with the Future Feature "GitHub Gist export," suggesting the sync engine and Gist export should
  share one underlying mechanism).
- Conflict resolution: last-write-wins at the individual favorite level, using the `updated_at` timestamp, with the
  `dirty` flag (Section 4.3) tracking local changes pending push. A future enhancement could add three-way merge, but
  MVP explicitly scopes to last-write-wins for simplicity, documented clearly to the user.

### 9.3 Network Egress Transparency

- The app must document (in a `--help` section and settings screen) exactly which hosts it may contact: the configured
  AI provider endpoint, `api.github.com`/`github.com` (sign-in/sync), and optionally `localhost:11434` (Ollama). No
  other outbound calls, no telemetry/analytics, ever.
- All HTTP calls use TLS (`reqwest` rustls backend) with no custom certificate trust overrides.

### 9.4 Local Data Privacy

- All personal data (favorites, notes, chat history) lives in `user.db` under the OS-appropriate local data directory (
  via `directories`/`etcetera`), never bundled into logs.
- A "reset local data" command/flag is provided (clears `user.db` after confirmation) to respect user control over their
  own data.

### 9.5 Configuration

- Primary config file: `config.toml` in the OS config directory, covering: active theme, keybinding profile, AI
  provider + model + endpoint override, Ollama fallback settings, sync toggle, log level/verbosity.
- CLI flags (via `clap`) can override config file values per-invocation (`--theme dracula`, `--no-ai`, `--db-path`) for
  scripting/testing convenience, never persisting the override unless explicitly saved.
- Config schema is versioned; unknown/deprecated keys are logged as warnings, not fatal errors, to tolerate config file
  evolution across app versions.

---

## 10. Testing Strategy

| Layer                            | Approach                                                                                                                                                                                                                                                                                                                                                         | Tooling                                                                                                                                                            |
|----------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Domain (`fwt-domain`)**        | Pure unit tests on data types/invariants (e.g., a `Favorite` cannot be constructed with an empty widget name). No mocking needed — no I/O exists here.                                                                                                                                                                                                           | `cargo test`, `rstest` for table-driven cases.                                                                                                                     |
| **Application (`fwt-app`)**      | Unit tests against **mock/fake implementations** of the port traits (in-memory `FakeCatalogRepository`, `FakeAiClient` that yields scripted streamed chunks) to test `update()` transitions and service orchestration logic deterministically, without a real DB or network.                                                                                     | `cargo test`, hand-written fakes (preferred over heavy mocking frameworks for trait objects this simple), `insta` for snapshotting complex `AppState` transitions. |
| **Infrastructure (`fwt-infra`)** | Integration tests against a **real temporary SQLite file** (via `tempfile`) applying real migrations, verifying repository implementations against the actual schema. AI client tests run against a local mock HTTP server (`wiremock` or `mockito`) rather than live network calls, to keep CI deterministic and offline-safe.                                  | `cargo test --features integration`, `tempfile`, `wiremock`.                                                                                                       |
| **Presentation (`fwt-tui`)**     | Snapshot tests rendering views into Ratatui's `TestBackend` at fixed terminal sizes, asserting buffer contents/styles via `insta` snapshots — catches unintended visual regressions per-ticket. Keybinding/input-handling logic tested by feeding synthetic `crossterm::Event` sequences into the event loop and asserting resulting `AppState`/rendered output. | `ratatui::backend::TestBackend`, `insta`.                                                                                                                          |
| **End-to-end**                   | A small number of scripted "golden path" scenarios (launch → search → open detail → favorite → yank → quit) run against a seeded test catalog DB, asserting on final `user.db` state and final rendered frame.                                                                                                                                                   | Custom harness composing the above tools; run in CI on Linux/macOS/Windows runners per NFR-10.                                                                     |
| **Performance**                  | Micro-benchmarks for search latency (NFR-3) and startup time (NFR-2) using `criterion`, run on a representative seeded catalog size, tracked over time to catch regressions.                                                                                                                                                                                     | `criterion`.                                                                                                                                                       |
| **Manual/exploratory**           | A checklist (in `docs/`) for manual verification across the terminal emulator matrix (NFR-6) and true-color/256-color/basic-16/no-Nerd-Font permutations (Section 8.2), performed at minimum before each tagged release.                                                                                                                                         | Manual QA checklist doc.                                                                                                                                           |

**Coverage philosophy:** the domain and application layers should approach very high coverage (they contain the business
logic and are cheap/fast to test exhaustively); infrastructure integration tests focus on the "does this adapter honor
its trait's contract" question; presentation snapshot tests focus on "did this ticket's UI change what I expected it to,
and nothing else."

---

## 11. Risks and Mitigation

| Risk                                                                                                                                                                           | Impact                                    | Likelihood | Mitigation                                                                                                                                                                                                                                                                                                                                                                      |
|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------|------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Catalog data accuracy/completeness** (hand-curating 350–500+ widgets with correct properties/methods/code samples is a large content effort, not just an engineering effort) | High — core value prop depends on it      | High       | Treat catalog content as a first-class deliverable with its own ticket/epic-adjacent workstream (`assets/catalog_seed/`), versioned independently (`catalog.db`, Section 4.2); consider community contribution tooling as a post-MVP stretch; start with a well-scoped subset (e.g., top 100 most-used widgets) for the true MVP release rather than blocking on full coverage. |
| **Fuzzy search quality perception** (subjective "good fuzzy search" bar is high, per explicit ask)                                                                             | Medium-High                               | Medium     | Time-box a Epic 2 spike ticket comparing `nucleo` vs `fuzzy-matcher` against representative real queries before committing; build a small internal eval set of query→expected-top-result pairs as a regression test.                                                                                                                                                            |
| **Terminal compatibility fragmentation** (NFR-6 matrix is wide)                                                                                                                | Medium                                    | Medium     | Rely on `crossterm`'s abstraction and capability detection rather than hand-rolled ANSI; maintain the Monochrome/no-Nerd-Font fallback as a first-class supported mode, not an afterthought; manual QA checklist (Section 10) before releases.                                                                                                                                  |
| **AI provider cost/rate-limits/API changes**                                                                                                                                   | Medium                                    | Medium     | Provider abstraction (`AiClient` trait, Section 5.2) isolates blast radius of provider API changes to one adapter; explicit, user-visible error states rather than silent retries that could cause surprise cost; document expected token/cost behavior for users bringing their own API key.                                                                                   |
| **OAuth/sync complexity and edge cases** (token expiry, revoked access, multi-device conflicts)                                                                                | Medium                                    | Medium     | Scope MVP sync to explicit last-write-wins (Section 9.2) and make sync a fully optional, clearly-labeled feature; comprehensive integration tests around token refresh and conflict scenarios in Epic 4.                                                                                                                                                                        |
| **Scope creep from "Future Features" bleeding into MVP**                                                                                                                       | Medium                                    | Medium     | Strict epic/ticket boundaries (see companion Epic docs); architecture in Section 2/7 deliberately leaves seams (e.g., `AiProviderPolicy`, `flutter_channel` column) so future features slot in without rework, without being *built* now.                                                                                                                                       |
| **Solo/small-team + AI-assisted ticket-by-ticket development drifting from architecture over time**                                                                            | Medium-High                               | Medium     | Workspace-enforced dependency rule (Section 7) makes architectural violations a compile error; mandatory fresh-session code review step per ticket (Section 12) specifically checks architectural conformance, not just correctness; ADRs (`docs/adr/`) capture and preserve key decisions across sessions that lack persistent memory.                                         |
| **SQLite write contention between UI thread reads and background async writes** (history logging, chat persistence, sync)                                                      | Low-Medium                                | Medium     | Connection pooling (`r2d2`/`deadpool-sqlite`) with WAL mode (`PRAGMA journal_mode=WAL`) enabled by default, permitting concurrent readers alongside a writer.                                                                                                                                                                                                                   |
| **Panic leaving terminal in raw mode / alternate screen ("broken shell")**                                                                                                     | Low likelihood but high user-trust impact | Low        | Mandatory RAII terminal guard + `panic::set_hook` (Section 2.3/2.5) is a Day-1, Epic-1 requirement, tested explicitly with an intentional-panic integration test.                                                                                                                                                                                                               |
| **Cross-platform packaging/distribution complexity** (macOS notarization, Windows binary trust, Linux distro variance)                                                         | Medium                                    | Medium     | Favor static/bundled dependencies (`rusqlite` bundled feature, rustls over native-tls) to minimize per-platform build variance; defer formal packaging/signing polish to a later stage explicitly, keep it out of MVP ticket scope unless required for basic runnability.                                                                                                       |

---

## 12. Master Development Workflow (Preview)

*(Full detail restated at the end of the companion Epics/Tickets deliverable; summarized here as it directly informs the
architectural choices above, especially Section 7's workspace boundaries and Section 11's drift mitigation.)*

Each ticket is implemented in an isolated, context-bounded session following a strict discipline: fresh session → load
only the relevant Epic + Ticket context → implement → stage changes (no auto-commit) → **new, separate session** for
critical code review against this TRD's architectural rules → fix flagged issues → validate (tests green) → commit with
a clear, conventional message → mark ticket complete → fully clear context before the next ticket. This workflow is
*why* Sections 2, 4, and 7 of this TRD are written as prescriptively as they are: they must function as the durable,
external "memory" that keeps every isolated session aligned to one coherent architecture.

---

## 13. Open Decisions Requiring Your Input Before Epics Are Finalized

These are flagged rather than silently decided, per the disciplined-planning mandate of this session:

1. **Fuzzy matcher library:** `nucleo` vs `fuzzy-matcher` — recommend a short spike ticket in Epic 2 rather than
   deciding now.
2. **Two-database (`catalog.db` + `user.db`) split (ADR-1):** recommended above; confirm before Epic 1 tickets are
   written, as it materially shapes the repository trait design.
3. **GitHub sync backend mechanism:** private Gist vs dedicated repo vs a lightweight custom backend — recommend
   Gist-based for MVP simplicity and synergy with the Future "Gist export" feature; confirm.
4. **Dart syntax highlighting approach:** `syntect` vs hand-rolled tokenizer — trade-off between fidelity and
   startup/memory budget; recommend a spike ticket in Epic 3.
5. **MVP catalog content scope:** full Flutter widget catalog vs a curated top-N subset for initial release —
   recommended to start narrower (Risk table, Section 11) but this is a product decision, not purely technical.

---

**End of TRD.md — Step 1 complete.**
