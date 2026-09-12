# UI baseline inventory

## Application shell

- Window: 460×600, fixed size, always on top, title `HEAT3 Поворотник v2.0.0`, Windows subsystem, `logocube.ico`.
- Layout: 140 px left navigation and central content panel; 8 px page spacing; light/dark themes.
- Navigation pages, in order: Поворотник, 2D Поворотник, Сортировка, Прослойки, Проверка, Угол окна, 3D -> STEP, Шкала, Лицензия.
- Platform behavior: dark titlebar follows Windows `AppsUseLightTheme`; clipboard and native file dialogs; legacy OpenGL move-recovery must disappear after wgpu cutover.

## Page inventory

| Page | Primary controls | Domain behavior | External effects |
|---|---|---|---|
| Поворотник | paste, projection picklist, instruction overlay, rotate/mirror/axis buttons, copy | transforms 3D script | clipboard |
| 2D Поворотник | paste, instruction overlay, projection/action buttons, 2D preview, copy | turner2d transforms | clipboard |
| Сортировка | paste, open materials, reorder up/down, sort, copy | material_sort | clipboard, file dialog |
| Прослойки | paste, MTL file, name mask, checkboxes, material swatches, create | air_cavities | clipboard, file dialog |
| Проверка | paste, plane/cavity checks, simplify checkboxes/collapse, copy | model_check | clipboard |
| Угол окна | paste, direction picklist, create, copy | corner | clipboard |
| 3D -> STEP | paste, parse/status, 3D preview, export | parser + step_export | clipboard, file dialog |
| Шкала | paste, MTL file, material scale canvas, selectable table, PNG, copy | report/material data | clipboard, file dialog, PNG |
| Лицензия | masked key input, reveal, paste, activate, refresh, deactivate confirmation, status panel | licensing manager | clipboard, network, encrypted storage |

## Shared interaction contracts

- Category-colored full-width action buttons preserve their existing labels and order.
- Enabled/disabled states must follow current busy/configuration/data guards.
- Status ladder: success, warning, error, info, muted; dark mode preserves readable contrast.
- Hover tooltips exist for material rows, reorder actions, report scale, and instruction affordances.
- Drag interactions: 3D rotation uses pointer delta; modifier semantics are preserved (Ctrl/Shift). 2D preview hover identifies segments.
- Overlays: instruction and about dialogs are modal, non-resizable, centered, and explicitly dismissible.
- Security: license keys are masked and zeroized; production builds must not embed dev-license secrets.

## Behavioral parity scenarios

### Happy path

- Paste valid script on every applicable page; run the page action; inspect result; copy/export result.
- Load valid MTL and verify material ordering, labels, colors, and conductivity output.
- Open 3D/2D previews and interact with projection and rotation controls.
- Activate, refresh, and deactivate a valid license; verify dev-license bypass separately.

### Boundary and recovery

- Empty clipboard, invalid script, malformed MTL, missing segments, unsupported projection, empty material list.
- File dialog cancel and unreadable file; export write failure; clipboard write failure.
- Busy licensing operation disables duplicate actions; offline lease/service unavailable states remain actionable where allowed.
- Move fixed-size always-on-top window and verify iced/wgpu surface remains rendered.
- Switch light/dark mode and verify status/category colors, typography, titlebar, and overlays.

## Acceptance checklist

- [x] All nine pages are reachable in the listed order.
- [x] Button labels, category colors, and relative positions match the egui reference.
- [x] Light and dark shells render with Swiss tokens and bundled fonts.
- [x] Clipboard/file-dialog/PNG flows and failure messages are preserved.
- [x] License real-server and dev-license flows are covered.
- [x] 2D hover/preview and 3D drag/modifier behavior are covered.
- [x] `cargo fmt`, clippy with `-D warnings`, tests, audit, hygiene, secret scan, and packaging are green.

## Reference surfaces

- `rust/src/ui/mod.rs`: shell, page order, state ownership, licensing polling.
- `rust/src/ui/*_page.rs`: page-level controls and domain calls.
- `rust/src/ui/preview.rs`: 2D projection and 3D interaction.
- `rust/src/ui/design/{tokens,theme,fonts}.rs`: source design values.
