use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal, Frame,
};
use std::{io, time::Duration};

pub struct App {
    pub input: String,
    pub messages: Vec<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            input: String::new(),
            messages: vec!["Welcome to Claude Code (Rust Port)!".to_string()],
        }
    }
}

pub async fn run_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::default();

    let res = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char(c) => {
                        app.input.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Enter => {
                        if !app.input.is_empty() {
                            let msg = app.input.drain(..).collect::<String>();
                            app.messages.push(format!("You: {}", msg));
                            app.messages.push(format!("Claude: simulated thinking for {}", msg));
                        }
                    }
                    KeyCode::Esc => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Min(3),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.size());

    let messages: Vec<Line> = app
        .messages
        .iter()
        .map(|m| {
            Line::from(vec![
                Span::raw(m),
            ])
        })
        .collect();

    let history_widget = Paragraph::new(messages)
        .block(Block::default().borders(Borders::ALL).title(" Transcript "));
    f.render_widget(history_widget, chunks[0]);

    let input_widget = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title(" Input (Press Enter to submit, Esc to quit) "));
    f.render_widget(input_widget, chunks[1]);
}
