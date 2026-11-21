//! Gera as três pastas editáveis em um diretório vazio.
use clisheet::{book::*, storage};
use std::path::PathBuf;
fn main() -> Result {
    let dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .unwrap_or_else(|| "examples/xlsx".into()),
    );
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut b = Book::new("pt-BR")?;
    b.change(|b|{b.m.rename_sheet(0,"Orçamento")?;b.tsv(0,1,1,"Categoria\tPrevisto\tRealizado\tSaldo\nMercado\t500\t420,50\t=B2-C2\nTransporte\t200\t190\t=B3-C3\nTotal\t=SUM(B2:B3)\t=SUM(C2:C3)\t=B4-C4")?;
 b.m.update_range_style(&parse_range("B2:D4")?.area(0),"num_fmt","\"R$\" #,##0.00")?;b.m.update_range_style(&parse_range("A1:D1")?.area(0),"font.b","true")?;b.m.set_columns_width(0,1,4,105.0)})?;
    storage::save(&mut b, &dir.join("orcamento.xlsx"), false)?;
    let mut b = Book::new("pt-BR")?;
    b.change(|b| {
        b.m.rename_sheet(0, "Vendas")?;
        b.tsv(
            0,
            1,
            1,
            "Produto\tQuantidade\tPreco\tTotal\nCafé\t2\t10\t\nChá\t3\t5\t",
        )?;
        b.create_table(0, parse_range("A1:D3")?, "tbVendas")?;
        b.table_formula(0, 2, 4, "=[@Quantidade]*[@Preco]")?;
        b.set(0, 1, 6, "=SUM(tbVendas[Total])")
    })?;
    b.change(|b| b.extend_table(0, 1, 1, true))?;
    storage::save(&mut b, &dir.join("vendas.xlsx"), false)?;
    let mut b = Book::new("pt-BR")?;
    b.change(|b| {
        b.m.rename_sheet(0, "Resumo")?;
        b.m.new_sheet()?;
        b.m.rename_sheet(1, "Preços 2026")?;
        b.tsv(1, 1, 1, "Item\tPreço\nCafé\t10")?;
        b.set(0, 1, 1, "='Preços 2026'!B2*2")?;
        b.set(0, 1, 3, "=$A1+B$2")
    })?;
    storage::save(&mut b, &dir.join("resumo.xlsx"), false)
}
