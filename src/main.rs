use std::{
    io::{self, stdout},
    path::Path,
};

use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use git2::{Repository, Status, StatusEntry};
use ratatui::{
    prelude::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame, Terminal,
};

#[derive(Debug)]
struct State {
    quit: bool,
    selected: usize,
    items: Vec<Item>,
}

#[derive(Default, Clone, Debug)]
struct Item {
    file: Option<String>,
    header: Option<String>,
    section: Option<Section>,
    status: Option<String>,
}

#[derive(Clone, Debug)]
struct Section {
    collapsed: bool,
    size: usize,
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut repo = Repository::open(".").unwrap();
    let items = create_status_items(&repo);

    let mut state = State {
        quit: false,
        selected: 0,
        items,
    };

    while !state.quit {
        terminal.draw(|frame| ui(frame, &state))?;
        handle_events(&mut state, &mut repo)?;
        state.selected = state.selected.clamp(0, state.items.len() - 1);
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn create_status_items(repo: &Repository) -> Vec<Item> {
    let statuses = repo.statuses(None).unwrap();
    let mut items = vec![];

    let untracked = statuses
        .into_iter()
        .filter(|entry| entry.status().is_wt_new())
        .map(|entry| Item {
            file: entry.path().map(|value| value.to_string()),
            ..Default::default()
        })
        .collect::<Vec<_>>();

    if !untracked.is_empty() {
        items.push(Item {
            header: Some(format!("Untracked files ({})", untracked.len())),
            section: Some(Section {
                collapsed: false,
                size: untracked.len(),
            }),
            ..Default::default()
        });
        items.extend(untracked);
    }

    let unstaged = statuses
        .into_iter()
        .filter(|entry| entry.status().is_wt_modified())
        .map(|entry| Item {
            file: entry.path().map(|value| value.to_string()),
            status: Some(unstaged_entry_status(entry)),
            section: Some(Section {
                collapsed: true,
                // TODO This would be for the diff. How big will it be?
                size: 0,
            }),
            ..Default::default()
        })
        .collect::<Vec<_>>();

    if !unstaged.is_empty() {
        items.push(Item {
            header: Some(format!("Unstaged changes ({})", unstaged.len())),
            section: Some(Section {
                collapsed: false,
                size: unstaged.len(),
            }),
            ..Default::default()
        });
        items.extend(unstaged);
    }

    let staged = statuses
        .into_iter()
        .filter(|entry| {
            entry.status().intersects(
                Status::INDEX_NEW
                    | Status::INDEX_DELETED
                    | Status::INDEX_TYPECHANGE
                    | Status::INDEX_RENAMED,
            )
        })
        .map(|entry| Item {
            file: entry.path().map(|value| value.to_string()),
            status: Some(staged_entry_status(entry)),
            ..Default::default()
        })
        .collect::<Vec<_>>();

    if !staged.is_empty() {
        items.push(Item {
            header: Some(format!("Staged changes ({})", staged.len())),
            section: Some(Section {
                collapsed: false,
                size: staged.len(),
            }),
            ..Default::default()
        });
        items.extend(staged);
    }

    items
}

fn unstaged_entry_status(entry: StatusEntry) -> String {
    if entry.status().is_wt_modified() {
        "modified".to_string()
    } else {
        format!("{:?}", entry.status())
    }
}

fn staged_entry_status(entry: StatusEntry) -> String {
    if entry.status().is_index_new() {
        "new file".to_string()
    } else {
        format!("{:?}", entry.status())
    }
}

fn ui(frame: &mut Frame, state: &State) {
    let lines = collapsed_items_iter(&state.items)
        .filter_map(|(i, item)| item.map(|item| (i, item)))
        .flat_map(|(i, item)| {
            let mut text = if let Some(ref text) = item.header {
                Line::styled(text, Style::new().fg(Color::Blue))
            } else if let Item {
                file: Some(file),
                status,
                ..
            } = item
            {
                match status {
                    Some(s) => Line::styled(format!("{}   {}", s, file), Style::new()),
                    None => Line::styled(format!("{}", file), Style::new().fg(Color::LightMagenta)),
                }
            } else {
                Line::styled("".to_string(), Style::new())
            };

            text.patch_style(
                Style::new()
                    .bg(if state.selected == i {
                        Color::DarkGray
                    } else {
                        Color::default()
                    })
                    .add_modifier(Modifier::BOLD),
            );

            if item.section.clone().is_some_and(|s| s.collapsed) {
                text.spans.push(Span::raw("…"))
            }

            if item.header.is_some() {
                vec![Line::raw(""), text]
            } else {
                vec![text]
            }
        })
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(Text::from(lines)), frame.size());
}

fn handle_events(state: &mut State, repo: &mut Repository) -> io::Result<bool> {
    if event::poll(std::time::Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == event::KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') => state.quit = true,
                    KeyCode::Char('j') => {
                        state.selected = collapsed_items_iter(&state.items)
                            .find(|(i, item)| i > &state.selected && item.is_some())
                            .unwrap_or((state.selected, None))
                            .0
                    }
                    KeyCode::Char('k') => {
                        state.selected = collapsed_items_iter(&state.items)
                            .filter(|(i, item)| i < &state.selected && item.is_some())
                            .last()
                            .unwrap_or((state.selected, None))
                            .0
                    }
                    KeyCode::Char('s') => {
                        if let Some(ref file) = state.items[state.selected].file {
                            let index = &mut repo.index().unwrap();
                            index.add_path(Path::new(&file)).unwrap();
                            index.write().unwrap();
                            state.items = create_status_items(repo);
                        }
                    }
                    KeyCode::Char('u') => {
                        if let Some(ref file) = state.items[state.selected].file {
                            let index = &mut repo.index().unwrap();
                            index.remove_path(Path::new(&file)).unwrap();
                            index.write().unwrap();
                            state.items = create_status_items(repo);
                        }
                    }
                    KeyCode::Tab => {
                        if let Some(ref mut section) = state.items[state.selected].section {
                            section.collapsed = !section.collapsed;
                        }
                    }
                    _ => (),
                }
            }
        }
    }
    Ok(false)
}

fn collapsed_items_iter<'a>(
    items: &'a Vec<Item>,
) -> impl Iterator<Item = (usize, Option<&'a Item>)> {
    items.iter().enumerate().scan(0, |skips, (i, next)| {
        let next_result = if *skips > 0 {
            *skips -= 1;
            (i, None)
        } else {
            if let Some(Section {
                collapsed: true,
                size,
            }) = next.section
            {
                *skips = size;
            }
            (i, Some(next))
        };

        Some(next_result)
    })
}
