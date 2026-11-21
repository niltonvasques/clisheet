# clisheet

**English** · [Português (Brasil)](README.pt-BR.md)

clisheet is a terminal spreadsheet editor written in Rust. Open a multi-sheet XLSX workbook, edit cells with the keyboard or mouse, and recalculate formulas locally. The interface and command names are in Brazilian Portuguese. Use `--locale en-US` for US number and formula syntax.

https://github.com/user-attachments/assets/d22ee96b-70b0-41ac-ab63-14fc39a6d560

In the sales demo, changing the coffee quantity from 2 to 4 updates the total from 35 to 55. The recording shows the calculated column, range selection, undo and redo, theme switching, and the result after saving and reopening the workbook.

## Build and run

From the project root, build with Rust 1.94 or later and Cargo. The first build needs access to the dependencies or a populated Cargo cache.

```sh
cargo build --release --locked
./target/release/clisheet examples/xlsx/resumo.xlsx
```

To install the command for your user account, run `cargo install --locked --path .` and put Cargo’s binary directory on your PATH.

```sh
clisheet
clisheet examples/xlsx/orcamento.xlsx
clisheet examples/xlsx/vendas.xlsx --read-only
clisheet examples/xlsx/resumo.xlsx --sheet "Preços 2026" --no-mouse
clisheet --locale en-US --theme dark
clisheet --recover
clisheet --help
clisheet --version
```

## Try a cross-sheet formula

1. Make a working copy with `cp -i examples/xlsx/resumo.xlsx ./my-copy.xlsx`.
2. Open `./target/release/clisheet my-copy.xlsx --sheet "Preços 2026"`. Type `:ir B2` and press Enter. Type `15` and press Enter again.
3. Click the **Resumo** tab, or type `:aba-anterior` and press Enter. A1 changes from **20** to **30**. Select it to see `='Preços 2026'!B2*2` in the formula bar.
4. Press Ctrl+S, wait for the save message, and quit with Ctrl+Q. Run `./target/release/clisheet my-copy.xlsx --sheet Resumo --read-only` to check A1 again.

In `vendas.xlsx`, D2 and D3 multiply quantity by price. D4 is the table total, while F1 sums the column by name. Both start at **35**. Open `incompativel.xlsx` to try **Salvar cópia** (Save a copy) on a workbook containing a chart.

## Keyboard and mouse

| Action | Controls |
|---|---|
| Navigate and select | Arrow keys to move, Shift+arrows to extend, Ctrl+arrows to jump to the edge of a data region, Page Up/Down to page through the sheet |
| Move across the current row | Home goes to column A, End to the last used column of the sheet |
| Jump to the start or end of the used area | Ctrl+Home, Ctrl+End |
| Move one row or column | Enter, Tab, Shift+Tab |
| Replace, edit, confirm, or cancel | Start typing, F2, Enter, Esc |
| Insert a line break while editing | Alt+Enter |
| Insert a formula reference | Click a cell or use F4 and the arrow keys. Hold Shift to extend the range. Click another tab or use F6/F7 to reference another sheet |
| Clear cell contents and keep formatting | Delete |
| Open, save, quit | Ctrl+O, Ctrl+S, Ctrl+Q |
| Undo or redo an operation | Ctrl+Z, Ctrl+Y |
| Copy, cut, paste using the internal clipboard | Ctrl+C, Ctrl+X, Ctrl+V |
| Find or go to an address | Ctrl+F, Ctrl+G |
| Switch sheets | Ctrl+Page Up/Down or click a tab |
| Help, command palette, recalculate | F1, F10, F5 |
| Select with the mouse | Click, Shift+click, or drag. Click a header to select a row or column |
| Edit or resize | Double-click a cell to edit. Drag a column divider to resize, or double-click it to fit the contents |
| Scroll or open the context menu | Mouse wheel, Shift+wheel for horizontal scrolling, right-click for actions |

The file picker supports directory navigation and direct path entry. Type `:` to enter a command when the terminal intercepts a shortcut. Enter the command names as shown and press Enter.

| Command | Effect |
|---|---|
| `:ir B2:D10` | Select a rectangular range |
| `:preencher 15`, `:baixo`, `:direita`, `:serie 1` | Fill a selection, fill down or right, or continue a series |
| `:formato percentual`, `:formato moeda`, `:formato personalizado #,##0.000` | Apply a percentage, currency, or custom number format |
| `:buscar formula SUM`, `:substituir seleção Café => Chá` | Search formulas on the sheet or replace text in the selection |
| `:congelar` at C3 | Freeze rows 1–2 and columns A–B |
| `:ordenar cab A+,B-` | Keep the header and sort by A ascending, then B descending. Use `sem` for data without a header |
| `:filtrar B >10`, `:filtrar A ~café`, `:filtrar A vazio`, `:remover-filtro` | Filter a range with a header by comparison, text, or blanks, or clear the filter |
| `:tabela tbVendas` | Create a table from a selection with unique column headers |
| `:coluna-calculada =[@Quantidade]*[@Preco]` | Set the formula for the active table column |
| `:registro`, `:totais`, `:redimensionar-tabela A1:D8` | Append a record, toggle the totals row, or resize the table |
| `:nota Conferir preço`, `:ler-nota` | Write or read a cell note |
| `:colar valores`, `:colar formulas`, `:colar formato`, `:colar transpor` | Paste values, formulas, formatting, or a transposed block |

Pasting writes to every position in the destination rectangle, including hidden rows. A cut updates the source once the paste completes. To paste external TSV data, use your terminal emulator’s paste command, often Ctrl+Shift+V. Bracketed paste supports tabs, rows, and quoted fields. For remote sessions, allocate a terminal with `ssh -t`. Use `:copiar-externo` to copy the selection as TSV to the system clipboard. Local sessions try wl-copy, xclip, xsel, or pbcopy when available, then fall back to OSC 52. Enable `--osc52` for external copying with Ctrl+C and Ctrl+X. OSC 52 allows you to copy up to 100,000 bytes to the clipboard of a compatible terminal, including over SSH.

The default theme is `light`. To start in dark mode, save `{"theme":"dark"}` in `~/.config/clisheet/config.json` or `$XDG_CONFIG_HOME/clisheet/config.json`. Override the setting for one run with `--theme light` or `--theme dark`. Type `:tema` and press Enter to switch while editing a workbook. The theme affects the display only.

Customize shortcuts in `$XDG_CONFIG_HOME/clisheet/keys.json` (default `~/.config/clisheet/keys.json`). For example, `{"F6":"ir A1"}` binds F6 to the command that selects A1.

## Exchange data with CSV and TSV

From the **Arquivo** (File) menu or the command line inside clisheet, use `:importar ponto-e-virgula latin1 /dados/vendas.csv` to import data starting at the selection. Import accepts the same typed values and formulas as the grid. Undo reverts the whole block. Blank rows keep their positions, quoted fields can contain newlines, and missing trailing fields become empty cells.

Use `:exportar seleção tab utf-8 /dados/recorte.tsv` to export the selection or `:exportar aba virgula utf-8 /dados/resumo.csv` to export a sheet’s displayed values to a new file. Delimiters are `tab`, `virgula` (comma), and `ponto-e-virgula` (semicolon). Encodings are `utf-8` and `latin1`. Use XLSX to keep formulas, multiple sheets, types, and styles.

## Cell input, saving, and recovery

With the default pt-BR locale, try `12,50`, `12,5%`, `2026-11-19`, and `=SOMA(A1;B1)`. Prefix input with an apostrophe to force text, as in `'0` or `'=literal`. Identifiers with leading zeros are preserved.

Ctrl+S saves to the open path for clisheet workbooks and supported external XLSX files with simple grids. A new workbook prompts for a path on its first save. Workbooks with features outside the supported set, such as charts, require **Salvar cópia** (Save a copy). The copy may omit those objects. When saving over its own file, clisheet checks for external changes and creates an `.xlsx.bak` backup by default. Use `--no-backup` to disable backups. After an interruption, `--recover` opens the most recent recoverable session as a workbook you can save to a new path.

## Tests and compatibility

```sh
cargo test --locked
cargo run --locked --example prova
cargo build --locked
python3 tests/independent.py
python3 tests/terminal.py
cargo run --release --locked --example benchmark
```

Both Python scripts require openpyxl (tested with 3.1.2). The pseudoterminal test runs on Linux. To regenerate the editable examples in a new directory, run `cargo run --locked --example gerar -- ./generated-examples`.

P0 and P1 are partially implemented. The compatibility matrix lists which features survive saving and when to save a copy. Project documentation is in Portuguese. [XLSX compatibility](docs/compatibility.md) · [Architecture](docs/architecture.md) · [Benchmarks](docs/benchmarks.md)
