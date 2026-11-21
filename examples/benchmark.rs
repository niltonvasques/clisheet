use clisheet::{
    book::{Book, Result},
    storage,
    ui::App,
};
use ratatui::{backend::TestBackend, Terminal};
use std::time::Instant;
fn measure<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let v = f();
    println!("{name}: {:.2} ms", start.elapsed().as_secs_f64() * 1000.);
    v
}
fn main() -> Result {
    for sparse in [false, true] {
        let mut b = Book::new("en")?;
        measure(
            if sparse {
                "Construção esparsa"
            } else {
                "Construção 100000 números + 10000 fórmulas"
            },
            || {
                b.change(|b| {
                    if sparse {
                        b.set(0, 1, 1, "10")?;
                        b.set(0, 1048576, 16384, "=A1*2")?
                    } else {
                        for r in 1..=10000 {
                            for c in 1..=10 {
                                b.m.set_user_input(0, r, c, "12")?
                            }
                            b.m.set_user_input(0, r, 11, &format!("=A{r}+J{r}"))?
                        }
                    }
                    Ok(())
                })
            },
        )?;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("bench.xlsx");
        measure("Gravação validada", || {
            storage::save(&mut b, &path, false)
        })?;
        drop(b);
        let mut b = measure("Abertura e cálculo", || storage::open(&path, "en"))?;
        measure("Edição transacional", || {
            b.change(|b| b.set(0, 1, 1, "42"))
        })?;
        measure("Recálculo completo", || b.finish());
        let mut app = App::new(b, "en");
        let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let mut times = vec![];
        for i in 0..201 {
            let tick = Instant::now();
            app.focus(
                if sparse && i % 2 == 0 { 1048576 } else { 1 + i },
                if sparse && i % 2 == 0 { 16384 } else { 1 },
                false,
            );
            term.draw(|f| app.draw(f)).unwrap();
            if i > 0 {
                times.push(tick.elapsed().as_secs_f64() * 1000.)
            }
        }
        times.sort_by(f64::total_cmp);
        println!(
            "Navegação e desenho 100×30 p50 {:.2} ms p95 {:.2} ms",
            times[100], times[190]
        );
    }
    if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
        if let Some(line) = s.lines().find(|l| l.starts_with("VmHWM")) {
            println!("{line}")
        }
    }
    Ok(())
}
