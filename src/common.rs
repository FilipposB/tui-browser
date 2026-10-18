use crossterm::event::Event;

pub trait EventHandling {
    fn handle_events(&mut self, event: &Event);
}