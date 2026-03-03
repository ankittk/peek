mod app;
pub(crate) mod ui;

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub use app::App;

use super::args::Cli;
use peek_core::CollectOptions;

/// Entry point for the live-updating TUI dashboard.
pub fn run_tui(pid: i32, cli: &Cli, interval: Duration) -> Result<()> {
    let opts = CollectOptions {
        resources: true, // always collect for sparklines
        kernel: cli.kernel || cli.all,
        network: cli.network || cli.all,
        files: cli.files || cli.all,
        env: cli.env || cli.all,
        tree: cli.tree || cli.all,
        gpu: true, // always collect in TUI so GPU tab is useful
    };

    let mut app = App::new(pid, opts, interval);

    // Initial data load
    app.refresh();

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Ensure we always restore the terminal even if the TUI panics
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        event_loop(&mut terminal, &mut app)
    }));

    // Best-effort restore of terminal state
    let restore_result = (|| -> Result<()> {
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        Ok(())
    })();

    match result {
        Ok(loop_result) => {
            // Prefer any restoration error over the inner result
            restore_result.and(loop_result)
        }
        Err(panic) => {
            // Ignore restoration errors on panic and resume unwinding
            let _ = restore_result;
            std::panic::resume_unwind(panic);
        }
    }
}

fn event_loop(
    terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll for input with a short timeout so the loop stays responsive
        let poll_timeout = app
            .interval
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_millis(50))
            .min(Duration::from_millis(100));

        if event::poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                handle_key(app, key.code, key.modifiers);
            }
        }

        if last_tick.elapsed() >= app.interval {
            app.refresh();
            last_tick = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match code {
        // Quit
        KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }

        // Tab navigation
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.next_tab(),
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.prev_tab(),
        KeyCode::Char('1') => app.active_tab = 0,
        KeyCode::Char('2') => app.active_tab = 1,
        KeyCode::Char('3') => app.active_tab = 2,
        KeyCode::Char('4') => app.active_tab = 3,
        KeyCode::Char('5') => app.active_tab = 4,
        KeyCode::Char('6') => app.active_tab = 5,
        KeyCode::Char('7') => app.active_tab = 6,

        // Pause / resume
        KeyCode::Char(' ') => app.paused = !app.paused,

        // Help overlay
        KeyCode::Char('?') => app.show_help = !app.show_help,

        _ => {}
    }
}
