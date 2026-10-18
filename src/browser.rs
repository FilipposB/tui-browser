use crate::common::EventHandling;
use crate::tab::TabManager;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserScreen {
    Browsing,
    EnteringUrl,
    ManagingTabs,
    ViewingHistory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
    url: String,
    title: String,
    visited_at_unix: u64,
}

pub struct Browser {
    should_quit: bool,
    tab_manager: TabManager,
    screen: BrowserScreen,
    url_input: String,
    history: Vec<HistoryEntry>,
    history_path: PathBuf,
    selected_history: Option<usize>,
    tabs_selection: usize,
}

impl Browser {
    pub fn new() -> io::Result<Self> {
        Self::with_history_file(history_file_path()?)
    }

    fn with_history_file(history_path: PathBuf) -> io::Result<Self> {
        let history = match fs::read(&history_path) {
            Ok(contents) => serde_json::from_slice(&contents)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            should_quit: false,
            tab_manager: TabManager::new(),
            screen: BrowserScreen::EnteringUrl,
            url_input: String::new(),
            history,
            history_path,
            selected_history: None,
            tabs_selection: 0,
        })
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        loop {
            self.handle_events()?;
            terminal.draw(|frame| self.render(frame))?;
            if self.should_quit {
                break Ok(());
            }
        }
    }

    fn handle_events(&mut self) -> std::io::Result<()> {
        let event = event::read()?;
        if let Event::Key(key) = &event {
            if key.kind == KeyEventKind::Press {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
                    self.should_quit = true;
                    return Ok(());
                }
                if is_history_shortcut(key) {
                    self.screen = BrowserScreen::ViewingHistory;
                    return Ok(());
                }
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('l') => {
                            self.start_url_entry();
                            return Ok(());
                        }
                        KeyCode::Char('t') => {
                            self.screen = BrowserScreen::ManagingTabs;
                            self.sync_tabs_selection();
                            return Ok(());
                        }
                        KeyCode::Char('w') if self.tab_manager.active_tab().is_some() => {
                            self.tab_manager.close_active_tab();
                            self.screen = if self.tab_manager.active_tab().is_some() {
                                BrowserScreen::Browsing
                            } else {
                                BrowserScreen::EnteringUrl
                            };
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }

        match self.screen {
            BrowserScreen::EnteringUrl => self.handle_url_entry(&event)?,
            BrowserScreen::ManagingTabs => self.handle_tab_management(&event),
            BrowserScreen::ViewingHistory => self.handle_history(&event)?,
            BrowserScreen::Browsing => {
                if let Event::Key(key) = &event {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Left => {
                                self.tab_manager.activate_previous_tab();
                                return Ok(());
                            }
                            KeyCode::Right => {
                                self.tab_manager.activate_next_tab();
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                }
                self.tab_manager.handle_events(&event);
            }
        }
        Ok(())
    }

    fn start_url_entry(&mut self) {
        self.url_input.clear();
        self.selected_history = None;
        self.screen = BrowserScreen::EnteringUrl;
    }

    fn handle_url_entry(&mut self, event: &Event) -> io::Result<()> {
        let Event::Key(key) = event else {
            return Ok(());
        };
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.return_to_previous_screen();
            }
            KeyCode::Enter => {
                let address = self
                    .selected_history
                    .and_then(|index| self.history_suggestions().get(index).copied())
                    .map(|entry| entry.url.clone())
                    .unwrap_or_else(|| self.url_input.trim().to_string());
                if !address.is_empty() {
                    let url = normalize_url(&address);
                    self.visit_url(url)?;
                }
            }
            KeyCode::Backspace => {
                self.url_input.pop();
                self.selected_history = None;
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.url_input.push(character);
                self.selected_history = None;
            }
            KeyCode::Up | KeyCode::Down => {
                let count = self.history_suggestions().len();
                if count > 0 {
                    self.selected_history = Some(match (key.code, self.selected_history) {
                        (KeyCode::Up, Some(index)) => (index + count - 1) % count,
                        (KeyCode::Down, Some(index)) => (index + 1) % count,
                        (KeyCode::Up, None) => count - 1,
                        (KeyCode::Down, None) => 0,
                        _ => unreachable!(),
                    });
                }
            }
            KeyCode::Tab => {
                if let Some(suggestion) = self
                    .selected_history
                    .and_then(|index| self.history_suggestions().get(index).copied())
                {
                    self.url_input = suggestion.url.clone();
                    self.selected_history = None;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn history_suggestions(&self) -> Vec<&HistoryEntry> {
        let query = self.url_input.trim().to_lowercase();
        let mut seen_urls = HashSet::new();
        self.history
            .iter()
            .filter(|entry| {
                (query.is_empty()
                    || entry.url.to_lowercase().contains(&query)
                    || entry.title.to_lowercase().contains(&query))
                    && seen_urls.insert(entry.url.as_str())
            })
            .take(5)
            .collect()
    }

    fn visit_url(&mut self, url: String) -> io::Result<()> {
        self.tab_manager.open_tab(url.clone(), url.clone());
        let title = self
            .tab_manager
            .active_tab()
            .map(|tab| tab.title().to_string())
            .unwrap_or_else(|| url.clone());
        self.history.insert(
            0,
            HistoryEntry {
                url,
                title,
                visited_at_unix: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                    .as_secs(),
            },
        );
        self.save_history()?;
        self.url_input.clear();
        self.selected_history = None;
        self.screen = BrowserScreen::Browsing;
        Ok(())
    }

    fn save_history(&self) -> io::Result<()> {
        if let Some(parent) = self.history_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_vec_pretty(&self.history)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(&self.history_path, contents)
    }

    fn handle_history(&mut self, event: &Event) -> io::Result<()> {
        let Event::Key(key) = event else {
            return Ok(());
        };
        if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
            self.return_to_previous_screen();
        }
        Ok(())
    }

    fn return_to_previous_screen(&mut self) {
        self.screen = if self.tab_manager.active_tab().is_some() {
            BrowserScreen::Browsing
        } else {
            BrowserScreen::EnteringUrl
        };
    }

    fn handle_tab_management(&mut self, event: &Event) {
        let Event::Key(key) = event else {
            return;
        };
        if key.kind != KeyEventKind::Press {
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.screen = if self.tab_manager.active_tab().is_some() {
                    BrowserScreen::Browsing
                } else {
                    BrowserScreen::EnteringUrl
                };
            }
            KeyCode::Up => {
                self.tabs_selection = self.tabs_selection.saturating_sub(1);
            }
            KeyCode::Down => {
                self.tabs_selection = self
                    .tabs_selection
                    .saturating_add(1)
                    .min(self.tab_manager.tab_count().saturating_sub(1));
            }
            KeyCode::Enter => {
                if self.tab_manager.activate_tab_at(self.tabs_selection) {
                    self.screen = BrowserScreen::Browsing;
                }
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                self.tab_manager.close_tab_at(self.tabs_selection);
                self.sync_tabs_selection();
                if self.tab_manager.active_tab().is_none() {
                    self.screen = BrowserScreen::EnteringUrl;
                }
            }
            KeyCode::Char('n') => self.start_url_entry(),
            _ => {}
        }
    }

    fn sync_tabs_selection(&mut self) {
        self.tabs_selection = self
            .tab_manager
            .active_tab_index()
            .unwrap_or(0)
            .min(self.tab_manager.tab_count().saturating_sub(1));
    }

    fn render(&self, frame: &mut Frame) {
        match self.screen {
            BrowserScreen::EnteringUrl => self.render_url_entry(frame),
            BrowserScreen::ManagingTabs => self.render_tabs(frame),
            BrowserScreen::ViewingHistory => self.render_history(frame),
            BrowserScreen::Browsing => self.render_browser(frame),
        }
    }

    fn render_browser(&self, frame: &mut Frame) {
        let [tab_row, content, footer] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .areas(frame.area());
        let tabs = self
            .tab_manager
            .tabs()
            .iter()
            .enumerate()
            .flat_map(|(index, tab)| {
                let active = Some(index) == self.tab_manager.active_tab_index();
                let style = if active {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray).bg(Color::DarkGray)
                };
                let label = if active {
                    format!(" {}  ·  {} ", tab.title(), tab.url())
                } else {
                    format!(" {} ", tab.title())
                };
                [Span::styled(label, style), Span::raw(" ")]
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(Line::from(tabs)).style(Style::default().bg(Color::Black)),
            tab_row,
        );
        if let Some(tab) = self.tab_manager.active_tab() {
            frame.render_widget(tab, content);
        } else {
            self.render_url_entry(frame);
        }
        frame.render_widget(
            Paragraph::new("Ctrl+L New URL   Ctrl+H History   Ctrl+T Manage tabs   Ctrl+W Close tab   ←/→ Switch tabs   Ctrl+Q Quit")
                .style(Style::default().fg(Color::DarkGray)),
            footer,
        );
    }

    fn render_url_entry(&self, frame: &mut Frame) {
        let suggestions = self.history_suggestions();
        let suggestion_height = if suggestions.is_empty() {
            0
        } else {
            suggestions.len() as u16 + 1
        };
        let area = centered_area(frame.area(), 76, 11 + suggestion_height);
        let [title, subtitle, input, suggestion_area, hints] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Length(suggestion_height),
                Constraint::Length(2),
            ])
            .margin(1)
            .areas(area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .title(" TUI Browser ")
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
            area,
        );
        frame.render_widget(
            Paragraph::new("Where would you like to go?")
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            title,
        );
        frame.render_widget(
            Paragraph::new("Enter an HTTP address to open it in a new tab.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray)),
            subtitle,
        );
        let display = format!("{}█", self.url_input);
        frame.render_widget(
            Paragraph::new(display)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .border_style(Style::default().fg(Color::DarkGray))
                        .title(" URL "),
                )
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: false }),
            input,
        );
        if !suggestions.is_empty() {
            let [label, list] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(suggestions.len() as u16),
                ])
                .areas(suggestion_area);
            frame.render_widget(
                Paragraph::new("History suggestions").style(Style::default().fg(Color::DarkGray)),
                label,
            );
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Length(1); suggestions.len()])
                .split(list);
            for (index, (entry, row)) in suggestions.iter().zip(rows.iter()).enumerate() {
                let style = if self.selected_history == Some(index) {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let label = if entry.title == entry.url {
                    entry.url.clone()
                } else {
                    format!("{}  —  {}", entry.title, entry.url)
                };
                frame.render_widget(Paragraph::new(label).style(style), *row);
            }
        }
        frame.render_widget(
            Paragraph::new(
                "↑/↓ Select    Tab Complete    Enter Open    Ctrl+H History    Esc Back",
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
            hints,
        );
    }

    fn render_tabs(&self, frame: &mut Frame) {
        let area = centered_area(frame.area(), 80, 20);
        let [list_area, footer] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .margin(1)
            .areas(area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .title(" Open tabs ")
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
            area,
        );

        let lines = self
            .tab_manager
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let marker = if Some(index) == self.tab_manager.active_tab_index() {
                    "●"
                } else {
                    " "
                };
                format!("{marker} {}  {}", tab.title(), tab.url())
            })
            .collect::<Vec<_>>();
        if lines.is_empty() {
            frame.render_widget(
                Paragraph::new("No tabs are open. Press n to open a URL.")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Gray)),
                list_area,
            );
        } else {
            let selected = self.tabs_selection.min(lines.len() - 1);
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Length(1); lines.len()])
                .split(list_area);
            for (index, (line, row)) in lines.iter().zip(rows.iter()).enumerate() {
                let style = if index == selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                frame.render_widget(Paragraph::new(line.as_str()).style(style), *row);
            }
        }
        frame.render_widget(
            Paragraph::new("↑/↓ Select   Enter Switch   x Close   n New URL   Esc Back")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray)),
            footer,
        );
    }

    fn render_history(&self, frame: &mut Frame) {
        let area = centered_area(frame.area(), 100, 24);
        let [entries, footer] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .margin(1)
            .areas(area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .title(" Browsing history ")
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
            area,
        );
        let content = if self.history.is_empty() {
            "No browsing history yet.".to_string()
        } else {
            self.history
                .iter()
                .map(|entry| {
                    format!(
                        "{}  (Unix time: {})\n  {}",
                        entry.title, entry.visited_at_unix, entry.url
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        frame.render_widget(
            Paragraph::new(content)
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: false }),
            entries,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "History file: {}    Esc Back",
                self.history_path.display()
            ))
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: false }),
            footer,
        );
    }
}

fn history_file_path() -> io::Result<PathBuf> {
    let data_dir = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "could not find the user data directory",
                )
            })?
    } else if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(xdg_data_home)
    } else {
        PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not find the user data directory",
            )
        })?)
        .join(".local")
        .join("share")
    };
    Ok(data_dir.join("tui-browser").join("history.json"))
}

fn centered_area(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn normalize_url(input: &str) -> String {
    if input.contains("://") {
        input.to_string()
    } else {
        format!("http://{input}")
    }
}

fn is_history_shortcut(key: &crossterm::event::KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(
            key.code,
            KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Backspace
        )
}

#[cfg(test)]
mod tests {
    use super::{Browser, BrowserScreen, HistoryEntry, normalize_url};
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEST_FILE: AtomicUsize = AtomicUsize::new(0);

    fn test_browser() -> Browser {
        let history_path = std::env::temp_dir().join(format!(
            "tui-browser-history-test-{}-{}.json",
            std::process::id(),
            NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        Browser::with_history_file(history_path).unwrap()
    }

    #[test]
    fn url_without_scheme_defaults_to_http() {
        assert_eq!(normalize_url("example.com/path"), "http://example.com/path");
    }

    #[test]
    fn url_with_scheme_is_preserved() {
        assert_eq!(normalize_url("http://example.com"), "http://example.com");
    }

    #[test]
    fn entering_urls_opens_each_address_in_a_new_tab() {
        let mut browser = test_browser();
        assert_eq!(browser.screen, BrowserScreen::EnteringUrl);
        assert_eq!(browser.tab_manager.tab_count(), 0);

        browser.url_input = "https://first.invalid".to_string();
        browser
            .handle_url_entry(&Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )))
            .unwrap();
        browser.start_url_entry();
        browser.url_input = "https://second.invalid".to_string();
        browser
            .handle_url_entry(&Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )))
            .unwrap();

        assert_eq!(browser.tab_manager.tab_count(), 2);
        assert_eq!(
            browser.tab_manager.active_tab().map(|tab| tab.url()),
            Some("https://second.invalid")
        );
        assert_eq!(browser.history[0].url, "https://second.invalid");
        assert_eq!(browser.history[0].title, "https://second.invalid");
        assert_eq!(browser.history[1].url, "https://first.invalid");
        assert_eq!(browser.history[1].title, "https://first.invalid");
        let _ = std::fs::remove_file(&browser.history_path);
    }

    #[test]
    fn history_suggestions_match_case_insensitively_and_keep_recent_order() {
        let mut browser = test_browser();
        browser.history = vec![
            HistoryEntry {
                url: "https://example.com/page".to_string(),
                title: "New Example Landing".to_string(),
                visited_at_unix: 0,
            },
            HistoryEntry {
                url: "https://example.com/old".to_string(),
                title: "Old Example".to_string(),
                visited_at_unix: 0,
            },
            HistoryEntry {
                url: "https://elsewhere.invalid".to_string(),
                title: "Elsewhere".to_string(),
                visited_at_unix: 0,
            },
        ];
        browser.url_input = "LANDING".to_string();

        assert_eq!(
            browser
                .history_suggestions()
                .iter()
                .map(|entry| entry.url.as_str())
                .collect::<Vec<_>>(),
            ["https://example.com/page"]
        );
    }

    #[test]
    fn empty_url_input_shows_recent_distinct_history_entries() {
        let mut browser = test_browser();
        browser.history = vec![
            HistoryEntry {
                url: "https://recent.invalid".to_string(),
                title: "Recent".to_string(),
                visited_at_unix: 30,
            },
            HistoryEntry {
                url: "https://recent.invalid".to_string(),
                title: "Recent".to_string(),
                visited_at_unix: 20,
            },
            HistoryEntry {
                url: "https://older.invalid".to_string(),
                title: "Older".to_string(),
                visited_at_unix: 10,
            },
        ];

        assert_eq!(
            browser
                .history_suggestions()
                .iter()
                .map(|entry| entry.url.as_str())
                .collect::<Vec<_>>(),
            ["https://recent.invalid", "https://older.invalid"]
        );
    }

    #[test]
    fn selected_history_suggestion_can_be_completed_into_the_url_input() {
        let mut browser = test_browser();
        browser.history = vec![HistoryEntry {
            url: "https://site.invalid/page".to_string(),
            title: "Example".to_string(),
            visited_at_unix: 0,
        }];
        browser.url_input = "example".to_string();

        browser
            .handle_url_entry(&Event::Key(KeyEvent::new(
                KeyCode::Down,
                KeyModifiers::NONE,
            )))
            .unwrap();
        browser
            .handle_url_entry(&Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)))
            .unwrap();

        assert_eq!(browser.url_input, "https://site.invalid/page");
        assert_eq!(browser.selected_history, None);
    }

    #[test]
    fn history_is_loaded_from_and_saved_to_json() {
        let browser = test_browser();
        let path = browser.history_path.clone();
        drop(browser);
        let mut browser = Browser::with_history_file(path.clone()).unwrap();
        browser.history.push(HistoryEntry {
            url: "https://example.invalid".to_string(),
            title: "Example".to_string(),
            visited_at_unix: 123,
        });
        browser.save_history().unwrap();

        let loaded = Browser::with_history_file(path.clone()).unwrap();
        assert_eq!(loaded.history[0].title, "Example");
        assert_eq!(loaded.history[0].visited_at_unix, 123);
        let file = std::fs::read_to_string(&path).unwrap();
        assert!(file.contains("\"visited_at_unix\": 123"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ctrl_h_backspace_encoding_opens_history_preview() {
        let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL);
        assert!(super::is_history_shortcut(&key));
        assert!(super::is_history_shortcut(&KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::CONTROL
        )));
    }
}
