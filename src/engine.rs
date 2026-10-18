use crossterm::event::Event;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use crate::common::EventHandling;

pub struct Engine {
}

impl Engine {
    pub fn load(data: &str) -> Self {
        Engine {}
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {

    }
}

impl EventHandling for Engine {
    fn handle_events(&self, event: &Event) {
    }
}