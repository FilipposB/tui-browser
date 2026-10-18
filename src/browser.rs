use crate::tab::TabManager;
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers
};
use ratatui::DefaultTerminal;
use ratatui::Frame;
use crate::common::EventHandling;

pub struct Browser {
    should_quit: bool,
    tab_manager: TabManager,
}

impl Browser {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            tab_manager: TabManager::new(),
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        loop {
            self.handle_events();
            terminal.draw(self.render())?;
            if self.should_quit {
                break Ok(());
            }
        }
    }

    fn handle_events(&mut self) {
        let key = event::read().unwrap();
        self.tab_manager.handle_events(&key);
        match key {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
                    self.should_quit = true;
                    return;
                }
            }
            // handle other events
            _ => {}
        }
    }

    fn render(&self) -> impl Fn(&mut Frame) {
        move |frame: &mut Frame| match self.tab_manager.active_tab() {
            None => {}
            Some(tab) => frame.render_widget(tab, frame.area()),
        }
    }
}
