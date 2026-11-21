use clisheet::{book::*, storage};
#[test]
fn calculation_history_and_savepoint() -> Result {
    let mut b = Book::new("en")?;
    b.change(|b| b.tsv(0, 1, 1, "10\t=SUM(A1:A2)\n20\t"))?;
    assert_eq!(b.val(0, 1, 2), "30");
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("orçamento.xlsx");
    storage::save(&mut b, &p, false)?;
    b.change(|b| b.set(0, 1, 1, "15"))?;
    assert_eq!(b.val(0, 1, 2), "35");
    b.history(false)?;
    assert_eq!(b.val(0, 1, 2), "30");
    assert!(!b.dirty());
    b.history(true)?;
    storage::save(&mut b, &p, true)?;
    let b = storage::open(&p, "en")?;
    assert_eq!(b.val(0, 1, 2), "35");
    assert_eq!(b.raw(0, 1, 2), "=SUM(A1:A2)");
    Ok(())
}
#[test]
fn copy_cut() -> Result {
    let mut b = Book::new("en")?;
    b.change(|b| b.tsv(0, 1, 1, "8\t=A1\t=$A1+B$2"))?;
    let p = b.clipboard(0, Range::cell(1, 3), false)?;
    b.change(|b| b.paste(&p, 0, 2, 3, "normal"))?;
    assert_eq!(b.raw(0, 2, 3), "=$A2+B$2");
    b.change(|b| b.paste(&p, 0, 1, 4, "normal"))?;
    assert_eq!(b.raw(0, 1, 4), "=$A1+C$2");
    let p = b.clipboard(0, Range::cell(1, 1), true)?;
    assert_eq!(b.val(0, 1, 1), "8");
    b.change(|b| b.paste(&p, 0, 2, 1, "normal"))?;
    assert_eq!(b.raw(0, 1, 2), "=A2");
    assert_eq!(b.val(0, 1, 2), "8");
    Ok(())
}
#[test]
fn tables() -> Result {
    let mut b = Book::new("en")?;
    b.change(|b| {
        b.tsv(0, 1, 1, "Quantidade\tPreco\tTotal\n2\t10\t\n3\t5\t")?;
        b.create_table(0, parse_range("A1:C3")?, "tbVendas")?;
        b.table_formula(0, 2, 3, "=[@Quantidade]*[@Preco]")?;
        b.set(0, 1, 5, "=SUM(tbVendas[Total])")
    })?;
    assert_eq!(b.val(0, 1, 5), "35");
    b.change(|b| b.extend_table(0, 1, 1, true))?;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vendas.xlsx");
    storage::save(&mut b, &path, false)?;
    let b = storage::open(&path, "en")?;
    assert_eq!(b.extra.tables[0].area.address(), "A1:C4");
    assert_eq!(b.raw(0, 2, 3), "=[@Quantidade]*[@Preco]");
    assert_eq!(b.val(0, 4, 3), "35");
    Ok(())
}
#[test]
fn brazil() -> Result {
    let mut b = Book::new("pt-BR")?;
    b.change(|b| {
        b.set(0, 1, 1, "12,50")?;
        b.set(0, 1, 2, "=IF(FALSE;1/0;42)")
    })?;
    assert_eq!(b.number(0, 1, 1), Some(12.5));
    assert_eq!(b.val(0, 1, 2), "42");
    Ok(())
}

#[test]
fn excel_1900_date_boundary() -> Result {
    let mut b = Book::new("en")?;
    for (col, (formula, expected)) in [
        ("=DATE(1900,1,1)+0", 1.),
        ("=DATE(1900,2,28)+0", 59.),
        ("=DATE(1900,2,29)+0", 60.),
        ("=DATE(1900,3,1)+0", 61.),
        ("=DAY(60)", 29.),
        ("=MONTH(60)", 2.),
        ("=DAY(1)", 1.),
        ("=WEEKDAY(1)", 1.),
    ]
    .iter()
    .enumerate()
    {
        b.change(|b| b.set(0, 1, 1 + col as i32, formula))?;
        assert_eq!(b.number(0, 1, 1 + col as i32), Some(*expected), "{formula}");
    }
    b.change(|b| b.set(0, 2, 1, "=TEXT(60,\"yyyy-mm-dd\")"))?;
    assert_eq!(b.val(0, 2, 1), "1900-02-29");
    Ok(())
}

#[test]
fn common_financial_functions_use_numeric_cash_flows() -> Result {
    let mut b = Book::new("en")?;
    b.change(|b| b.tsv(0, 1, 1, "-100\n110"))?;
    for (r, (formula, expected)) in [
        ("=PMT(0,10,100)", -10.),
        ("=PV(0,4,-25)", 100.),
        ("=FV(0,4,-25)", 100.),
        ("=NPV(10%,110,121)", 200.),
        ("=IRR(A1:A2)", 0.1),
    ]
    .into_iter()
    .enumerate()
    {
        b.change(|b| b.set(0, r as i32 + 1, 3, formula))?;
        let value = b
            .number(0, r as i32 + 1, 3)
            .ok_or_else(|| b.val(0, r as i32 + 1, 3))?;
        assert!((value - expected).abs() < 1e-7, "{formula}: {value}");
    }
    Ok(())
}
