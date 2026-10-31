# mailapp — Implementation plan

> High-level plan to go from an empty repo to the MVP of [PRODUCT.md](PRODUCT.md), in the visual language of [DESIGN.md](DESIGN.md). Section numbers like §8.2 refer to PRODUCT.md.

## How the work is sliced

- **Risks first.** M0 answers the questions that could change the stack (GPUI + `ui` outside Zed, Blitz, text input) before any feature is built.
- **Vertical milestones.** Each one ends with something used for real (read, then triage, then write), not with a finished layer.
- **Daily use as early as possible.** From M3 on, mailapp is the daily client for IMAP/Gmail accounts; what hurts in daily use decides the order of M4–M6.
- **What is painful to retrofit is there from day one**: encryption, migrations, i18n keys, theme tokens, the command pipeline with its actor, the label-based model, CI on all targets.
- **macOS first.** Windows builds in CI from M0, gets a smoke test at the end of each milestone, and a full parity pass in M7.

Sizes are relative: **S** < **M** < **L**. An **L** step spans several PRs and gets its own short plan before it starts.

```
M0 Prototype ─► M1 Read ─► M2 Triage ─► M3 Write ══► daily use starts
                                           │
                 ┌─────────────────────────┼─────────────────────────┐
                 ▼                         ▼                         ▼
          M4 Rich composer          M5 Coverage              M6 Agents & MCP
                 └─────────────────────────┼─────────────────────────┘
                                           ▼
                                   M7 Finish → MVP
```

| Milestone | Outcome | Size |
|---|---|---|
| M0 Prototype | The stack holds, or we know what to change | M |
| M1 Read | All IMAP/Gmail accounts readable in mailapp | L+ |
| M2 Triage | The inbox is processed in mailapp, with undo | L |
| M3 Write | Mail is sent from mailapp → daily use | L |
| M4 Rich composer | Notion-style composer complete | L |
| M5 Coverage | Microsoft 365, search, address books | M+ |
| M6 Agents & MCP | External and in-app agents, behind guardrails | L |
| M7 Finish | Settings, motion, Windows parity, hardening → MVP | L |

---

## M0 — Prototype: retire the technical risks

Same goal as §13, plus two checks: a text-input spike (search, recipients, settings and the whole composer depend on it) and Microsoft consent.

| # | Step | Size |
|---|---|---|
| 0.1 | **Workspace.** Cargo workspace with the §8.8 crates M0 needs (`domain`, `application`, `sqlite`, `imap`, `mail-render`, `ui`, `app`); the others are created when first needed. `gpui` + `ui` + `theme` pinned to one Zed commit, Inter bundled. A window opens with a `ui` context menu, popover and tooltip, which shows that `ui` works outside Zed. CI: build, clippy and tests on macOS and Windows, plus `cargo check --target aarch64-apple-ios` on the core crates. | S |
| 0.2 | **Blitz, with a decision gate.** Collect about 30 real emails (newsletters, transactional, Outlook-generated, plain text) and anonymize them: this becomes the snapshot corpus. Render each one in GPUI through Blitz on its own thread. Measure fidelity next to a browser, time to first paint, scrolling and memory. **Gate**: if fidelity is not acceptable, evaluate the webview path before building M1 (`MailBody` keeps the swap local). | M |
| 0.3 | **Text input.** Single-line and multi-line inputs on GPUI's input handler, informed by gpui-component: IME (accents, dead keys) on macOS and Windows, selection, clipboard, undo. | M |
| 0.4 | **IMAP → SQLCipher → list.** One account, Inbox headers synced on a tokio runtime, stored in SQLCipher (key in the OS secret store), shown in a virtualized list fed through a channel. | M |
| 0.5 | **Microsoft consent check** (one hour). Register the Azure app and try consent with each tenant you use. Many company tenants block user consent for third-party apps; better to know now. | S |

**Done when** each risk has a verdict and the 0.4 list scrolls 20k messages smoothly, starting from the database without the network. Spike code that does not fit the layers is discarded; the workspace, CI and corpus stay.

## M1 — Read: a read-only client for every account

Read in mailapp; still triage and send elsewhere. Nothing is written to the server yet.

| # | Step | Size |
|---|---|---|
| 1.1 | **Domain and storage.** Account, Identity, Mailbox, Label, Message, Thread. Folders are exclusive labels from the start, so Gmail fits without a second model. SQLCipher schema with versioned migrations from the first table, FTS5 table filled at sync time, encrypted cache for bodies and attachments (§9). Ports `MailProvider` (with capabilities), `MailStore`, `SecretStore`, `Clock`, each with an in-memory fake. | M |
| 1.2 | **Sync engine, generic IMAP.** Mailbox list, initial sync inside the depth window, incremental sync with CONDSTORE/QRESYNC, UIDVALIDITY resets, IDLE on Inbox and polling elsewhere, bodies fetched lazily. A per-account connection budget (IDLE, sync, fetch and later actions share it), reconnects with backoff. Local threading (References / In-Reply-To) when the provider has no `server_threads`. Integration tests against Stalwart and GreenMail in Docker. | L |
| 1.3 | **Gmail capabilities.** `imap-gmail`: All Mail synced once, each message stored once by `X-GM-MSGID`, threads from `X-GM-THRID`, labels from `X-GM-LABELS`. | M |
| 1.4 | **Design foundation.** DESIGN.md tokens (colors light and dark, space tints, radii, type scale) and the materials (`slab`, `lift`, `pri`, `chip`, `well`, `ghost`) as GPUI elements, with the fallbacks of DESIGN §9. Fluent wired in now: every string goes through a key from the first view (FR filled as you go, EN completed in M7). | M |
| 1.5 | **Main window, read-only.** Sidebar (spaces, nav, labels), conversation list (unread, account square in « Tous »), reader with the thread. One view-model per screen, refreshed by domain events (§8.6). `j`/`k`, Enter, Esc, switching spaces. The unified inbox and per-account spaces. Animations come in M7. | L |
| 1.6 | **Rendering pipeline** (§8.4). mail-parser → sanitize → legacy attributes to inline CSS → strip trackers, unwrap links → Blitz → `MailBody` (selection, link hit-testing, copy). Remote images blocked, allowed per message or per sender. Plain-text view, « Open in browser ». Attachments: open and save. Snapshot tests on the corpus. | L |
| 1.7 | **Add account and first launch.** Generic IMAP and Gmail app password flows (`PropM-Ajout`), a minimal `PropM-Bienvenue`. Until M7, everything else is a default in code. | S |

**Done when** every IMAP/Gmail account you use is readable in mailapp, new mail arrives through IDLE, and a restart shows everything instantly from the local database.

> If one of your daily accounts is Microsoft 365, pull 5.1 into M1.

## M2 — Triage: act on mail, with undo

| # | Step | Size |
|---|---|---|
| 2.1 | **Command pipeline** (§8.2), the backbone of everything after it: command(actor) → validate → guardrails → domain change → transaction (state + journal entry with its inverse) → events → outbound worker replaying to the provider with retries. Pending journal entries are the offline queue; undo runs the inverse. The actor (`User`, `Agent`, `Mcp`) is present from the first command, even though only `User` exists yet. In-memory event bus. | L |
| 2.2 | **Triage actions.** Read/unread, star, archive, trash, move, add/remove label, each as a command plus its `RemoteChange` for IMAP and Gmail. Round checkbox that archives, undo toast (5 s), context menu (`PropM-Menu`) with its « Déplacer vers » panel. | M |
| 2.3 | **Palette and keyboard.** One action registry: each command has a GPUI action, a shortcut and a palette entry (⌘K, `PropM-Palette`). Shortcuts shown next to their actions. The keymap lives in the database; its editor comes in M7. | M |
| 2.4 | **Snooze** (§8.5). A small hand-written parser for FR and EN (today, tomorrow 9am, this weekend, next week, in N days, weekday + time), with tests. Server `Snoozed` folder (label on Gmail), `snoozed_until` as state, due items handled at runtime and caught up at startup. `PropM-Reporter`. | M |
| 2.5 | **Native notifications** for new Inbox mail. | S |

**Done when** a full day of triage happens in mailapp, the changes show up on your phone or webmail, and actions taken offline replay on reconnect.

## M3 — Write: send mail, and start using mailapp daily

| # | Step | Size |
|---|---|---|
| 3.1 | **Sending.** `MailSender` port, SMTP adapter (lettre), MIME built with mail-builder. Identities, aliases and signatures per account. A copy goes to Sent unless the provider stores it itself (Gmail does): a capability, not a conditional. Sending is a command (`SendDraft`), so a failed send retries and never loses the draft. | M |
| 3.2 | **Minimal composer.** Small block model: paragraph, list, quote; bold, italic, link. Produces `text/html` and `text/plain`. Reply / reply all / forward with quoting and correct `In-Reply-To` / `References`. Recipient tokens with autocomplete from history (frequency × recency). Attachments through picker and drag & drop. Drafts autosaved locally and synced to the server. | L |

**Done when** you have stopped opening your old client for IMAP/Gmail accounts. From here on, bugs from daily use come first, and M4, M5 and M6 are ordered by what you miss most.

## M4 — Rich composer

| # | Step | Size |
|---|---|---|
| 4.1 | Floating format bar, « / » block menu, block handle with drag-to-reorder, file blocks, agent draft banner slot (filled in M6). Composer window per `PropM-Composer`. Keep the block model small (§12). | L |

## M5 — Coverage: Microsoft, search, address books

| # | Step | Size |
|---|---|---|
| 5.1 | **Microsoft 365 / Outlook.com.** OAuth2 with PKCE and a loopback redirect, XOAUTH2 over IMAP and SMTP, token refresh, tokens in the secret store. Add-account flow for Microsoft. | M |
| 5.2 | **Search.** FTS5 plus operators (`from:` `to:` `subject:` `has:attachment` `in:` `is:unread` `before:` `after:`), in the palette and the list. Server-side search through the `server_search` capability (IMAP `SEARCH`, Gmail `X-GM-RAW`) for history outside the depth window. | M |
| 5.3 | **Address books.** `ContactSource` port: macOS Contacts, CardDAV, Microsoft Graph contacts, merged into autocomplete. | M |

## M6 — Agents & MCP

| # | Step | Size |
|---|---|---|
| 6.1 | **Tool registry and guardrails** (§7.2, §7.3). Tools are thin wrappers over commands and queries. Guardrails live in the pipeline: agents cannot delete permanently, sending needs approval in the app, one-time codes and magic links are redacted before any model reads a message, each account opts in. An activity log screen reads the journal and offers undo. | M |
| 6.2 | **MCP server.** Streamable HTTP on `127.0.0.1`, bearer token, `Host`/`Origin` checks, off by default. Tested with Claude Code and opencode as clients. It is useful before any in-app AI exists: Claude Code can already triage the mailbox. | M |
| 6.3 | **Agent providers.** `AgentProvider` port. CLI adapter first (`claude`, `opencode` found in `PATH`, given mailapp's MCP config: it reuses their login, so there is no key to manage), then `anthropic` and `openai-compat` (OpenRouter). Recorded responses for CI. | M |
| 6.4 | **Agent features in the UI.** Thread summary with suggested actions in the reader, draft a reply from a one-line intent, rewrite, translate, « Rédiger avec Claude », agent status in the sidebar. | M |

## M7 — Finish the MVP

| # | Step | Size |
|---|---|---|
| 7.1 | **Settings screen** (`PropM-Reglages`): general, accounts and identities, sync, appearance, content and privacy, agents and MCP, shortcuts. Everything stored in the encrypted database. | L |
| 7.2 | **Motion and finish.** DESIGN §7 animations, reduced motion, light/dark review, contrast check over the tints, EN strings completed. | M |
| 7.3 | **Windows parity.** Credential Manager, notifications, window controls on the right, IME, file pickers, drag & drop, a full manual pass on every screen. | M |
| 7.4 | **Hardening.** Journal crash safety, a mailbox with 100k+ messages, resync after UIDVALIDITY changes, render cache memory budget, attachment preview. | M |

**Done when** the MVP goal holds: mailapp is your only mail client on macOS and Windows.

---

## Rules from day one

- One crate per layer and per adapter; dependencies point inward, and the compiler enforces it.
- Every port has an in-memory fake; adapters are tested against Stalwart and GreenMail in Docker; agent tests replay recorded responses.
- Nothing is ever written in plaintext: SQLCipher and the secret store from the first byte, so there is never a plaintext-to-encrypted migration.
- Migrations from the first table. Before daily use, a schema change can wipe and resync; after it, local-only state (journal, snoozes, settings) must survive.
- No string or color literals in views: Fluent keys and theme tokens.

## After the MVP

Suggested v1 order, reusing what exists: undo send and send later (the journal and the snooze mechanism) → « À répondre » and smart tabs → one-click unsubscribe → full offline compose → rules → in-app assistant and local models → JMAP → Graph backend → calendar invites → Dock badge.

## If the MVP slips

These MVP items can move to v1 without hurting daily use. Moving them changes the spec, so the decision belongs to the product owner:

- Address books (5.3): history-based autocomplete covers most of the need.
- Server-side search (part of 5.2).
- The shortcut editor (part of 7.1): the keymap stays in the database, just without an editor.
- The `anthropic` and `openai-compat` providers (part of 6.3): CLI agents through MCP already do the job.
- Windows parity (7.3), if macOS is the daily machine.

## Open questions

1. **Which providers do you use daily?** That decides whether Gmail or Microsoft comes first, and whether 5.1 moves into M1.
2. **The mockups show items PRODUCT.md puts in v1 or later**: the « À répondre » view, the Dock badge, a conversation in its own window. This plan keeps them out of the MVP; the mockups only guide the layout.
3. **The email corpus**: real messages, even anonymized, should not end up in a public GPL repo without a careful scrub. A git-ignored folder, or a private repo?
4. **MCP crate**: the official Rust SDK (`rmcp`) if its pinned version supports a streamable HTTP server, otherwise a small hand-written server. To decide at 6.2.
5. **Windows**: is there a machine for the per-milestone smoke tests, or only CI until M7?
