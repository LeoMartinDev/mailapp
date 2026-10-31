# mailapp

> Working name. A native, local-first, keyboard-driven email client written in Rust with GPUI, built to work hand in hand with AI agents — both the ones it calls and the ones that call it.

Inspired by [bytemail](https://bytemailapp.com/) (Swift/SwiftUI, macOS only), but open source, cross-platform (macOS + Windows) and built on Zed's GPUI.

---

## 1. Vision

- **Fast and native.** Every interaction is served from a local copy of the mailbox. The network is never in the critical path of the UI.
- **Yours.** No mailapp server, no account, no telemetry. Mail goes straight from your provider to your machine, encrypted at rest.
- **Agent-ready, human in the loop.** Agents can summarize, draft and organize; external agents (Claude Code, opencode, …) can plug into the mailbox through MCP. Nothing is sent or destroyed without you.
- **Built to be extended.** Each provider, agent backend or platform integration is an adapter. Custom behavior for one provider never leaks into the core.

The MVP goal is simple: **replace the author's daily mail client.** v1 is what makes it better than the alternatives.

## 2. Principles

1. **Local-first.** The local database is the UI's source of truth; changes are applied optimistically and replayed to the server.
2. **Keyboard-first.** Every action is reachable from a shortcut or the command palette.
3. **Privacy by default.** Remote content blocked, trackers stripped, agents off until enabled.
4. **Adapters, not conditionals.** No `if provider == gmail` in the core — providers expose capabilities.
5. **One pipeline for every change.** UI, agents and MCP clients all mutate state through the same commands, guardrails and journal.
6. **Readable over clever.** Plain layers, explicit dependencies enforced by the compiler.

## 3. Scope

### Platforms

| Platform | Status |
|---|---|
| macOS | Target (primary) |
| Windows | Target |
| iOS | Not now. Architecture kept mobile-ready (see §8.6) |
| Linux, Android | Out of scope |

### Distribution

Built from source (`cargo build`) for now. No signed or notarized binaries until the project has users (signing costs money; unsigned macOS builds trigger Gatekeeper and Keychain prompts after each rebuild).

### License

**GPL-3.0.** Compatible with GPUI (Apache-2.0) and lets us reuse Zed's GPL crates (`ui`, `theme`).

## 4. Features

Legend: **MVP** = needed to replace the daily client · **v1** = makes it better · **Later** = planned, architecture must allow it.

### 4.1 Accounts & providers

| Feature | Milestone |
|---|---|
| Multiple accounts | MVP |
| Generic IMAP/SMTP (password / app password) | MVP |
| Microsoft 365 / Outlook.com via OAuth2 (XOAUTH2 over IMAP/SMTP), app registered by the project | MVP |
| Gmail via IMAP + app password, using Gmail extensions (`X-GM-MSGID`, `X-GM-THRID`, `X-GM-LABELS`): real labels, real threads, each message stored once | MVP |
| Identities / aliases per account | MVP |
| JMAP (Fastmail, Stalwart…) | v1 |
| Microsoft Graph backend (native API instead of IMAP) | v1 |
| Gmail API + Google OAuth (requires paid Google CASA verification) | Later |
| iCloud-specific tweaks | Later |

### 4.2 Mailbox

| Feature | Milestone |
|---|---|
| Unified inbox across accounts (per-account color) | MVP |
| Folders and labels (folders modelled as exclusive labels) | MVP |
| Read/unread, star, archive, trash, move | MVP |
| Conversation (thread) view — modern, minimal | MVP |
| Push: IMAP IDLE on Inbox, periodic polling of other mailboxes | MVP |
| Snooze (natural language: "tomorrow 9am"), see §8.5 | MVP |
| Smart tabs (Primary / Updates / Promotions, heuristic-based) | v1 |
| "To Reply" view | v1 |
| Rules (classic conditions → actions) | v1 |
| Full offline mode (compose and queue everything offline) | v1 |
| Pinned folders, separate message windows | Later |
| Optional flat message-list view | Later |

### 4.3 Reading

| Feature | Milestone |
|---|---|
| HTML rendering with Blitz (pure Rust, drawn in GPUI) | MVP |
| HTML sanitization + legacy attribute preprocessing (see §8.4) | MVP |
| Remote images blocked by default, allow per message / per sender | MVP |
| Tracking pixels removed, tracked links unwrapped | MVP |
| Plain-text view and "Open in browser" fallback | MVP |
| Attachments: preview, open, save | MVP |
| One-click unsubscribe (`List-Unsubscribe-Post`) | v1 |
| Calendar invite cards (accept / decline) | v1 |
| Phishing heuristics | Later |

### 4.4 Composing

| Feature | Milestone |
|---|---|
| **Block-based WYSIWYG composer** (Notion-style: paragraphs, lists, quotes, inline styles, links) producing `text/html` + `text/plain` | MVP |
| Reply / reply all / forward with quoting | MVP |
| Signatures per account and per alias | MVP |
| Drafts synced to the server | MVP |
| Attachments (drag & drop, picker) | MVP |
| Recipient autocomplete (see §4.6) | MVP |
| Undo send (5–30 s) | v1 |
| Send later (see §8.5) | v1 |
| Templates | v1 |
| Always-Bcc | v1 |
| Follow-up reminder when nobody replies | Later |

### 4.5 Search

| Feature | Milestone |
|---|---|
| Local full-text search across all accounts (SQLite FTS5) | MVP |
| Operators: `from:` `to:` `subject:` `has:attachment` `in:` `is:unread` `before:` `after:` | MVP |
| Server-side search for history not synced locally | MVP |
| Saved searches | Later |

### 4.6 Contacts

| Feature | Milestone |
|---|---|
| Autocomplete from local history (ranked by frequency and recency) | MVP |
| Address books: macOS Contacts, CardDAV, Microsoft (Graph contacts) | MVP |
| Contact management screen | Later |

Known gap: Google contacts need Google OAuth, which is out of scope for now — Gmail users rely on history.

### 4.7 Keyboard & UI

| Feature | Milestone |
|---|---|
| Keyboard navigation (`j`/`k`, `e`, `r`, …) | MVP |
| Command palette (⌘K / Ctrl+K) | MVP |
| Shortcuts configurable from the settings screen | MVP |
| Settings screen (all settings, stored in the encrypted database — no hand-edited files) | MVP |
| UI in English and French, user-selectable | MVP |
| Custom light and dark themes | MVP |

### 4.8 Agents

| Feature | Milestone |
|---|---|
| Providers via API: Anthropic, OpenRouter (OpenAI-compatible adapter) | MVP |
| Providers via installed CLI: `claude`, `opencode` (auto-detected in `PATH`) | MVP |
| Summarize a thread | MVP |
| Draft a reply from a one-line intent, rewrite, translate | MVP |
| **MCP server** exposing the mailbox to external agents (see §7) | MVP |
| In-app assistant that answers questions and performs (undoable) actions | v1 |
| Local models (Ollama, LM Studio) — free via the OpenAI-compatible adapter | v1 |
| Automatic triage / prioritization / categorization on incoming mail (opt-in per account) | Later |
| Extraction of tasks and dates | Later |
| Autonomy: allow-listed actions without approval, per account | Later |
| MCP **client**: assistant connects to external MCP servers (Linear, Notion…) | Later |

### 4.9 System integration

| Feature | Milestone |
|---|---|
| Native notifications | MVP |
| Secrets in macOS Keychain / Windows Credential Manager | MVP |
| Dock / taskbar unread badge | v1 |
| Widgets, menu bar extra, settings sync between machines | Later |

## 5. Non-goals

- No mobile (for now), no Linux.
- No POP3, no Exchange EWS.
- **No mailapp server**: no relay, no account, no cloud component.
- No telemetry; no crash report leaves the machine without an explicit user action.
- No AMP for Email (the `text/html` part is always rendered).
- No PGP / S/MIME in the MVP (to be decided later — it impacts composer and rendering).

## 6. Key decisions

| Area | Decision | Why |
|---|---|---|
| UI framework | GPUI as a **git dependency on the Zed repo**, pinned to a commit | The official crates.io release is stale (0.2.2); the Zed repo is the real upstream |
| Components | Zed's `ui` + `theme` crates (same commit) for **mechanics only**: context menus, popovers, tooltips, scrollbars, modals, keybinding hints. Everything carrying the visual identity (buttons, list rows, sidebar, reading pane, composer) is custom | `ui` lets us control colors, fonts, density and icons, but component shapes are hard-coded and IDE-like. A component that gets in the way is copied into our crate and modified (GPL allows it) |
| Text input | Custom, informed by gpui-component's Input (Apache-2.0) | Zed's `editor` drags in workspace, LSP, git, telemetry… |
| HTML rendering | **Blitz** (Stylo + Taffy, drawn into GPUI) | No JavaScript is ever needed in email. Blitz has no native-view z-order or clipping issues (unlike a webview), is pure Rust and works on every GPUI platform. Its gaps are mitigated (§8.4) |
| Composer | Block-based WYSIWYG from the MVP | A block model is much simpler to build and test than free-form rich text, and produces clean HTML |
| Providers | Base trait + optional capabilities | Lets each provider be tailored without core conditionals |
| Gmail | IMAP + app password + X-GM extensions; no Google OAuth for now | Google's restricted-scope verification is paid and yearly |
| Storage | SQLite + SQLCipher + FTS5; attachments and body cache encrypted on disk | Local-first and encrypted at rest; FTS5 lives inside the encrypted DB (a Tantivy index would not be encrypted) |
| Sync depth | Per account; default 12 months full, headers beyond, attachments on demand | A large Gmail mailbox is tens of GB |
| Architecture | Hexagonal + light CQRS + in-memory domain events | See §8 |
| Agents | Single tool registry shared by UI, CLI agents and MCP | One implementation, one set of guardrails |
| MCP transport | Streamable HTTP on `127.0.0.1` with a bearer token, inside the app process | Supported by Claude Code and opencode; no extra binary |

## 7. Agents & MCP

### 7.1 Agent providers

`AgentProvider` is a port with these adapters:

- `anthropic` — Messages API, streaming.
- `openai-compat` — any OpenAI-compatible endpoint: OpenRouter now; Ollama, LM Studio, vLLM later at no cost.
- `cli` — spawns an installed agent CLI as a subprocess, reusing its existing authentication and subscription:
  - `claude -p … --output-format stream-json --mcp-config <mailapp MCP>`
  - `opencode run …` with mailapp's MCP server configured (exact flags to verify during implementation).

API keys live in the OS secret store. CLI agents are given mailapp's own MCP server, so they get mailbox access with no extra work.

### 7.2 Tool registry

Agent-facing tools are **thin wrappers over application commands and queries** (§8.2). The same registry is:

- exposed by the MCP server to external clients,
- handed to CLI agents through their MCP config,
- used by the in-app assistant (v1) via the API providers' tool calling.

Initial tool set (to refine): `list_accounts`, `list_mailboxes`, `search`, `get_thread`, `get_message`, `create_draft`, `update_draft`, `request_send` (requires approval), `archive`, `mark_read`, `apply_label`, `remove_label`, `move`, `snooze`.

### 7.3 Guardrails

Every command carries an **actor**: `User`, `Agent { provider }` or `Mcp { client }`. Guardrails are enforced in the command pipeline, not in the tools, so no entrypoint can bypass them.

- MCP server **off by default**, enabled **per account**.
- **No permanent deletion** by agents (trash at most).
- **Sending requires approval in the app**: agents create drafts; the user sends.
- **One-time codes and magic links are redacted** before any model reads a message (MCP and in-app alike).
- Every agent action is **journaled and undoable**, and visible in an activity log.
- MCP HTTP server binds to `127.0.0.1` only, requires the bearer token, and validates `Host`/`Origin` headers (DNS-rebinding protection).

Autonomy (later) plugs into the same mechanism: a per-account allow-list of commands an agent may run without approval.

## 8. Architecture

### 8.1 Overview

Hexagonal architecture (ports & adapters) with a light CQRS split in the core and in-memory domain events. No event sourcing: the mail server is the source of truth, so rebuilding state from our own events would mean reconciling twice.

Three groups around one executable:

```
mailapp/          the executable: builds the infrastructure adapters, hands them to the core, opens the UI
     │ assembles
ui/ · mcp/        talk to the user (GPUI) and to agents (HTTP): send actions, run queries, listen to events
     │ use
core/             the mail logic, no I/O
                  model: Account · Message · Thread · Label · Draft · Contact · pure rules
                  actions (commands) · queries · events · ports (traits) · event bus · sync engine · tool registry
     ▲ ports implemented by
infrastructure/   talk to the outside world: imap · imap-gmail · smtp · microsoft · jmap · sqlite · secrets · contacts-*
                  agent-anthropic · agent-openai-compat · agent-cli
```

Dependencies always point inward: `ui`, `mcp` and every infrastructure crate depend on `core`, and `core` depends on none of them. Each is its own crate, so the compiler forbids the core from importing GPUI, SQLite or IMAP. Inside `core`, the model stays free of ports and I/O by convention.

`mail-render` is a piece of the UI, in its own crate so that Blitz's heavy dependencies and a possible engine swap stay out of `ui`.

### 8.2 Commands, queries, events

- **Commands** are the only way to change state (`ArchiveThread`, `ApplyLabel`, `SendDraft`, `Snooze`, `SummarizeThread`…). Each goes through one pipeline:

  ```
  command(actor) → validate → guardrails → domain change
    → store transaction: new state + journal entry (with its inverse)
    → emit domain events
    → outbound worker replays the journal entry to the provider (retries, offline queue)
  ```

  The journal gives optimistic UI, the offline queue and undo from a single mechanism. Undo = execute the inverse.

- **Queries** read SQLite directly in UI-ready shapes (`ThreadList`, `ThreadDetail`, `Search`). No separate read model, no projections.

- **Domain events** (`MessageReceived`, `ThreadArchived`, `DraftSent`, …) are published on an in-memory bus after commit. New behavior subscribes to events instead of being wired into existing code.

- **Reliability: state is the queue.** Work that must not be lost is expressed as state (e.g. `classified_at IS NULL`, `snoozed_until <= now`). Handlers are idempotent and catch up at startup with a query. No outbox.

Example — adding agent classification later is one handler and one prompt:

```
IMAP sync → MailStore.save → MessageReceived
  → ClassifyOnReceive handler (core)
  → AgentProvider.complete(prompt)
  → command ApplyLabel { actor: Agent } → journal (undoable) → provider
```

### 8.3 Provider adapters and capabilities

Illustrative (final signatures will use boxed futures or `async-trait` for dyn-compatibility):

```rust
pub trait MailProvider: Send + Sync {
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>>;
    async fn sync(&self, mailbox: &MailboxId, since: SyncState) -> Result<SyncDelta>;
    async fn fetch_body(&self, id: &RemoteId) -> Result<RawMessage>;
    async fn apply(&self, change: &RemoteChange) -> Result<()>; // flags, move, trash, draft append

    // Optional capabilities: the core asks, and falls back to a local strategy when absent.
    fn labels(&self) -> Option<&dyn Labels> { None }
    fn server_threads(&self) -> Option<&dyn ServerThreads> { None }
    fn push(&self) -> Option<&dyn Push> { None }
    fn server_search(&self) -> Option<&dyn ServerSearch> { None }
    fn native_snooze(&self) -> Option<&dyn NativeSnooze> { None }
}

pub trait MailSender { async fn send(&self, msg: &OutgoingMessage) -> Result<()>; }
```

Other ports: `MailStore`, `AgentProvider`, `ContactSource`, `SecretStore`, `Notifier`, `Clock`.

`imap-gmail` reuses `imap` and only adds the `labels` and `server_threads` capabilities based on X-GM extensions, syncing "All Mail" once.

### 8.4 Mail rendering pipeline

```
raw MIME → mail-parser → pick text/html (fallback text/plain)
  → sanitize (strip scripts, forms, event handlers)
  → preprocess legacy attributes into inline CSS
     (cellpadding, cellspacing, valign, table border, <font>…)
  → strip tracking pixels, unwrap tracked links
  → Blitz document (own thread — it is not Send)
     · NetProvider: remote resources blocked unless allowed; fetched without cookies/referrer
     · layout, then rasterize viewport bands into GPUI images (cache with memory budget)
  → MailBody element: selection, link hit-testing, copy
```

Mitigations for Blitz being beta: a pinned fork carrying our patches, a corpus of real (anonymized) emails under snapshot tests, and the plain-text view + "Open in browser" fallback. Everything is isolated behind `MailBody`, so swapping the engine (e.g. back to a webview) touches one component.

### 8.5 Sync engine

- Network runs on a dedicated **tokio** runtime; GPUI's executor drives the UI. They talk through channels.
- IMAP: `CONDSTORE`/`QRESYNC` when available, `UIDVALIDITY` handling, IDLE on Inbox, polling elsewhere.
- Depth window per account; bodies and attachments fetched lazily beyond it.
- **Snooze / send later**: provider-native when the `native_snooze` capability exists; otherwise the message moves to a server `Snoozed` folder (label on Gmail) so other devices see it leave the inbox, and returns when due. Due items only fire while mailapp runs; missed ones are caught up at startup (state is the queue).

### 8.6 UI

- The UI only talks to the core: it sends actions (commands), runs queries and listens to events. It never sees IMAP or SQLite.
- Inside the `ui` crate, `design/` holds the tokens and components (the only code that knows how things look) and `screens/` holds one view and one view-model per screen.
- One **view-model per screen**, as a GPUI entity: holds query results, subscribes to relevant domain events, calls `cx.notify()`. Views only render and dispatch commands. Changes made by an agent through MCP show up in the UI automatically.
- Shortcuts use GPUI actions and `KeyBinding`; the keymap is stored in settings and edited in the settings screen.
- i18n with Fluent (English, French).
- Visual language, tokens, components, motion and screens: see [DESIGN.md](DESIGN.md).

### 8.7 Mobile readiness

`core` and the infrastructure crates must not depend on desktop-only APIs. CI runs `cargo check --target aarch64-apple-ios` on them. Zed ships an experimental `gpui_ios` backend (touch, scrolling, native text input; no file pickers yet). The real blocker for iOS is background mail notifications, which require a push server — at odds with "no mailapp server". To be decided when relevant.

### 8.8 Workspace layout

```
crates/
  core/                  model, rules, actions, queries, events, ports, sync engine (no I/O)
  infrastructure/
    sqlite/              MailStore + journal (SQLCipher, FTS5)
    imap/                generic IMAP (async-imap)
    imap-gmail/          Gmail capabilities on top of imap
    smtp/                lettre
    microsoft/           OAuth2; Graph backend in v1
    jmap/                v1 (jmap-client)
    secrets/             keyring
    contacts-macos/  contacts-carddav/  contacts-microsoft/
    agent-anthropic/  agent-openai-compat/  agent-cli/
  ui/                    GPUI: design system (design/), screens and view-models (screens/), i18n
  mcp/                   MCP HTTP server
  mail-render/           sanitize + preprocess + Blitz → GPUI, used by ui
  mailapp/               the executable: assembles everything
```

Package names are prefixed `mailapp-` (`mailapp-core`, `mailapp-imap`…); the executable is `mailapp`. Zed's `ui` crate is imported as `zed_ui` (`zed_ui = { package = "ui", … }`), so `ui` in our code never means two things.

## 9. Security & privacy

- **Database** encrypted with SQLCipher; random 256-bit key stored in Keychain / Credential Manager.
- **Attachments and body cache** encrypted on disk (AEAD, key derived from the master key).
- **Credentials and API keys** only in the OS secret store.
- **Remote content** blocked by default; trackers stripped; no JavaScript is ever executed.
- **Agents**: guardrails of §7.3; automatic processing is opt-in per account (later).
- **Network**: the app talks only to mail providers, configured agent providers and (when allowed) remote images.

## 10. Tech stack

| Concern | Choice |
|---|---|
| UI | `gpui`, `ui`, `theme` from the Zed repo (pinned commit) |
| HTML rendering | `blitz-dom` / `blitz-html` + `anyrender_vello_cpu` (pinned fork) |
| Async network | `tokio` |
| IMAP | `async-imap` |
| SMTP | `lettre` |
| JMAP (v1) | `jmap-client` |
| MIME | `mail-parser`, `mail-builder` |
| OAuth2 | `oauth2` |
| Storage | `rusqlite` with bundled SQLCipher, FTS5 |
| Secrets | `keyring` |
| i18n | `fluent` |

## 11. Testing

- **Core**: unit tests with in-memory fakes of every port (`FakeMailProvider`, `FakeAgentProvider`, …).
- **Mail adapters**: integration tests against real servers in Docker (Stalwart for IMAP/JMAP/SMTP, GreenMail as a second IMAP implementation).
- **Rendering**: snapshot tests on a corpus of real, anonymized emails (newsletters, transactional, Outlook-generated).
- **Agents**: recorded responses replayed in CI; no network calls.

## 12. Risks

| Risk | Mitigation |
|---|---|
| Blitz is beta; email HTML fidelity gaps | Attribute preprocessing, pinned fork with patches, snapshot corpus, text/browser fallback, engine isolated behind `MailBody` |
| GPUI is not published; API churn | Pin a Zed commit; upgrade deliberately |
| Zed's `ui` has never been used outside Zed (as far as we know) | Validate in the first prototype; vendor components if needed |
| Block-based WYSIWYG is likely half of the MVP UI effort | Start early; keep the block model small |
| Gmail without OAuth depends on app passwords (2FA required) | Revisit Google OAuth if the project grows |
| Snooze / send later only fire while the app runs | Startup catch-up; document the limitation |
| iOS notifications need a server | Deferred; architecture stays mobile-ready |

## 13. First milestone (M0 — prototype)

Goal: retire the biggest technical risks before building features.

1. Workspace skeleton; Zed `gpui` + `ui` + `theme` pinned and compiling on macOS and Windows.
2. Blitz renders a corpus of ~30 real emails inside a GPUI window — fidelity and performance assessed. **Decision gate**: if fidelity is unacceptable, revisit the rendering engine.
3. One IMAP account synced into SQLCipher, threads listed in a virtualized GPUI list.
