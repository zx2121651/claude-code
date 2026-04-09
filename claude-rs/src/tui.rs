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
use tokio::sync::{mpsc, oneshot};

use crate::engine::{QueryEngine, EngineEvent};
use crate::tools::{BashTool, AgentTool, FileReadTool, FileEditTool, GlobTool};

pub struct App {
    pub input: String,
    pub messages: Vec<String>,
    pub is_processing: bool,
    pub pending_permission: Option<(String, String, oneshot::Sender<bool>)>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            input: String::new(),
            messages: vec!["Welcome to Claude Code (Rust Port)!".to_string()],
            is_processing: false,
            pending_permission: None,
        }
    }
}

pub async fn run_tui(api_key: String, model: String) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::default();

    let res = run_app(&mut terminal, &mut app, api_key, model).await;

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

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App, api_key: String, model: String) -> Result<()> {
    let (tx_engine, mut rx_engine) = mpsc::channel::<EngineEvent>(100);
    let (tx_ui, mut rx_ui) = mpsc::channel::<String>(10);

    tokio::spawn(async move {
        let mut engine = QueryEngine::new(
            api_key,
            model,
            Some("You are Claude Code, an AI assistant.")
        );
        engine.register_tool(Box::new(BashTool));
        engine.register_tool(Box::new(AgentTool {}));
        engine.register_tool(Box::new(FileReadTool));
        engine.register_tool(Box::new(FileEditTool));
        engine.register_tool(Box::new(GlobTool));

        while let Some(prompt) = rx_ui.recv().await {
            let _ = engine.submit_message(&prompt, Some(tx_engine.clone())).await;
        }
    });

    loop {
        terminal.draw(|f| ui(f, app))?;

        while let Ok(event) = rx_engine.try_recv() {
            match event {
                EngineEvent::TextDelta(text) => {
                    let text = text.replace("\n", "");
                    if let Some(last) = app.messages.last_mut() {
                        if last.starts_with("Claude: ") {
                            last.push_str(&text);
                        } else {
                            app.messages.push(format!("Claude: {}", text));
                        }
                    } else {
                        app.messages.push(format!("Claude: {}", text));
                    }
                }
                EngineEvent::ToolExecutionStarted(tool_name) => {
                    app.messages.push(format!("[Running Tool: {}]", tool_name));
                }
                EngineEvent::ToolExecutionCompleted(_tool_name, _res) => {
                }
                EngineEvent::ToolExecutionError(tool_name, err) => {
                    app.messages.push(format!("[Tool Error {}]: {}", tool_name, err));
                }
                EngineEvent::TurnCompleted => {
                    app.is_processing = false;
                }
                EngineEvent::Error(err) => {
                    app.messages.push(format!("API Error: {}", err));
                    app.is_processing = false;
                }
                EngineEvent::PermissionRequested(tool_name, tool_input, reply_tx) => {
                    app.pending_permission = Some((tool_name, tool_input, reply_tx));
                }
            }
        }

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {

                if app.pending_permission.is_some() {
                    let handled = match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            if let Some((_, _, reply_tx)) = app.pending_permission.take() {
                                let _ = reply_tx.send(true);
                                app.messages.push("[User Allowed Tool Execution]".to_string());
                            }
                            true
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Enter => {
                            if let Some((_, _, reply_tx)) = app.pending_permission.take() {
                                let _ = reply_tx.send(false);
                                app.messages.push("[User Denied Tool Execution]".to_string());
                            }
                            true
                        }
                        _ => false
                    };

                    if handled {
                        continue;
                    }
                }

                // Normal input processing
                match key.code {
                    KeyCode::Char(c) => {
                        app.input.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Enter => {
                        if !app.input.is_empty() && !app.is_processing {
                            let msg = app.input.drain(..).collect::<String>();
                            app.messages.push(format!("You: {}", msg));

                            app.is_processing = true;

                            let _ = tx_ui.send(msg).await;
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
    let mut constraints = vec![
        Constraint::Min(3),
        Constraint::Length(3),
    ];

    if app.pending_permission.is_some() {
        constraints.insert(1, Constraint::Length(6));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(constraints.clone()) // Fixed the inference error here
        .split(f.size());

    let messages: Vec<Line> = app
        .messages
        .iter()
        .flat_map(|m| {
            m.split('\n').map(|l| Line::from(vec![Span::raw(l)]))
        })
        .collect();

    let history_widget = Paragraph::new(messages)
        .block(Block::default().borders(Borders::ALL).title(" Transcript "));
    f.render_widget(history_widget, chunks[0]);

    if let Some((tool_name, tool_input, _)) = &app.pending_permission {
        let warning_text = vec![
            Line::from(vec![Span::styled(format!("Danger: Tool '{}' requested permission to run.", tool_name), Style::default().fg(Color::Red))]),
            Line::from(vec![Span::raw("Input:")]),
            Line::from(vec![Span::styled(tool_input.as_str(), Style::default().fg(Color::Yellow))]),
            Line::from(vec![Span::styled("Do you want to allow this? [y/N]", Style::default().fg(Color::White).bg(Color::Red))]),
        ];

        let warning_widget = Paragraph::new(warning_text)
            .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Red)));

        f.render_widget(warning_widget, chunks[1]);

        let input_widget = Paragraph::new(app.input.as_str())
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(" Engine Suspended "));
        f.render_widget(input_widget, chunks[2]);

    } else {
        let input_title = if app.is_processing {
            " Claude is typing... "
        } else {
            " Input (Press Enter to submit, Esc to quit) "
        };

        let input_widget = Paragraph::new(app.input.as_str())
            .style(Style::default().fg(if app.is_processing { Color::DarkGray } else { Color::Yellow }))
            .block(Block::default().borders(Borders::ALL).title(input_title));
        f.render_widget(input_widget, chunks[1]);
    }
}
