use clap::{Parser, ValueEnum};
use clisheet::{
    book::{Book, Result},
    storage,
    ui::App,
};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Paragraph,
    Terminal,
};
use std::{
    collections::HashMap,
    io::{self, IsTerminal},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
#[derive(Parser)]
#[command(
    name = "clisheet",
    version,
    about = "Edite e recalcule planilhas XLSX no terminal",
    after_help = "F1 abre a ajuda. F10 mostra as ações. Ctrl+Q sai. Use : para digitar comandos."
)]
struct Args {
    /// Pasta de trabalho XLSX. Sem caminho, cria uma pasta em branco.
    #[arg(value_name = "ARQUIVO.xlsx", conflicts_with = "recover")]
    file: Option<PathBuf>,
    /// Abre a planilha em modo somente leitura.
    #[arg(long)]
    read_only: bool,
    /// Seleciona a aba pelo nome.
    #[arg(long, value_name = "NOME")]
    sheet: Option<String>,
    /// Desativa a captura de mouse.
    #[arg(long)]
    no_mouse: bool,
    /// Envia cópias ao clipboard externo (desktop local ou OSC 52 no terminal).
    #[arg(long)]
    osc52: bool,
    /// Localidade de números, datas e fórmulas.
    #[arg(long,default_value="pt-BR",value_parser=["pt-BR","en-US"])]
    locale: String,
    /// Tema da grade. Prevalece sobre config.json (padrão: light).
    #[arg(long, value_enum)]
    theme: Option<Theme>,
    /// Recupera a sessão mais recente em uma nova pasta de trabalho.
    #[arg(long)]
    recover: bool,
    /// Dispensa o backup .xlsx.bak ao salvar no caminho de origem.
    #[arg(long)]
    no_backup: bool,
}
#[derive(Clone, ValueEnum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Theme {
    Dark,
    Light,
}
#[derive(Default, serde::Deserialize)]
struct Config {
    theme: Option<Theme>,
}

fn startup_theme(directory: &std::path::Path, override_theme: Option<Theme>) -> Result<Theme> {
    let path = directory.join("config.json");
    let config: Config = match std::fs::read(&path) {
        Ok(data) => {
            serde_json::from_slice(&data).map_err(|e| format!("{}: {e}", path.display()))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    Ok(override_theme.or(config.theme).unwrap_or(Theme::Light))
}
struct Screen;
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        );
        let _ = disable_raw_mode();
    }
}
fn ioerr(e: impl std::fmt::Display) -> String {
    e.to_string()
}
/// Run a document operation on a worker. The terminal keeps its last coherent
/// frame until the new revision is ready. Input remains queued by Crossterm.
fn with_progress<T: Send>(
    term: &mut Terminal<CrosstermBackend<io::Stdout>>,
    back: &Buffer,
    label: &str,
    job: impl FnOnce() -> T + Send,
) -> Result<T> {
    std::thread::scope(|scope| {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        scope.spawn(move || {
            let _ = tx.send(job());
        });
        let mut n = 0;
        loop {
            match rx.recv_timeout(Duration::from_millis(if n == 0 { 45 } else { 120 })) {
                Ok(v) => return Ok(v),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Operação interrompida".into())
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    term.draw(|f| {
                        let area = f.area();
                        if area == back.area {
                            f.buffer_mut().clone_from(back);
                        }
                        let text = format!(" {} {} • aguarde", ["◐", "◓", "◑", "◒"][n % 4], label);
                        f.render_widget(
                            Paragraph::new(text)
                                .style(Style::default().add_modifier(Modifier::REVERSED)),
                            Rect::new(0, area.height.saturating_sub(1), area.width, 1),
                        );
                    })
                    .map_err(ioerr)?;
                    n += 1;
                }
            }
        }
    })
}
fn run(args: Args) -> Result {
    let mut book = if let Some(ref path) = args.file {
        storage::open(path, &args.locale)?
    } else if args.recover {
        let p = storage::latest_recovery()?;
        let mut b = storage::open(&p, &args.locale)?;
        b.path = None;
        b.digest = None;
        b.saved = u64::MAX;
        b
    } else {
        Book::new(&args.locale)?
    };
    book.readonly = args.read_only;
    let mut app = App::new(book, &args.locale);
    if args.recover {
        let p = storage::latest_recovery()?;
        let t = std::fs::metadata(p)
            .and_then(|m| m.modified())
            .map_err(ioerr)?;
        app.status = format!(
            "Recuperado de {} • salve uma cópia",
            chrono::DateTime::<chrono::Local>::from(t).format("%d/%m/%Y %H:%M:%S")
        )
    }
    app.osc52 = args.osc52;
    app.backup = !args.no_backup;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("clisheet");
    app.theme = matches!(startup_theme(&config, args.theme)?, Theme::Light);
    let config = config.join("keys.json");
    if config.exists() {
        app.bindings = serde_json::from_slice::<HashMap<String, String>>(
            &std::fs::read(config).map_err(ioerr)?,
        )
        .map_err(|e| format!("keys.json: {e}"))?
    }
    if let Some(name) = args.sheet {
        let id = app
            .book
            .m
            .get_worksheets_properties()
            .iter()
            .position(|w| w.name == name)
            .ok_or("Aba desconhecida")?;
        app.switch(id as u32)?
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("A interface requer um terminal interativo (por SSH, use ssh -t)".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    for sig in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGINT,
    ] {
        signal_hook::flag::register(sig, stop.clone()).map_err(ioerr)?;
    }
    enable_raw_mode().map_err(ioerr)?;
    let _guard = Screen;
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste).map_err(ioerr)?;
    if !args.no_mouse {
        execute!(io::stdout(), EnableMouseCapture).map_err(ioerr)?
    }
    let mut term = Terminal::new(CrosstermBackend::new(io::stdout())).map_err(ioerr)?;
    let mut last = Instant::now();
    let mut redraw = true;
    let mut previous_frame = Buffer::empty(Rect::default());
    while !app.quit && !stop.load(Ordering::Relaxed) {
        if redraw {
            previous_frame = term.draw(|f| app.draw(f)).map_err(ioerr)?.buffer.clone();
            redraw = false
        }
        if event::poll(Duration::from_millis(150)).map_err(ioerr)? {
            let e = event::read().map_err(ioerr)?;
            let before = app.book.version;
            let previous = app.journal.clone();
            let no_motion =
                !matches!(&e,event::Event::Mouse(m)if m.kind==event::MouseEventKind::Moved);
            if no_motion {
                let heavy = matches!(
                    &e,
                    event::Event::Paste(_)
                        | event::Event::Key(event::KeyEvent {
                            code: event::KeyCode::Enter | event::KeyCode::Delete,
                            ..
                        })
                ) || matches!(&e,event::Event::Key(k) if k.modifiers.contains(event::KeyModifiers::CONTROL))
                    || matches!(&e,event::Event::Mouse(_) if matches!(app.mode,clisheet::ui::Mode::Files(_)|clisheet::ui::Mode::Menu));
                if heavy {
                    with_progress(&mut term, &previous_frame, "Aplicando operação", || {
                        app.event(e)
                    })?
                } else {
                    app.event(e)
                }
                redraw = true
            }
            if app.journal != previous {
                let _ = std::fs::remove_file(previous);
            }
            if app.book.version != before {
                last = Instant::now()
            }
        } else if app.advance_drag()? {
            redraw = true;
        }
        if !app.book.dirty() && app.journal_version.take().is_some() {
            let _ = std::fs::remove_file(&app.journal);
        }
        if app.book.dirty()
            && !app.book.readonly
            && app.journal_version != Some(app.book.version)
            && last.elapsed() >= Duration::from_secs(1)
        {
            match with_progress(&mut term, &previous_frame, "Gravando recuperação", || {
                storage::checkpoint(&app.book, &app.journal)
            })? {
                Ok(()) => {
                    app.journal_version = Some(app.book.version);
                    redraw = true
                }
                Err(e) => {
                    app.status = format!("Recuperação indisponível: {e}");
                    last = Instant::now();
                    redraw = true
                }
            }
        }
    }
    if stop.load(Ordering::Relaxed) && app.book.dirty() && !app.book.readonly {
        storage::checkpoint(&app.book, &app.journal)?
    } else {
        let _ = std::fs::remove_file(app.journal);
    }
    Ok(())
}
fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!(
            "clisheet: {}",
            e.chars().filter(|c| !c.is_control()).collect::<String>()
        );
        std::process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_precedence() -> Result {
        let dir = tempfile::tempdir().map_err(ioerr)?;
        assert!(matches!(startup_theme(dir.path(), None)?, Theme::Light));
        std::fs::write(dir.path().join("config.json"), br#"{"theme":"dark"}"#).map_err(ioerr)?;
        assert!(matches!(startup_theme(dir.path(), None)?, Theme::Dark));
        assert!(matches!(
            startup_theme(dir.path(), Some(Theme::Light))?,
            Theme::Light
        ));
        Ok(())
    }
}
