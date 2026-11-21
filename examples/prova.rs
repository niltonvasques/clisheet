//! Prova de cálculo, cópia e round-trip com o motor usado na interface.
use ironcalc::{
    base::{expressions::types::CellReferenceIndex, Model},
    export::save_to_xlsx,
    import::load_from_xlsx,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("entrada.xlsx");
    let output = dir.path().join("resultado.xlsx");
    let mut m = Model::new_empty("Resumo", "en", "UTC", "en")?;
    m.rename_sheet("Sheet1", "Resumo")?;
    m.add_sheet("Preços 2026")?;
    m.set_user_input(1, 2, 2, "10".into())?;
    m.set_user_input(0, 1, 1, "='Preços 2026'!B2*2".into())?;
    m.set_user_input(0, 1, 3, "=$A1+B$2".into())?;
    m.evaluate();
    save_to_xlsx(&m, input.to_str().unwrap())?;
    let mut m = load_from_xlsx(input.to_str().unwrap(), "en", "UTC", "en")?;
    m.evaluate();
    println!(
        "Aberto: Resumo!A1 = {}",
        m.get_formatted_cell_value(0, 1, 1)?
    );
    m.set_user_input(1, 2, 2, "15".into())?;
    let src = CellReferenceIndex {
        sheet: 0,
        row: 1,
        column: 3,
    };
    let dst = CellReferenceIndex { row: 2, ..src };
    let copy = m.extend_copied_value("=$A1+B$2", &src, &dst)?;
    assert_eq!(copy, "=$A2+B$2");
    m.set_user_input(0, 2, 3, copy)?;
    m.evaluate();
    save_to_xlsx(&m, output.to_str().unwrap())?;
    let mut m = load_from_xlsx(output.to_str().unwrap(), "en", "UTC", "en")?;
    m.evaluate();
    assert_eq!(m.get_formatted_cell_value(0, 1, 1)?, "30");
    println!(
        "Reaberto: Resumo!A1 = 30, C2 = {}",
        m.get_localized_cell_content(0, 2, 3)?
    );
    Ok(())
}
