use clisheet::{
    book::*,
    storage,
    ui::{App, Mode},
};
use crossterm::event::{
    Event, KeyCode as K, KeyEvent, KeyModifiers as M, MouseButton as Btn, MouseEvent,
    MouseEventKind as Kind,
};
use ratatui::{backend::TestBackend, Terminal};
fn app() -> Result<App> {
    Ok(App::new(Book::new("en")?, "en"))
}
#[test]
fn formulas() -> Result {
    let mut b = Book::new("en")?;
    let cases = [
        ("SUM(A1:A2)", "30"),
        ("AVERAGE(A1:A2)", "15"),
        ("MIN(A1:A2)", "10"),
        ("MAX(A:A)", "20"),
        ("COUNT(A1:A3)", "2"),
        ("COUNTA(A1:A3)", "2"),
        ("COUNTBLANK(A1:A3)", "1"),
        ("IF(FALSE,1/0,42)", "42"),
        ("AND(TRUE,FALSE)", "FALSE"),
        ("OR(TRUE,FALSE)", "TRUE"),
        ("NOT(TRUE)", "FALSE"),
        ("IFERROR(1/0,42)", "42"),
        ("SUMIF(A1:A2,\">10\")", "20"),
        ("SUMIFS(A1:A2,A1:A2,\">10\")", "20"),
        ("COUNTIF(A1:A2,10)", "1"),
        ("COUNTIFS(A1:A2,\">0\",A1:A2,\"<20\")", "1"),
        ("AVERAGEIF(A1:A2,\">0\")", "15"),
        ("ABS(-3)", "3"),
        ("ROUND(1.25,1)", "1.3"),
        ("ROUNDUP(1.21,1)", "1.3"),
        ("ROUNDDOWN(1.29,1)", "1.2"),
        ("INT(-1.2)", "-2"),
        ("MOD(7,3)", "1"),
        ("POWER(2,3)", "8"),
        ("SQRT(16)", "4"),
        ("LEFT(\"abc\",2)", "ab"),
        ("RIGHT(\"abc\",2)", "bc"),
        ("MID(\"abcd\",2,2)", "bc"),
        ("LEN(\"abc\")", "3"),
        ("TRIM(\" a  b \")", "a b"),
        ("UPPER(\"abc\")", "ABC"),
        ("LOWER(\"AbC\")", "abc"),
        ("CONCATENATE(\"A\",\"B\")", "AB"),
        ("VLOOKUP(20,A1:B2,2,FALSE)", "b"),
        ("INDEX(B1:B2,2)", "b"),
        ("MATCH(20,A1:A2,0)", "2"),
        ("YEAR(DATE(2026,4,3))", "2026"),
        ("MONTH(DATE(2026,4,3))", "4"),
        ("DAY(DATE(2026,4,3))", "3"),
        ("TODAY()>40000", "TRUE"),
        ("NOW()>=TODAY()", "TRUE"),
        ("ISBLANK(A3)", "TRUE"),
        ("ISNUMBER(A1)", "TRUE"),
        ("ISTEXT(B1)", "TRUE"),
        ("ISERROR(1/0)", "TRUE"),
        ("XLOOKUP(10,A1:A2,B1:B2)", "a"),
        ("HLOOKUP(10,A1:B2,2,FALSE)", "20"),
        ("IFNA(NA(),4)", "4"),
        ("IFS(FALSE,1,TRUE,2)", "2"),
        ("CONCAT(B1:B2)", "ab"),
        ("TEXTJOIN(\"-\",TRUE,B1:B2)", "a-b"),
        ("TEXT(12,\"0000\")", "0012"),
        ("VALUE(\"12\")", "12"),
        ("SUBSTITUTE(\"aba\",\"a\",\"c\")", "cbc"),
        ("AVERAGEIFS(A1:A2,A1:A2,\">10\")", "20"),
        ("SUMPRODUCT(A1:A2,A1:A2)", "500"),
        ("SUBTOTAL(9,A1:A2)", "30"),
        ("MONTH(EDATE(DATE(2025,1,31),1))", "2"),
        ("DAY(EOMONTH(DATE(2024,2,1),0))", "29"),
        ("NETWORKDAYS(DATE(2026,1,5),DATE(2026,1,9))", "5"),
        ("2^3+5%*20", "9"),
        ("\"a\"&\"b\"", "ab"),
        ("1/0", "#DIV/0!"),
    ];
    b.change(|b| {
        b.tsv(0, 1, 1, "10\ta\n20\tb")?;
        for (i, (expr, _)) in cases.iter().enumerate() {
            b.set(0, i as i32 + 1, 5, &format!("={expr}"))?
        }
        Ok(())
    })?;
    for (i, (expr, expected)) in cases.iter().enumerate() {
        assert_eq!(&b.val(0, i as i32 + 1, 5), expected, "{expr}")
    }
    b.change(|b| b.set(0, 1, 7, "=G1"))?;
    assert!(b.val(0, 1, 7).starts_with('#'));
    Ok(())
}
#[test]
fn structure_and_failed_batch() -> Result {
    let mut b = Book::new("en")?;
    b.change(|b| {
        b.m.new_sheet()?;
        b.m.rename_sheet(1, "Preços 2026")?;
        b.set(1, 2, 2, "15")?;
        b.set(0, 1, 1, "='Preços 2026'!B2")
    })?;
    b.change(|b| b.m.rename_sheet(1, "Preço d'água"))?;
    assert_eq!(b.val(0, 1, 1), "15");
    b.change(|b| b.m.delete_sheet(1))?;
    assert!(b.val(0, 1, 1).contains("REF"));
    b.history(false)?;
    assert_eq!(b.val(0, 1, 1), "15");
    b.change(|b| {
        b.tsv(1, 1, 1, "1\n2\n3")?;
        b.set(1, 1, 4, "=SUM(A1:A3)")
    })?;
    b.change(|b| b.m.insert_rows(1, 2, 1))?;
    assert_eq!(b.raw(1, 1, 4), "=SUM(A1:A4)");
    assert_eq!(b.val(1, 1, 4), "6");
    let before = b.version;
    assert!(b.change(|b| b.tsv(0, ROWS, 1, "first\nsecond")).is_err());
    assert_eq!(b.version, before);
    assert!(b.raw(0, ROWS, 1).is_empty());
    Ok(())
}
#[test]
fn filters_subtotal_and_sort() -> Result {
    let mut a = app()?;
    a.book
        .change(|b| b.tsv(0, 1, 1, "Nome\tValor\nB\t20\nA\t10\nC\t30"))?;
    a.command("ir A1:B4")?;
    a.command("ordenar cab A+")?;
    assert_eq!(a.book.val(0, 2, 2), "10");
    a.book.change(|b| {
        b.m.set_rows_hidden(0, 3, 3, true)?;
        b.set(0, 1, 5, "=SUBTOTAL(9,B2:B4)")?;
        b.set(0, 1, 6, "=SUBTOTAL(109,B2:B4)")
    })?;
    a.command("filtrar B >10")?;
    assert_eq!(a.book.val(0, 1, 5), "50");
    assert_eq!(a.book.val(0, 1, 6), "30");
    a.command("remover-filtro")?;
    assert!(a.book.ws(0).is_row_hidden(3)?);
    assert!(!a.book.ws(0).is_row_hidden(2)?);
    Ok(())
}
fn key(k: K) -> Event {
    Event::Key(KeyEvent::from(k))
}
fn mouse(kind: Kind, x: u16, y: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: M::NONE,
    })
}
#[test]
fn tui_inputs_and_resize() -> Result {
    let mut a = app()?;
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    a.event(Event::Paste("10\t=SUM(A1:A2)\n20\t".into()));
    assert_eq!(a.book.val(0, 1, 2), "30");
    a.event(Event::Key(KeyEvent::new(K::Char('z'), M::CONTROL)));
    assert_eq!(a.book.raw(0, 1, 2), "");
    a.event(key(K::F(2)));
    a.event(key(K::Char('7')));
    a.event(key(K::Enter));
    assert_eq!(a.book.val(0, 1, 1), "7");
    a.command("ir C3")?;
    a.command("congelar")?;
    term.draw(|f| a.draw(f)).unwrap();
    a.event(mouse(Kind::ScrollDown, 14, 12));
    term.draw(|f| a.draw(f)).unwrap();
    let x = a.cols[2].start;
    let y = a.rows[3].start;
    let r = a.rows[3].index;
    let c = a.cols[2].index;
    a.event(mouse(Kind::Down(Btn::Left), x, y));
    assert_eq!((a.v.r, a.v.c), (r, c));
    let end = a.rows[5].index;
    a.event(mouse(Kind::Drag(Btn::Left), x, a.rows[5].start));
    assert_eq!(a.selection().end, end);
    let mut larger = Terminal::new(TestBackend::new(120, 36)).unwrap();
    larger.draw(|f| a.draw(f)).unwrap();
    assert_eq!(a.selection().end, end);
    a.command("nova-aba")?;
    larger.draw(|f| a.draw(f)).unwrap();
    a.event(mouse(Kind::Down(Btn::Left), 2, 33));
    assert_eq!(a.sheet, 0);
    a.event(key(K::Char('=')));
    a.event(mouse(
        Kind::Down(Btn::Left),
        a.cols[0].start,
        a.rows[0].start,
    ));
    assert!(a.editor.text.contains("A1"));
    a.event(key(K::Esc));
    assert!(matches!(a.mode, Mode::Nav));
    Ok(())
}
#[test]
fn external_and_readonly() -> Result {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("protegido.xlsx");
    let mut a = app()?;
    ironcalc::export::save_to_xlsx(a.book.m.get_model(), p.to_str().unwrap()).unwrap();
    let old = std::fs::read(&p).unwrap();
    a.book = storage::open(&p, "pt-BR")?;
    assert!(!a.book.copy_only);
    a.book.change(|b| b.set(0, 1, 1, "42"))?;
    a.event(Event::Key(KeyEvent::new(K::Char('s'), M::CONTROL)));
    assert!(matches!(a.mode, Mode::Nav));
    assert_eq!(storage::open(&p, "pt-BR")?.val(0, 1, 1), "42");
    assert_eq!(std::fs::read(p.with_extension("xlsx.bak")).unwrap(), old);
    let copy = tmp.path().join("copia.xlsx");
    storage::save(&mut a.book, &copy, false)?;
    std::fs::write(&copy, &old).unwrap();
    assert!(storage::save(&mut a.book, &copy, false).is_err());
    assert_eq!(std::fs::read(&copy).unwrap(), old);
    a.book.readonly = true;
    assert!(a.command("nova-aba").is_err());
    assert!(a.command("salvar").is_err());
    Ok(())
}
#[test]
fn typed_roundtrip_and_failure() -> Result {
    let mut b = Book::new("pt-BR")?;
    b.change(|b| {
        for (i, v) in [
            "00123",
            "'0",
            "0",
            "'",
            "=\"\"",
            "2026-11-19",
            "12,5%",
            "東京 e\u{301}",
            "'=literal",
            "=IF(FALSE,1/0,42)",
            "=SOMA(1;2)",
            "=NAOFUNCIONA(7)",
        ]
        .iter()
        .enumerate()
        {
            b.set(0, 1, i as i32 + 1, v)?
        }
        Ok(())
    })?;
    assert_eq!(b.number(0, 1, 6), Some(46345.));
    assert_eq!(b.number(0, 1, 7), Some(0.125));
    assert_eq!(b.val(0, 1, 10), "42");
    assert_eq!(b.val(0, 1, 11), "3");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tipos.xlsx");
    storage::save(&mut b, &path, false)?;
    let mut b = storage::open(&path, "pt-BR")?;
    assert_eq!(b.val(0, 1, 1), "00123");
    assert_eq!(b.number(0, 1, 2), None);
    assert_eq!(b.number(0, 1, 3), Some(0.));
    assert_eq!(b.raw(0, 1, 4), "'");
    assert_eq!(b.raw(0, 1, 5), "=\"\"");
    assert_eq!(b.val(0, 1, 8), "東京 e\u{301}");
    assert_eq!(b.val(0, 1, 9), "=literal");
    assert!(
        b.raw(0, 1, 12).to_uppercase().contains("NAOFUNCIONA"),
        "raw {} display {}",
        b.raw(0, 1, 12),
        b.val(0, 1, 12)
    );
    assert!(b.val(0, 1, 12).starts_with('#'));
    let before = std::fs::read(&path).unwrap();
    std::fs::create_dir(path.with_extension("xlsx.bak")).unwrap();
    b.change(|b| b.set(0, 1, 1, "alterada"))?;
    assert!(storage::save(&mut b, &path, true).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert!(b.dirty());
    Ok(())
}
#[test]
fn table_commands_and_structural_undo() -> Result {
    let mut a = app()?;
    a.book
        .change(|b| b.tsv(0, 1, 1, "Item\tN\tP\tTotal\nZ\t2\t7\t\nA\t3\t5\t"))?;
    for cmd in [
        "ir A1:D3",
        "tabela Vendas",
        "ir D2",
        "coluna-calculada =[@N]*[@P]",
        "ir F1",
        "preencher =SUM(Vendas[Total])",
        "ir A1:D3",
        "ordenar cab A+",
    ] {
        a.command(cmd)?
    }
    assert_eq!(a.book.val(0, 2, 1), "A");
    assert_eq!(a.book.val(0, 2, 4), "15");
    assert_eq!(a.book.val(0, 1, 6), "29");
    for cmd in [
        "ir B2",
        "renomear-coluna Quantidade",
        "renomear-tabela tbLoja",
        "ir A3",
        "inserir-linhas",
    ] {
        a.command(cmd)?
    }
    assert_eq!(a.book.extra.tables[0].area.address(), "A1:D4");
    assert_eq!(a.book.val(0, 1, 6), "29");
    assert_eq!(a.book.raw(0, 3, 4), "=[@Quantidade]*[@P]");
    a.command("duplicar-aba")?;
    assert_eq!(a.book.extra.tables.len(), 2);
    assert_eq!(a.book.val(a.sheet, 1, 6), "29");
    a.command("excluir-aba")?;
    assert_eq!(a.book.extra.tables.len(), 1);
    a.command("desfazer")?;
    assert_eq!(a.book.extra.tables.len(), 2);
    Ok(())
}

#[test]
fn note_and_cross_sheet_move() -> Result {
    let mut a = app()?;
    a.book.change(|b| {
        b.tsv(0, 1, 1, "10\t=A1*2\t=B1")?;
        b.m.new_sheet()
    })?;
    let clip = a.book.clipboard(0, Range::cell(1, 2), true)?;
    a.book.change(|b| b.paste(&clip, 1, 2, 3, "normal"))?;
    assert_eq!(a.book.val(1, 2, 3), "20");
    assert_eq!(a.book.val(0, 1, 3), "20");
    a.command("nota Revisar preço de café")?;
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("nota.xlsx");
    storage::save(&mut a.book, &p, false)?;
    a.book = storage::open(&p, "en")?;
    a.command("ler-nota")?;
    assert_eq!(a.status, "Revisar preço de café");
    Ok(())
}

#[test]
fn notes_follow_records_and_preserve_authorship() -> Result {
    let mut a = app()?;
    a.book.change(|b| b.tsv(0, 1, 1, "Produto\nZ\nA"))?;
    a.command("ir A2")?;
    a.command("nota Conferido")?;
    a.book.m.notes_mut(0)?[0].author_name = "Lúcia & Cia.".into();
    a.command("ir A1:A3")?;
    a.command("ordenar cab A+")?;
    assert_eq!(a.book.ws(0).comments[0].cell_ref, "A3");
    a.command("ir A2")?;
    a.command("inserir-linhas")?;
    assert_eq!(a.book.ws(0).comments[0].cell_ref, "A4");
    let clip = a.book.clipboard(0, Range::cell(4, 1), true)?;
    a.book.change(|b| b.paste(&clip, 0, 2, 3, "normal"))?;
    assert_eq!(a.book.ws(0).comments.len(), 1);
    assert_eq!(a.book.ws(0).comments[0].cell_ref, "C2");
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("notas.xlsx");
    storage::save(&mut a.book, &p, false)?;
    let b = storage::open(&p, "en")?;
    assert_eq!(b.ws(0).comments[0].author_name, "Lúcia & Cia.");
    assert_eq!(b.ws(0).comments[0].text, "Conferido");
    Ok(())
}

#[test]
fn table_filter_follows_edits_and_keeps_totals_visible() -> Result {
    let mut a = app()?;
    a.book.change(|b| b.tsv(0, 1, 1, "Item\tN\na\t10\nb\t15"))?;
    for cmd in [
        "ir A1:B3",
        "tabela tb",
        "ir B2",
        "totais",
        "ir A1:B4",
        "filtrar B >25",
    ] {
        a.command(cmd)?;
    }
    assert!(a.book.ws(0).is_row_hidden(2)?);
    assert!(!a.book.ws(0).is_row_hidden(4)?);
    a.book.change(|b| b.set(0, 2, 2, "30"))?;
    assert!(!a.book.ws(0).is_row_hidden(2)?);
    assert_eq!(a.book.val(0, 4, 2), "30");
    a.command("desfazer")?;
    assert!(a.book.ws(0).is_row_hidden(2)?);
    assert_eq!(a.book.val(0, 4, 2), "0");
    Ok(())
}

#[test]
fn structured_and_sheet_references_share_a_formula() -> Result {
    let mut a = app()?;
    a.book.change(|b| {
        b.tsv(0, 1, 1, "Preço\n10")?;
        b.create_table(0, parse_range("A1:A2")?, "Itens")?;
        b.m.new_sheet()?;
        b.rename_tab(1, "Ajuste")?;
        b.set(1, 1, 1, "5")?;
        b.set(0, 1, 3, "=SUM(Itens[Preço])+Ajuste!A1")
    })?;
    a.book.change(|b| b.rename_tab(1, "Ajuste d'água"))?;
    assert_eq!(a.book.val(0, 1, 3), "15");
    assert!(a.book.raw(0, 1, 3).contains("'Ajuste d''água'!A1"));
    a.book.change(|b| b.delete_tab(1))?;
    assert!(a.book.val(0, 1, 3).contains("REF"));
    a.book.history(false)?;
    assert_eq!(a.book.val(0, 1, 3), "15");
    Ok(())
}

#[test]
fn mixed_table_dependencies_follow_a_cut_between_sheets() -> Result {
    let mut b = Book::new("en")?;
    b.change(|b| {
        b.tsv(0, 1, 1, "Qtd\n5")?;
        b.create_table(0, parse_range("A1:A2")?, "Itens")?;
        b.set(0, 1, 3, "10")?;
        b.set(0, 1, 4, "=SUM(Itens[Qtd])+C1")?;
        b.set(0, 2, 4, "=SUM(Itens[Qtd])+D1")?;
        b.m.new_sheet()
    })?;
    let p = b.clipboard(0, parse_range("C1:D1")?, true)?;
    b.change(|b| b.paste(&p, 1, 2, 2, "normal"))?;
    assert_eq!(b.val(1, 2, 3), "15");
    assert_eq!(b.val(0, 2, 4), "20");
    b.change(|b| b.set(1, 2, 2, "20"))?;
    assert_eq!(b.val(0, 2, 4), "30");
    assert!(b.raw(1, 2, 3).contains("Itens[Qtd]"));
    Ok(())
}

#[test]
fn converting_one_table_preserves_other_table_operands() -> Result {
    let mut b = Book::new("en")?;
    b.change(|b| {
        b.tsv(0, 1, 1, "Valor\n5")?;
        b.tsv(0, 1, 3, "Valor\n12")?;
        b.create_table(0, parse_range("A1:A2")?, "Primeira")?;
        b.create_table(0, parse_range("C1:C2")?, "Segunda")?;
        b.set(0, 1, 6, "=SUM(primeira[Valor])+SUM(Segunda[Valor])")
    })?;
    b.change(|b| b.convert_table(0, 1, 1))?;
    assert!(b.raw(0, 1, 6).contains("Segunda[Valor]"));
    assert!(!b.raw(0, 1, 6).to_lowercase().contains("primeira["));
    b.change(|b| {
        b.extend_table(0, 2, 3, false)?;
        b.set(0, 3, 3, "21")
    })?;
    assert_eq!(b.val(0, 1, 6), "38");
    Ok(())
}

#[test]
fn pick_a_reference_on_another_tab_and_return_to_origin() -> Result {
    let mut a = app()?;
    a.command("nova-aba")?;
    a.command("renomear-aba Preço d'água")?;
    a.command("ir B1")?;
    a.command("preencher 12")?;
    a.command("ir A1")?;
    a.command("aba-anterior")?;
    a.command("ir E3")?;
    a.event(key(K::Char('=')));
    a.event(key(K::F(6)));
    a.event(key(K::Right));
    assert_eq!(a.editor.text, "='Preço d''água'!B1");
    a.event(key(K::Enter));
    assert_eq!(a.sheet, 0);
    assert_eq!(a.book.val(0, 3, 5), "12");
    assert_eq!((a.v.r, a.v.c), (4, 5));
    a.event(key(K::Char('=')));
    a.event(key(K::F(6)));
    a.event(key(K::Esc));
    assert_eq!(a.sheet, 0);
    assert_eq!((a.v.r, a.v.c), (4, 5));
    Ok(())
}

#[test]
fn text_interchange_keeps_blank_rows_unicode_and_undo() -> Result {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("vendas local.csv");
    std::fs::write(
        &path,
        b"Item;Valor\r\nCaf\xe9;12,5\r\n\r\n\"x\r\n\r\ny\";7\r\n",
    )
    .unwrap();
    let mut app = App::new(Book::new("pt-BR")?, "pt-BR");
    app.command(&format!(
        "importar ponto-e-virgula latin1 {}",
        path.display()
    ))?;
    assert_eq!(app.book.val(0, 2, 1), "Café");
    assert_eq!(app.book.number(0, 2, 2), Some(12.5));
    assert_eq!(app.book.val(0, 3, 1), "");
    assert!(app.book.val(0, 4, 1).contains("y"));
    app.command("ir A1:B4")?;
    let output = tmp.path().join("seleção.tsv");
    app.command(&format!("exportar seleção tab utf-8 {}", output.display()))?;
    let text = std::fs::read_to_string(&output).unwrap();
    assert!(text.contains("Café\t12,5"));
    assert!(text.contains("\n\t\n"));
    assert!(app
        .command(&format!("exportar aba tab latin1 {}", output.display()))
        .is_err());
    app.command("desfazer")?;
    assert!(app.book.raw(0, 2, 1).is_empty());
    Ok(())
}

#[test]
fn drag_selection_keeps_scrolling_until_mouse_release() -> Result {
    let mut a = app()?;
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| a.draw(f)).unwrap();
    let x = a.cols[1].start;
    let y = a.rows[1].start;
    let bottom = a.rows.last().unwrap().start;
    a.event(mouse(Kind::Down(Btn::Left), x, y));
    a.event(mouse(Kind::Drag(Btn::Left), x, bottom));
    let before = a.selection();
    assert!(a.advance_drag()?);
    term.draw(|f| a.draw(f)).unwrap();
    assert_eq!(a.selection().r, before.r);
    assert!(a.selection().end > before.end);
    a.event(mouse(Kind::Up(Btn::Left), x, bottom));
    assert!(!a.advance_drag()?);
    Ok(())
}

#[test]
fn default_grid_colors_and_explicit_fill() -> Result {
    use ratatui::style::Color;
    let mut a = app()?;
    a.book.change(|b| b.tsv(0, 1, 2, "Texto\tCor"))?;
    a.command("ir C1")?;
    a.command("fundo #315877")?;
    a.command("ir A1")?;
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| a.draw(f)).unwrap();
    let y = a.rows[0].start;
    let buffer = term.backend().buffer();
    let plain = &buffer[(a.cols[1].start + 1, y)];
    assert_eq!(plain.bg, Color::White);
    assert_eq!(plain.fg, Color::Black);
    assert_eq!(buffer[(a.cols[2].start + 1, y)].bg, Color::Rgb(49, 88, 119));
    Ok(())
}

#[test]
fn dark_theme_adapts_workbook_colors_and_restores_them() -> Result {
    use ratatui::style::Color;
    let mut a = app()?;
    a.book.change(|b| b.set(0, 1, 2, "Texto"))?;
    for cmd in ["ir B1", "cor #000000", "fundo #FFFF00", "ir A1", "tema"] {
        a.command(cmd)?;
    }
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| a.draw(f)).unwrap();
    let pos = (a.cols[1].start + 1, a.rows[0].start);
    let cell = &term.backend().buffer()[pos];
    assert_eq!(cell.bg, Color::Rgb(85, 85, 0));
    assert!(matches!(cell.fg,Color::Rgb(r,g,b) if r>=184 && g>=184 && b>=184));
    a.command("tema")?;
    term.draw(|f| a.draw(f)).unwrap();
    let cell = &term.backend().buffer()[pos];
    assert_eq!(
        (cell.fg, cell.bg),
        (Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 0))
    );
    Ok(())
}

#[test]
fn colon_completion_selects_edits_and_never_runs_on_accept() -> Result {
    let mut a = app()?;
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let type_text = |a: &mut App, s: &str| {
        for ch in s.chars() {
            a.event(key(K::Char(ch)))
        }
    };
    type_text(&mut a, ":sai");
    a.event(key(K::Tab));
    assert_eq!(a.editor.text, "sair");
    assert!(!a.quit);
    a.event(key(K::Esc));

    type_text(&mut a, ":cop");
    a.event(key(K::Tab));
    term.draw(|f| a.draw(f)).unwrap();
    let line = |term: &Terminal<TestBackend>, y| {
        (0..80)
            .map(|x| term.backend().buffer()[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    };
    assert_eq!(
        line(&term, 20),
        " ▸ copia           Salva uma cópia em outro arquivo"
    );
    assert_eq!(
        line(&term, 21),
        "   copiar          Copia a seleção para colar no clisheet"
    );
    assert_eq!(
        line(&term, 22),
        "   copiar-externo  Copia a seleção para o clipboard externo"
    );
    assert_eq!(line(&term, 23), ": cop");
    a.event(key(K::Down));
    a.event(key(K::Enter));
    assert_eq!(a.editor.text, "copiar");
    term.draw(|f| a.draw(f)).unwrap();
    assert_eq!(line(&term, 23), ": copiar");
    assert!(line(&term, 22).contains("Cont"));
    assert!(matches!(a.mode, Mode::Prompt));
    a.event(key(K::Esc));

    type_text(&mut a, ":formato perc");
    a.event(key(K::Tab));
    assert_eq!(a.editor.text, "formato percentual");
    a.event(key(K::Esc));

    type_text(&mut a, ":");
    a.event(key(K::Tab));
    a.event(key(K::BackTab));
    a.event(key(K::Esc));
    assert!(matches!(a.mode, Mode::Prompt));
    assert_eq!(a.editor.text, "");
    type_text(&mut a, "irx B2");
    a.event(key(K::Home));
    a.event(key(K::Right));
    a.event(key(K::Right));
    a.event(key(K::Tab));
    assert_eq!(a.editor.text, "ir B2");
    Ok(())
}
