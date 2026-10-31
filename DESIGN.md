# mailapp — Design

> Direction **A3 · Matière**: opaque glass with real thickness, quiet monochrome surfaces with a single blue accent, and motion that explains every change of state. Nothing in the visual language is specific to macOS or Windows.

- **Interactive mockups** (light and dark, private canvas): https://claude.ai/artifact/DSZrLaz6NV6odo6vJSA1XG — pages « A3 · Matière » and « A3 · Sombre ».
- **Sources**: `design/a3/*.dc.html`. They are Design Component files: they render in the canvas above, not as plain HTML. Their `<style>` blocks hold the exact recipes summarised below.
- **References** that shaped the direction: Linear (receding chrome, density), Arc (accounts as colored spaces), Things 3 (calm list, round checkbox, delight).

## 1. Principles

1. **Content first.** Sidebar and toolbars recede; the conversation is the brightest, most raised thing on screen.
2. **Three levels of relief, never more.** Flat frame → raised panels → maximum relief for the selection and the primary button.
3. **One accent**, used for state and action only (unread, selection, focus, Send, agent).
4. **Accounts are spaces.** Each account has a color, a tile and its own window tint; « Tous les comptes » is the unified inbox.
5. **The inbox is a to-do list.** A round checkbox archives; every action can be undone from a toast.
6. **Motion explains.** Every state change animates in under 450 ms and is disabled by `prefers-reduced-motion`.
7. **Keyboard everywhere.** Shortcuts are shown next to the actions they trigger; ⌘K reaches everything.

## 2. Materials

| Level | Used for | Recipe (light) | Recipe (dark) |
|---|---|---|---|
| Frame (flat) | Window background, sidebar | Per-space gradient + grain (fractal noise, multiply, 8 %) | Near-black per-space gradient + grain (overlay, 7 %) |
| Panel `.slab` | List, reader, settings, sheets | `#FFFFFF`→`#F7F8FB` at 90–95 %, backdrop blur 30 px · inset top highlight `#FFF` · inset bottom shade · 1 px outline `rgba(16,24,40,.07)` · 2 px bottom edge · soft drop `0 18px 40px -14px` | `#16161A`→`#0F0F12` at 95 % · top rim `rgba(255,255,255,.07)` · darker edges below |
| Raised `.lift` | Selected row (sliding plate), popovers, menus | `#FFFFFF`→`#F6F7FA` · 1 px outline · 2 px edge · drop `0 10px 22px -12px` | `#232428`→`#1A1B1E` · top rim `rgba(255,255,255,.09)` |
| Primary `.pri` | Send, main call to action | `#3170E6`→`#265DCB` · top highlight 28 % · 1 px `#2459C4` outline · 2 px `#1E4CA8` edge | Same fill, darker outline/edge |
| Small control `.chip` | Action bar, selected segment | Like `.lift`, smaller drop | Like `.lift`, smaller drop |
| Recessed `.well` | Inputs, filters, Claude summary, reply field | `rgba(16,24,40,.045)` + inset shadow | `rgba(0,0,0,.24)` + inset shadow |
| Flat `.ghost` | Secondary buttons | 1 px inset line; becomes a chip on hover | Same |

Interaction states: hover lifts by 1 px, press sinks by 1 px with an inset shadow. Secondary controls stay flat until hovered.

## 3. Color

| Token | Light | Dark |
|---|---|---|
| Text | `#18181B` | `#EDEDEF` |
| Secondary text | `#5F5F68` | `#A1A1AA` |
| Accent fill | `#2F6FEB` | `#2F6FEB` |
| Accent text / icons | `#2962CF` | `#82A9F3` |
| Rings, focus | `#2F6FEB` | `#5B8FF5` |
| Frame base | `#EEF0F5` | `#050506` |
| Warning text | `#B45309` | `#E08A3C` |
| Destructive text | `#C0261A` | `#F28B80` |

Accounts (tiles and dots, white letters): Perso `#B45309`, Travail `#0F766E`, Gmail `#6D28D9`, Tous `#2F6FEB`. In dark, account colors used **as text** switch to `#E08A3C`, `#2BB3A0`, `#A37BF5`, `#6EA0F7`.

Window tint per space (gradient at 155°):

| Space | Light | Dark |
|---|---|---|
| Tous | `#D6E2FB` → `#EDF0F7` → `#E3E0F5` | `#0C1120` → `#070709` → `#0F0C18` |
| Perso | `#F7E1C8` → `#F4F0EA` → `#F1DBD0` | `#170F06` → `#080809` → `#140B09` |
| Travail | `#CDEDE6` → `#EDF3F1` → `#DAE7F2` | `#061512` → `#08090A` → `#070F15` |
| Gmail | `#E4D9FA` → `#F1EEF7` → `#F3DCEA` | `#110B1E` → `#08080B` → `#150B13` |

Dark is near-black: the frame is almost `#000`, panels are only a few steps lighter, and depth comes from the top rim highlight rather than from lighter fills.

Every text meets 4.5:1 on its actual background, including over the tints.

## 4. Typography

Inter only, with optical sizes. One family on every platform.

| Role | Size / weight |
|---|---|
| Meta, shortcuts | 11.5 px |
| Secondary, previews | 12–12.5 px |
| UI | 13 px / 400–550 |
| Reading | 14–14.5 px, line-height 1.65 |
| Panel titles | 15 px / 650 |
| Subject, page titles | 26–28 px / 650, tracking −0.024 em |

Counts and times use tabular figures. Unread = weight 650 on the sender.

## 5. Shape and layout

- Window padding 10 px, gap between panels 10 px.
- Radii: panels 18, rows and sheets 13, controls 9–11, pills 11–15.
- Main window: sidebar 216 px on the frame · conversation list (440 px while reading, full width when the reader is closed) · reader (article max 660 px).
- Panels sit side by side; they never overlap.

## 6. Components

- **Sidebar**: space header (tile + name), search (⌘K), flat nav (selected = translucent white), agent status, space switcher at the bottom.
- **Conversation row**: round checkbox (archives; blue ring when unread), sender, account square (only in « Tous »), time, subject, one-line preview, agent badge in a well.
- **Sliding selection**: one raised plate that slides from row to row.
- **Reader**: single action bar (Archiver E | Reporter H | Répondre R), subject, account chip, Claude summary in a well with suggested actions, the thread as **stacked sheets** (older messages narrower and partly tucked under the next; click to unfold), reply well with « Rédiger avec Claude » and Send.
- **Toast**: dark pill at the bottom with « Annuler », 5 s.
- **Context menu**: a raised popover (radius 14, 288 px). From top to bottom: four quick-action tiles in a well (Archiver E, Reporter H, Répondre R, Non lu U), then a short list with icons and keycaps (open in a new window, reply all, forward · move to, labels · Claude actions · trash in destructive red). The highlighted item is a raised plate, like the selected row, never an accent fill. The right-clicked row shows as selected. « Déplacer vers » opens a side panel with its own filter, recent folders (1, 2) and the account's folders.
- **Popovers**: `.lift`, 32 px items, shortcuts as small keycaps on the right.
- **Controls**: switches with a raised knob, segmented controls (well + chip), swatches as a radio group.
- **Composer**: block editor with floating format bar, « / » block menu, block handle, file blocks, agent draft banner, recipient tokens with autocomplete.

## 7. Motion

| Interaction | Animation | Timing |
|---|---|---|
| Open a conversation | Reader rises 12 px, scales .985 → 1, fades in | 420 ms `cubic-bezier(.2,.8,.2,1)` |
| Move the selection | Plate slides to the new row with a slight overshoot | 360 ms `cubic-bezier(.3,1.3,.5,1)` |
| Archive | Checkbox fills, check draws; row slides right 36 px and collapses; toast rises | 260 / 300 / 340 ms |
| Close the reader | List widens | 450 ms |
| Change space | Window tint cross-fades; space tile recolors | 700 / 500 ms |
| Unfold a sheet | Margins open, content fades down | 340 / 300 ms |
| Hover / press | Lift 1 px / sink 1 px | 160 ms |

With reduced motion, all transitions and animations are off.

## 8. Screens

| Screen | Light | Dark |
|---|---|---|
| Inbox (unified, spaces, reader) | `PropM-Main` | `PropM-Main-Sombre` |
| Composer | `PropM-Composer` | `PropM-Composer-Sombre` |
| Conversation in its own window | `PropM-Conversation` | `PropM-Conversation-Sombre` |
| Command palette ⌘K | `PropM-Palette` | `PropM-Palette-Sombre` |
| Settings (7 sections) | `PropM-Reglages` | `PropM-Reglages-Sombre` |
| Add account (provider → Microsoft / Gmail / IMAP → finish) | `PropM-Ajout` | `PropM-Ajout-Sombre` |
| First launch | `PropM-Bienvenue` | `PropM-Bienvenue-Sombre` |
| Row context menu | `PropM-Menu` | `PropM-Menu-Sombre` |
| Snooze (natural language) | `PropM-Reporter` | `PropM-Reporter-Sombre` |

All files are in `design/a3/` with the `.dc.html` extension.

## 9. Notes for the GPUI implementation

- Panels are 90–95 % opaque: a solid gradient is an acceptable fallback wherever in-window backdrop blur is not available.
- If the renderer has no inset shadows, draw the top highlight as a 1 px light top border and the thickness as a 2 px darker bottom border.
- Grain is a small tiled noise texture over the frame only.
- Bundle Inter with the app so both platforms render the same.
- The OS draws the window controls (left on macOS, right on Windows); keep that corner of the frame free.
