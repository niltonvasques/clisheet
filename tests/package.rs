use clisheet::{
    book::{Book, Result},
    storage,
};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

fn replace_part(file: &Path, key: &str, edit: impl Fn(&str) -> String) {
    let mut source = ZipArchive::new(Cursor::new(fs::read(file).unwrap())).unwrap();
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..source.len() {
        let mut f = source.by_index(i).unwrap();
        if f.is_dir() {
            continue;
        }
        output
            .start_file(f.name(), SimpleFileOptions::default())
            .unwrap();
        if f.name() == key {
            let mut xml = String::new();
            f.read_to_string(&mut xml).unwrap();
            output.write_all(edit(&xml).as_bytes()).unwrap();
        } else {
            std::io::copy(&mut f, &mut output).unwrap();
        }
    }
    fs::write(file, output.finish().unwrap().into_inner()).unwrap();
}
#[test]
fn shared_formulas_are_calculated_and_reexported() -> Result {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("externo.xlsx");
    let dest = d.path().join("copia.xlsx");
    let mut b = Book::new("en")?;
    b.change(|b| b.tsv(0, 1, 1, "10\t=A1*2\n15\t=A2*2"))?;
    storage::save(&mut b, &p, false)?;
    replace_part(&p, "xl/worksheets/sheet1.xml", |s| {
        assert!(s.contains("<f>A1*2</f>") && s.contains("<f>A2*2</f>"));
        s.replace(
            "<f>A1*2</f>",
            r#"<f t="shared" ref="B1:B2" si="0">A1*2</f>"#,
        )
        .replace("<f>A2*2</f>", r#"<f t="shared" si="0"/>"#)
    });
    let mut b = storage::open(&p, "en")?;
    assert!(b.copy_only);
    assert_eq!(b.raw(0, 2, 2), "=A2*2");
    b.change(|b| b.set(0, 2, 1, "25"))?;
    storage::save(&mut b, &dest, false)?;
    let copy = storage::open(&dest, "en")?;
    assert_eq!(copy.val(0, 2, 2), "50");
    assert_eq!(copy.raw(0, 2, 2), "=A2*2");
    Ok(())
}
#[test]
fn date_epoch_and_chart_protection() -> Result {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("grafico.xlsx");
    fs::copy("examples/xlsx/incompativel.xlsx", &p).unwrap();
    let before = fs::read(&p).unwrap();
    let mut book = storage::open(&p, "pt-BR")?;
    book.change(|b| b.set(0, 1, 1, "Mês fiscal"))?;
    assert!(storage::save(&mut book, &p, true).is_err());
    assert_eq!(fs::read(&p).unwrap(), before);
    let copy = tmp.path().join("copia.xlsx");
    storage::save(&mut book, &copy, false)?;
    replace_part(&copy, "xl/workbook.xml", |s| {
        s.replacen("<sheets>", r#"<workbookPr date1904="1"/><sheets>"#, 1)
    });
    assert!(storage::open(&copy, "pt-BR")
        .err()
        .unwrap()
        .contains("1904"));
    Ok(())
}

#[test]
fn bundled_table_opens_in_both_locales() -> Result {
    let tmp = tempfile::tempdir().unwrap();
    for locale in ["en", "pt-BR"] {
        let mut b = storage::open(Path::new("examples/xlsx/vendas.xlsx"), locale)?;
        assert_eq!(b.number(0, 4, 4), Some(35.0));
        b.change(|b| b.set(0, 2, 2, "4"))?;
        let dest = tmp.path().join(format!("{locale}.xlsx"));
        storage::save(&mut b, &dest, false)?;
        let b = storage::open(&dest, if locale == "en" { "pt-BR" } else { "en" })?;
        assert_eq!(b.number(0, 4, 4), Some(55.0));
    }
    Ok(())
}

#[test]
#[cfg(unix)]
fn concurrent_saves_preserve_the_winner_and_file_mode() -> Result {
    use std::{os::unix::fs::PermissionsExt, sync::Barrier, thread};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pasta.xlsx");
    storage::save(&mut Book::new("pt-BR")?, &path, false)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let gate = Barrier::new(2);
    let outcomes = thread::scope(|scope| {
        let workers: Vec<_> = ["10", "20"]
            .into_iter()
            .map(|value| {
                let gate = &gate;
                let p = &path;
                scope.spawn(move || {
                    let mut b = storage::open(p, "pt-BR").unwrap();
                    b.change(|b| b.set(0, 1, 1, value)).unwrap();
                    gate.wait();
                    (value, storage::save(&mut b, p, true))
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        outcomes.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let winner = outcomes
        .iter()
        .find(|(_, result)| result.is_ok())
        .unwrap()
        .0;
    assert_eq!(storage::open(&path, "pt-BR")?.val(0, 1, 1), winner);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    Ok(())
}

#[test]
fn oversized_array_is_rejected_before_engine_import() -> Result {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("array.xlsx");
    let mut b = Book::new("en")?;
    b.change(|b| b.set(0, 1, 1, "=1+1"))?;
    storage::save(&mut b, &p, false)?;
    replace_part(&p, "xl/worksheets/sheet1.xml", |s| {
        s.replace("<f>1+1</f>", r#"<f t="array" ref="A1:A500001">1+1</f>"#)
    });
    assert!(storage::open(&p, "en").err().unwrap().contains("Matrizes"));
    Ok(())
}
