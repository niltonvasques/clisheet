use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
fn main() -> std::io::Result<()> {
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnableMouseCapture)?;
    let events = (|| {
        let mut events = Vec::new();
        while events.len() < 3 {
            if let Event::Mouse(m) = event::read()? {
                events.push(m.kind);
            }
        }
        Ok::<_, std::io::Error>(events)
    })();
    execute!(std::io::stdout(), DisableMouseCapture)?;
    disable_raw_mode()?;
    println!("{:?}", events?);
    Ok(())
}
