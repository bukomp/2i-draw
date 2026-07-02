mod app;
mod canvas;
mod ui;
mod update;

use std::io::{stdout, Stdout};
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

enum Outcome {
    Quit,
    Update,
}

fn main() -> Result<()> {
    install_panic_hook();

    let mut app = App::new();
    let rx = update::spawn_check();

    loop {
        let mut terminal = setup_terminal()?;
        let outcome = run(&mut terminal, &mut app, &rx);
        restore_terminal();
        let _ = terminal.show_cursor();
        match outcome? {
            Outcome::Quit => break,
            Outcome::Update => {
                app.update_requested = false;
                println!(
                    "⬆ updating idraw ({} → latest)...",
                    &update::BUILD_COMMIT[..update::BUILD_COMMIT.len().min(7)]
                );
                match update::perform_update() {
                    Ok(bin) => {
                        println!("✓ updated — restarting");
                        // unix: replace this process with the new binary
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::CommandExt;
                            let err = std::process::Command::new(&bin).exec();
                            eprintln!("restart failed ({err}); run {} manually", bin.display());
                        }
                        #[cfg(not(unix))]
                        {
                            println!("restart idraw to use the new version: {}", bin.display());
                        }
                        break;
                    }
                    Err(e) => {
                        app.status = format!("update failed: {e}");
                    } // loop re-enters the TUI
                }
            }
        }
    }

    Ok(())
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    rx: &std::sync::mpsc::Receiver<update::UpdateStatus>,
) -> Result<Outcome> {
    loop {
        if let Ok(st) = rx.try_recv() {
            if let update::UpdateStatus::Available { commit } = &st {
                let short = &commit[..commit.len().min(7)];
                app.status = format!("update available ({short}) — press U");
            }
            app.update = st;
        }

        terminal.draw(|f| ui::draw(f, app))?;

        if event::poll(Duration::from_millis(50))? {
            loop {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            app.handle_key(key);
                        }
                    }
                    Event::Mouse(mouse) => {
                        app.handle_mouse(mouse);
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }

                if !event::poll(Duration::from_millis(0))? {
                    break;
                }
            }
        }

        if app.update_requested {
            return Ok(Outcome::Update);
        }
        if app.quit {
            return Ok(Outcome::Quit);
        }
    }
}
