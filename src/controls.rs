use crate::{
    search_editor::SearchEditor, static_selection::StaticSelection, Focus, EVENT_POLL_TIMEOUT,
};
use crossterm::event::Event as CEvent;
use crossterm::event::{self, KeyCode, KeyEventKind, MouseEventKind};
use parking_lot::RwLock;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use tracing::{debug, error};

pub fn controls(
    static_menu_selection: Arc<StaticSelection>,
    focus: Arc<RwLock<Focus>>,
    stop: Arc<AtomicBool>,
    tx: tokio::sync::mpsc::Sender<()>,
) {
    loop {
        let static_menu_selection = static_menu_selection.to_owned();
        if event::poll(EVENT_POLL_TIMEOUT).unwrap() {
            if let Ok(event) = event::read() {
                match event {
                    CEvent::Key(key) => {
                        if let KeyEventKind::Press = key.kind {
                            let focus_ = focus.read().to_owned();
                            match focus_ {
                                Focus::Main => match key.code {
                                    KeyCode::Char('n') => {
                                        *focus.write() = Focus::SearchMod(Arc::new(RwLock::new(
                                            Some(SearchEditor::new_add()),
                                        )))
                                    }
                                    KeyCode::Char('e') => {
                                        if static_menu_selection
                                            .pane_selected
                                            .load(Ordering::SeqCst)
                                            == 1
                                        {
                                            let (search_terms_is_empty, selected_search_term_value) = {
                                                let search_term_tracker_lock =
                                                    static_menu_selection
                                                        .search_term_tracker
                                                        .read();
                                                (
                                                    search_term_tracker_lock
                                                        .search_terms
                                                        .is_empty(),
                                                    search_term_tracker_lock
                                                        .get_value_at_current_index(),
                                                )
                                            };
                                            if !search_terms_is_empty {
                                                if let Some(selected_search_term_value) =
                                                    selected_search_term_value
                                                {
                                                    *focus.write() = Focus::SearchMod(Arc::new(
                                                        RwLock::new(Some(SearchEditor::new_edit(
                                                            selected_search_term_value,
                                                        ))),
                                                    ))
                                                } else {
                                                    error!("Search terms pane was selected, search terms was not empty, yet somehow there wasn't a value selected.");
                                                }
                                            }
                                        }
                                    }
                                    KeyCode::Char('h') => *focus.write() = Focus::Help,
                                    KeyCode::Char('q') | KeyCode::Esc => {
                                        *focus.write() = Focus::ConfirmClose
                                    }
                                    KeyCode::Left => static_menu_selection.pane_left(),
                                    KeyCode::Right => static_menu_selection.pane_right(),
                                    KeyCode::Up => {
                                        match static_menu_selection
                                            .pane_selected
                                            .load(Ordering::SeqCst)
                                        {
                                            0 => static_menu_selection.root_up(),
                                            1 => static_menu_selection
                                                .search_term_tracker
                                                .write()
                                                .up(),
                                            2 => static_menu_selection.result_up(),
                                            _ => {}
                                        }
                                    }
                                    KeyCode::Down => {
                                        match static_menu_selection
                                            .pane_selected
                                            .load(Ordering::SeqCst)
                                        {
                                            0 => static_menu_selection.root_down(),
                                            1 => static_menu_selection
                                                .search_term_tracker
                                                .write()
                                                .down(),
                                            2 => static_menu_selection.result_down(),
                                            _ => {}
                                        }
                                    }
                                    KeyCode::Enter => {
                                        match static_menu_selection
                                            .pane_selected
                                            .load(Ordering::SeqCst)
                                        {
                                            0 => static_menu_selection.root_toggle(),
                                            1 => {}
                                            2 => {}
                                            _ => {}
                                        }
                                    }
                                    KeyCode::F(5) => {
                                        debug!("Triggered run start/stop");
                                        let mut running_lock = static_menu_selection.running.lock();
                                        if *running_lock {
                                            static_menu_selection
                                                .run_control_temporarily_disabled
                                                .store(true, Ordering::SeqCst);
                                            static_menu_selection
                                                .stop
                                                .store(true, Ordering::SeqCst);
                                        } else {
                                            *running_lock = true;
                                            *static_menu_selection.timer.write() =
                                                Some((Instant::now(), None));
                                            tx.blocking_send(()).expect("Failed to send trigger");
                                        }
                                    }
                                    _ => {}
                                },
                                Focus::SearchMod(search_editor) => match key.code {
                                    KeyCode::Backspace => {
                                        search_editor.write().as_mut().unwrap().backspace()
                                    }
                                    KeyCode::Char(ch) => {
                                        search_editor.write().as_mut().unwrap().add_char(ch)
                                    }
                                    KeyCode::Esc => *focus.write() = Focus::Main,
                                    KeyCode::Enter => {
                                        let mut focuslock = focus.write(); //this lock must be held until the end of this scope
                                        let mut search_editor_lock = search_editor.write(); //it is imperitive that nothing tries to read this lock after this write cycle, it should be safe
                                        let probably_search_editor = search_editor_lock.take();
                                        *focuslock = Focus::Main;
                                        let search_editor = match probably_search_editor {
                                            Some(search_editor) => search_editor,
                                            None => {
                                                error!("Write proper error here, this shouldn't be possible as this loop runthrough is the only place that can both run a write lock on search_editor or focus.");
                                                continue;
                                            }
                                        };
                                        let (editor_mode, state) = search_editor.resolve();
                                        static_menu_selection
                                            .search_term_tracker
                                            .write()
                                            .update(editor_mode, state);
                                    }
                                    _ => {}
                                },
                                Focus::Help => match key.code {
                                    KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') => {
                                        *focus.write() = Focus::Main
                                    }
                                    _ => {}
                                },
                                Focus::ConfirmClose => match key.code {
                                    KeyCode::Esc | KeyCode::Char('n') => {
                                        *focus.write() = Focus::Main;
                                    }
                                    KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('q') => {
                                        stop.store(true, Ordering::SeqCst);
                                        drop(tx);
                                        break;
                                    }
                                    _ => {}
                                },
                            }
                        }
                    }
                    CEvent::Mouse(mouse_event) => {
                        let focus_ = focus.read().to_owned();
                        if let Focus::Main = focus_ {
                            if static_menu_selection.pane_selected.load(Ordering::SeqCst) == 2 {
                                match mouse_event.kind {
                                    MouseEventKind::ScrollDown => {
                                        static_menu_selection.result_down();
                                    }
                                    MouseEventKind::ScrollUp => {
                                        static_menu_selection.result_up();
                                    }
                                    MouseEventKind::Down(_) => {
                                        if static_menu_selection
                                            .pane_selected
                                            .load(Ordering::SeqCst)
                                            != 2
                                        {
                                            static_menu_selection.set_pane_selected(2);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else {
        }
    }
}
