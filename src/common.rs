use crossterm::event::Event;

pub trait EventHandling {
    fn handle_events(&self, event: &Event);
}