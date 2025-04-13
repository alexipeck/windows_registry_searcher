use std::{
    error::Error,
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use parking_lot::RwLock;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Terminal,
};
use tracing::error;

use crate::{static_selection::StaticSelection, Focus, KEY_COUNT, SELECTION_COLOUR, VALUE_COUNT};

pub fn renderer_wrappers_wrapper(
    static_menu_selection: Arc<StaticSelection>,
    focus: Arc<RwLock<Focus>>,
    stop: Arc<AtomicBool>,
) -> Result<(), ()> {
    let result = renderer_wrapper(
        static_menu_selection.to_owned(),
        focus.to_owned(),
        stop.to_owned(),
    );
    stop.store(true, Ordering::SeqCst);
    match result {
        Ok(_) => return Ok(()),
        Err(err) => {
            error!("{}", err);
            return Err(());
        }
    }
}

pub fn renderer_wrapper(
    static_menu_selection: Arc<StaticSelection>,
    focus: Arc<RwLock<Focus>>,
    stop: Arc<AtomicBool>,
) -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal: Terminal<CrosstermBackend<io::Stdout>> = Terminal::new(backend)?;
    terminal.clear()?;

    let renderer_result = renderer(
        &mut terminal,
        static_menu_selection.to_owned(),
        focus.to_owned(),
        stop.to_owned(),
    );

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    renderer_result
}

pub fn renderer(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    static_menu_selection: Arc<StaticSelection>,
    focus: Arc<RwLock<Focus>>,
    stop: Arc<AtomicBool>,
) -> Result<(), Box<dyn Error>> {
    let mut vertical_scroll = 0;
    let mut visible_height = 0;

    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }

        let focus_state = focus.read().to_owned();
        if let Focus::Main = focus_state {
            if static_menu_selection.pane_selected.load(Ordering::SeqCst) == 2 {
                let selected_index = static_menu_selection.get_result_selected();
                let results_count = static_menu_selection.results.lock().len();

                if visible_height > 0 {
                    let buffer = (visible_height / 4).max(1);

                    if selected_index >= vertical_scroll + visible_height - buffer {
                        vertical_scroll = (selected_index + buffer)
                            .saturating_sub(visible_height)
                            .min(results_count.saturating_sub(visible_height));
                    } else if selected_index < vertical_scroll + buffer {
                        vertical_scroll = selected_index.saturating_sub(buffer);
                    }
                } else {
                    vertical_scroll = selected_index;
                }
            }
        }

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Max(100)].as_ref())
                .split(f.size());
            let running = *static_menu_selection.running.lock();
            let run_control_disabled = static_menu_selection
                .run_control_temporarily_disabled
                .load(Ordering::SeqCst);
            let top_paragraph = Paragraph::new(Line::from(vec![
                Span::raw("[H for the Help menu]"),
                Span::raw(" [Arrow keys for navigation]"),
                Span::raw(" [Enter to select/toggle]"),
                Span::raw(" [Page up/down for first/last element]"),
                Span::raw(" [F5 "),
                Span::styled(
                    if running {
                        if running && run_control_disabled {
                            "Stopping"
                        } else {
                            "Stop"
                        }
                    } else {
                        "Start"
                    },
                    Style::default().fg(if running && !run_control_disabled {
                        Color::Green
                    } else if running && run_control_disabled {
                        Color::Red
                    } else {
                        Color::Green
                    }),
                ),
                Span::raw("]"),
                {
                    let timer = static_menu_selection.timer.read();
                    match timer.as_ref() {
                        Some((start, end)) => {
                            let t = match end.as_ref() {
                                Some(end) => format!(
                                    "[Last runtime: {}s]",
                                    end.duration_since(*start).as_secs()
                                ),
                                None => format!("[Runtime: {}s]", start.elapsed().as_secs()),
                            };
                            Span::raw(t)
                        }
                        None => Span::raw(""),
                    }
                },
                Span::raw(format!(
                    " [Key count: {}]",
                    KEY_COUNT.load(Ordering::SeqCst)
                )),
                Span::raw(format!(
                    " [Value count: {}]",
                    VALUE_COUNT.load(Ordering::SeqCst)
                )),
                Span::raw(format!(
                    " [Results count: {}]",
                    static_menu_selection.results.lock().len()
                )),
            ]))
            .block(Block::default())
            .wrap(Wrap { trim: true });
            f.render_widget(top_paragraph, chunks[0]);
            let bottom_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .margin(1)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)].as_ref())
                .split(chunks[1]);
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(75)].as_ref())
                .split(bottom_chunks[0]);

            let pane_selected = static_menu_selection.pane_selected.load(Ordering::SeqCst);

            let roots_paragraph = Paragraph::new(static_menu_selection.generate_root_list()).block(
                Block::default()
                    .title(Span::styled(
                        " 1. Root Selection ",
                        Style::default().fg(Color::White),
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(if pane_selected == 0 {
                        SELECTION_COLOUR
                    } else {
                        Color::White
                    })),
            );

            let search_terms_paragraph = Paragraph::new(
                static_menu_selection
                    .search_term_tracker
                    .read()
                    .render(pane_selected == 1),
            )
            .block(
                Block::default()
                    .title(Span::styled(
                        " 2. Search Terms ",
                        Style::default().fg(Color::White),
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(if pane_selected == 1 {
                        SELECTION_COLOUR
                    } else {
                        Color::White
                    })),
            )
            .wrap(Wrap { trim: true });

            f.render_widget(roots_paragraph, left_chunks[0]);
            f.render_widget(search_terms_paragraph, left_chunks[1]);

            let results = static_menu_selection.generate_results();
            let right_text = Text::from(results.clone());

            let results_area = bottom_chunks[1].inner(Margin {
                vertical: 1,
                horizontal: 0,
            });
            visible_height = results_area.height as usize;

            let right_paragraph = Paragraph::new(right_text.clone())
                .scroll((vertical_scroll as u16, 0))
                .block(
                    Block::default()
                        .title(Span::styled(
                            " 3. Results ",
                            Style::default().fg(Color::White),
                        ))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(if pane_selected == 2 {
                            SELECTION_COLOUR
                        } else {
                            Color::White
                        })),
                );
            f.render_widget(right_paragraph, bottom_chunks[1]);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state = ScrollbarState::new(results.len()).position(vertical_scroll);

            f.render_stateful_widget(
                scrollbar,
                bottom_chunks[1].inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );

            // Renders overlay
            let focus = focus.read().to_owned();
            match focus {
                Focus::Main => {}
                _ => {
                    let vertical_split = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            [
                                Constraint::Ratio(1, 3),
                                Constraint::Ratio(1, 3),
                                Constraint::Ratio(1, 3),
                            ]
                            .as_ref(),
                        )
                        .split(f.size());
                    let horizontal_split = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(
                            [
                                Constraint::Ratio(1, 3),
                                Constraint::Ratio(1, 3),
                                Constraint::Ratio(1, 3),
                            ]
                            .as_ref(),
                        )
                        .split(vertical_split[1]);
                    let middle_pane = horizontal_split[1];
                    let paragraph = match focus {
                        Focus::ConfirmClose => Paragraph::new("Y/N").block(
                            Block::default()
                                .title(Span::styled(
                                    "Confirm Close",
                                    Style::default().fg(Color::White),
                                ))
                                .style(Style::default().bg(Color::DarkGray))
                                .borders(Borders::ALL)
                                .border_style(Style::default().fg(Color::White)),
                        ),
                        Focus::Help => Paragraph::new("Placeholder").block(
                            Block::default()
                                .title(Span::styled(
                                    "Help/Controls",
                                    Style::default().fg(Color::White),
                                ))
                                .style(Style::default().bg(Color::DarkGray))
                                .borders(Borders::ALL)
                                .border_style(Style::default().fg(Color::White)),
                        ),
                        Focus::SearchMod(search_editor) => {
                            Paragraph::new(search_editor.read().as_ref().unwrap().render()).block(
                                Block::default()
                                    .title(Span::styled(
                                        "Search Modify",
                                        Style::default().fg(Color::White),
                                    ))
                                    .style(Style::default().bg(Color::DarkGray))
                                    .borders(Borders::ALL)
                                    .border_style(Style::default().fg(Color::White)),
                            )
                        }
                        Focus::Main => unreachable!(), // this case will never run
                    };
                    f.render_widget(paragraph, middle_pane);
                }
            }
        })?;
    }
    Ok(())
}
