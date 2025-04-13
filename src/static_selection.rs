use crate::{
    root::{Root, SelectedRoots},
    search_term_tracker::SearchTermTracker,
    DEBOUNCE, SELECTION_COLOUR,
};
use parking_lot::{Mutex, RwLock};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    time::Instant,
};
use strum::IntoEnumIterator;
use tokio::sync::Notify;

pub struct StaticSelection {
    pub pane_selected: Arc<AtomicU8>,       //horizontal
    pane_last_changed: Arc<Mutex<Instant>>, //horizontal

    pub search_term_tracker: Arc<RwLock<SearchTermTracker>>,

    root_selected: Arc<AtomicU8>,
    root_selection_last_changed: Arc<Mutex<Instant>>,

    pub selected_roots: Arc<RwLock<SelectedRoots>>,

    pub running: Arc<Mutex<bool>>,
    pub timer: Arc<RwLock<Option<(Instant, Option<Instant>)>>>,
    pub run_control_temporarily_disabled: Arc<AtomicBool>, //running thread resets this once closed
    pub stop: Arc<AtomicBool>,                             //running thread resets this once closed
    pub stop_notify: Arc<Notify>,

    pub results: Arc<Mutex<BTreeSet<String>>>,
    result_selected: Arc<AtomicU8>,
    result_selection_last_changed: Arc<Mutex<Instant>>,
}

impl Default for StaticSelection {
    fn default() -> Self {
        Self {
            pane_selected: Arc::new(AtomicU8::new(0)),
            pane_last_changed: Arc::new(Mutex::new(Instant::now())),
            root_selected: Arc::new(AtomicU8::new(0)),
            root_selection_last_changed: Arc::new(Mutex::new(Instant::now())),
            search_term_tracker: Arc::new(RwLock::new(SearchTermTracker::default())),
            selected_roots: Arc::new(RwLock::new(SelectedRoots::default())),
            running: Arc::new(Mutex::new(false)),
            timer: Arc::new(RwLock::new(None)),
            run_control_temporarily_disabled: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            stop_notify: Arc::new(Notify::new()),
            results: Arc::new(Mutex::new(BTreeSet::new())),
            result_selected: Arc::new(AtomicU8::new(0)),
            result_selection_last_changed: Arc::new(Mutex::new(Instant::now())),
        }
    }
}

impl StaticSelection {
    pub fn generate_root_list(&self) -> Vec<Line<'static>> {
        let root_selected = self.root_selected.load(Ordering::SeqCst);
        let pane_selected = self.pane_selected.load(Ordering::SeqCst) == 0;
        Root::iter()
            .map(|root| {
                let root_enabled = self.selected_roots.read().is_enabled(&root);
                Line::from(vec![
                    Span::styled(
                        format!("{:38}", root.to_string(),),
                        Style::default().fg(if pane_selected && root as u8 == root_selected {
                            SELECTION_COLOUR
                        } else {
                            Color::White
                        }),
                    ),
                    Span::styled(
                        if root_enabled { "Enabled" } else { "Disabled" },
                        Style::default().fg(if root_enabled {
                            Color::Green
                        } else {
                            Color::White
                        }),
                    ),
                ])
            })
            .collect::<Vec<Line>>()
    }

    pub fn generate_results(&self) -> Vec<Line<'static>> {
        let pane_selected = self.pane_selected.load(Ordering::SeqCst) == 2;
        let result_selected = self.result_selected.load(Ordering::SeqCst) as usize;

        let results_lock = self.results.lock();
        let results_vec: Vec<&String> = results_lock.iter().collect();

        results_vec
            .iter()
            .enumerate()
            .map(|(index, result)| {
                Line::from(vec![Span::styled(
                    result.to_string(),
                    Style::default().fg(if pane_selected && index == result_selected {
                        SELECTION_COLOUR
                    } else {
                        Color::White
                    }),
                )])
            })
            .collect::<Vec<Line>>()
    }

    pub fn pane_left(&self) {
        if self.pane_last_changed.lock().elapsed() < DEBOUNCE {
            return;
        }
        let new_value = match self.pane_selected.load(Ordering::SeqCst) {
            0 => 2,
            1 => 0,
            2 => 1,
            _ => return,
        };
        self.pane_selected.store(new_value, Ordering::SeqCst);
        *self.pane_last_changed.lock() = Instant::now();
    }

    pub fn pane_right(&self) {
        if self.pane_last_changed.lock().elapsed() < DEBOUNCE {
            return;
        }
        let new_value = match self.pane_selected.load(Ordering::SeqCst) {
            0 => 1,
            1 => 2,
            2 => 0,
            _ => return,
        };
        self.pane_selected.store(new_value, Ordering::SeqCst);
        *self.pane_last_changed.lock() = Instant::now();
    }

    pub fn root_up(&self) {
        if self.root_selection_last_changed.lock().elapsed() < DEBOUNCE {
            return;
        }
        let current_value = self.root_selected.load(Ordering::SeqCst);
        let new_value = if current_value == 0 {
            9
        } else {
            current_value - 1
        };
        self.root_selected.store(new_value, Ordering::SeqCst);
        *self.root_selection_last_changed.lock() = Instant::now();
    }

    pub fn root_down(&self) {
        if self.root_selection_last_changed.lock().elapsed() < DEBOUNCE {
            return;
        }
        let new_value = (self.root_selected.load(Ordering::SeqCst) + 1) % 10;
        self.root_selected.store(new_value, Ordering::SeqCst);
        *self.root_selection_last_changed.lock() = Instant::now();
    }

    pub fn root_toggle(&self) {
        let selected = self.root_selected.load(Ordering::SeqCst);
        if let Some(root) = Root::from_u8(selected) {
            self.selected_roots.write().toggle(&root);
        }
    }

    pub fn result_up(&self) {
        if self.result_selection_last_changed.lock().elapsed() < DEBOUNCE {
            return;
        }

        let results_count = self.results.lock().len() as u8;
        if results_count == 0 {
            return;
        }

        let current_value = self.result_selected.load(Ordering::SeqCst);
        let new_value = if current_value == 0 {
            results_count - 1
        } else {
            current_value - 1
        };

        self.result_selected.store(new_value, Ordering::SeqCst);
        *self.result_selection_last_changed.lock() = Instant::now();
    }

    pub fn result_down(&self) {
        if self.result_selection_last_changed.lock().elapsed() < DEBOUNCE {
            return;
        }

        let results_count = self.results.lock().len() as u8;
        if results_count == 0 {
            return;
        }

        let new_value = (self.result_selected.load(Ordering::SeqCst) + 1) % results_count;
        self.result_selected.store(new_value, Ordering::SeqCst);
        *self.result_selection_last_changed.lock() = Instant::now();
    }

    pub fn get_result_selected(&self) -> usize {
        self.result_selected.load(Ordering::SeqCst) as usize
    }

    pub fn set_pane_selected(&self, pane: u8) {
        self.pane_selected.store(pane, Ordering::SeqCst);
        *self.pane_last_changed.lock() = Instant::now();
    }
}
