use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::{Frame, Terminal};
use std::io;
use std::time::Duration;

use crate::db::Database;
use crate::schema::{History, SearchFilter};

struct ScoredEntry {
    history: History,
    score: i64,
    positions: Vec<usize>,
}

pub fn standalone_search(db: &Database, initial_query: Option<&str>) -> Result<Option<String>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let matcher = SkimMatcherV2::default();

    let mut input = initial_query.unwrap_or("").to_string();
    let mut cursor = input.len();
    let mut selected: usize = 0;
    let mut results: Vec<ScoredEntry> = Vec::new();
    let mut result_cmd: Option<String> = None;
    let total = db.count().unwrap_or(0);

    // Initial query.
    refresh_results(db, &matcher, &input, &mut results);

    loop {
        terminal.draw(|f| draw(f, &input, cursor, &results, selected, total))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Enter => {
                        if let Some(e) = results.get(selected) {
                            result_cmd = Some(e.history.command.clone());
                        }
                        break;
                    }
                    KeyCode::Up => selected = selected.saturating_sub(1),
                    KeyCode::Down => {
                        if selected + 1 < results.len() { selected += 1; }
                    }
                    KeyCode::Char(c) => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                            break;
                        }
                        input.insert(cursor, c);
                        cursor += 1;
                        selected = 0;
                        refresh_results(db, &matcher, &input, &mut results);
                    }
                    KeyCode::Backspace => {
                        if cursor > 0 {
                            cursor -= 1;
                            input.remove(cursor);
                            selected = 0;
                            refresh_results(db, &matcher, &input, &mut results);
                        }
                    }
                    KeyCode::Left => cursor = cursor.saturating_sub(1),
                    KeyCode::Right => { if cursor < input.len() { cursor += 1; } }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(result_cmd)
}

fn refresh_results(db: &Database, matcher: &SkimMatcherV2, query: &str, results: &mut Vec<ScoredEntry>) {
    let filter = SearchFilter {
        query: if query.is_empty() { None } else { Some(query.to_string()) },
        limit: 500,
        ..Default::default()
    };

    let entries = db.search(&filter).unwrap_or_default();

    if query.is_empty() {
        *results = entries.into_iter().map(|h| ScoredEntry { history: h, score: 0, positions: vec![] }).collect();
    } else {
        let mut scored: Vec<ScoredEntry> = entries
            .into_iter()
            .filter_map(|h| {
                matcher.fuzzy_indices(&h.command, query).map(|(score, pos)| ScoredEntry {
                    history: h, score, positions: pos,
                })
            })
            .collect();
        scored.sort_by(|a, b| b.score.cmp(&a.score));
        *results = scored;
    }
}

fn draw(f: &mut Frame, input: &str, cursor: usize, results: &[ScoredEntry], selected: usize, total: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(1)])
        .split(f.area());

    // Input bar.
    let input_w = Paragraph::new(input)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)).title(" stytsch search "));
    f.render_widget(input_w, chunks[0]);
    f.set_cursor_position((chunks[0].x + cursor as u16 + 1, chunks[0].y + 1));

    // Results.
    let items: Vec<ListItem> = results.iter().enumerate().take(chunks[1].height as usize).map(|(i, s)| {
        let h = &s.history;
        let exit_style = if h.exit == 0 { Style::default().fg(Color::Green) } else { Style::default().fg(Color::Red) };
        let base = if i == selected { Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD) } else { Style::default() };

        let cmd_spans = highlight(&h.command, &s.positions);
        let mut spans = vec![
            Span::styled(format!(" {:>3} ", h.exit), exit_style),
        ];
        spans.extend(cmd_spans);
        ListItem::new(Line::from(spans)).style(base)
    }).collect();

    let list = List::new(items).block(
        Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray))
            .title(format!(" {} / {} ", results.len(), total)),
    );
    f.render_widget(list, chunks[1]);

    // Status bar.
    let bar = Paragraph::new(" Enter: select | Esc: cancel | Type to filter")
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));
    f.render_widget(bar, chunks[2]);
}

fn highlight(cmd: &str, positions: &[usize]) -> Vec<Span<'static>> {
    let chars: Vec<char> = cmd.chars().collect();
    let mut spans = Vec::new();
    let mut buf = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if positions.contains(&i) {
            if !buf.is_empty() { spans.push(Span::raw(buf.clone())); buf.clear(); }
            spans.push(Span::styled(c.to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() { spans.push(Span::raw(buf)); }
    spans
}
