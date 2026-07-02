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
    let mut rx = update::spawn_check();

    loop {
        let mut terminal = setup_terminal()?;
        let outcome = run(&mut terminal, &mut app, &mut rx);
        restore_terminal();
        let _ = terminal.show_cursor();
        match outcome? {
            Outcome::Quit => break,
            Outcome::Update => {
                app.update_requested = false;
                println!(
                    "⬆ updating 2idraw ({} → latest)...",
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
                            println!("restart 2idraw to use the new version: {}", bin.display());
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

const AUTO_CHECK_EVERY: Duration = Duration::from_secs(300);

fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    rx: &mut std::sync::mpsc::Receiver<update::UpdateStatus>,
) -> Result<Outcome> {
    let mut last_check = std::time::Instant::now();
    let mut manual_check = false;
    loop {
        if app.check_requested {
            // user pressed U with no update known — re-check now
            app.check_requested = false;
            app.update = update::UpdateStatus::Checking;
            *rx = update::spawn_check();
            last_check = std::time::Instant::now();
            manual_check = true;
        } else if last_check.elapsed() >= AUTO_CHECK_EVERY
            && !matches!(
                app.update,
                update::UpdateStatus::Checking | update::UpdateStatus::Available { .. }
            )
        {
            app.update = update::UpdateStatus::Checking;
            *rx = update::spawn_check();
            last_check = std::time::Instant::now();
            manual_check = false;
        }

        if let Ok(st) = rx.try_recv() {
            match &st {
                update::UpdateStatus::Available { commit } => {
                    let short = &commit[..commit.len().min(7)];
                    app.status = format!("update available ({short}) — press U");
                }
                // only announce quiet results when the user asked for the check
                update::UpdateStatus::UpToDate if manual_check => {
                    app.status = "2idraw is up to date".to_string();
                }
                update::UpdateStatus::CheckFailed(e) if manual_check => {
                    app.status = format!("update check failed: {e}");
                }
                _ => {}
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
