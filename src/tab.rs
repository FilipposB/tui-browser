use crossterm::event::Event;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use crate::common::EventHandling;
use crate::engine::Engine;

pub struct TabManager {
    active_tab: Option<usize>,
    opened_tabs: Vec<Tab>,
}

impl TabManager {

}

pub struct Tab {
    id: usize,
    title: String,
    url: String,
    engine: Engine
}

impl Widget for &Tab {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized
    {
        self.engine.render(area, buf);
    }
}

impl EventHandling for Tab {
    fn handle_events(&self, event: &Event) {
        match event {
            Event::Key(_) => {}
            _ => {}
        }
    }
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            active_tab: None,
            opened_tabs: Vec::new(),
        }
    }

    pub fn activate_tab(&mut self, tab_id: usize){
        self.active_tab = self.opened_tabs.iter().find(|tab| tab.id == tab_id).map(|tab| tab.id);
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_tab.and_then(|id| self.opened_tabs.get(id))
    }

    pub fn open_tab(&mut self, title: String, url: String) {
        let tab_id = self.opened_tabs.len();
        let tab = Tab {
            id: tab_id,
            title,
            url,
            engine: Engine::load("123")
        };
        self.opened_tabs.push(tab);
        self.activate_tab(tab_id);
    }

}

impl EventHandling for TabManager {
    fn handle_events(&self, event: &Event) {
        match event {
            Event::Key(_) => {}
            _ => {}
        }
        self.active_tab.and_then(|id| self.opened_tabs.get(id)).map(|tab| tab.handle_events(event));
    }
}