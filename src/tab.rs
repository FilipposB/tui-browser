use crate::common::EventHandling;
use crate::engine::Engine;
use crossterm::event::Event;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

pub struct TabManager {
    active_tab: Option<usize>,
    next_tab_id: usize,
    opened_tabs: Vec<Tab>,
}

pub struct Tab {
    id: usize,
    title: String,
    url: String,
    engine: Engine,
}

impl Tab {
    pub fn title(&self) -> &str {
        self.engine.document_title().unwrap_or(&self.title)
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Widget for &Tab {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.engine.render(area, buf);
    }
}

impl EventHandling for Tab {
    fn handle_events(&mut self, event: &Event) {
        self.engine.handle_event(event);
    }
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            active_tab: None,
            next_tab_id: 0,
            opened_tabs: Vec::new(),
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_tab
            .and_then(|id| self.opened_tabs.iter().find(|tab| tab.id == id))
    }

    pub fn active_tab_index(&self) -> Option<usize> {
        self.active_tab
            .and_then(|id| self.opened_tabs.iter().position(|tab| tab.id == id))
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.opened_tabs
    }

    pub fn tab_count(&self) -> usize {
        self.opened_tabs.len()
    }

    pub fn tab_strip(&self) -> String {
        self.opened_tabs
            .iter()
            .map(|tab| {
                let marker = if Some(tab.id) == self.active_tab {
                    "›"
                } else {
                    " "
                };
                format!(" {marker} {} ", tab.title())
            })
            .collect::<String>()
    }

    pub fn activate_tab_at(&mut self, index: usize) -> bool {
        let Some(tab) = self.opened_tabs.get(index) else {
            return false;
        };
        self.active_tab = Some(tab.id);
        true
    }

    pub fn activate_next_tab(&mut self) {
        if self.opened_tabs.is_empty() {
            return;
        }
        let next = self
            .active_tab_index()
            .map_or(0, |index| (index + 1) % self.opened_tabs.len());
        self.activate_tab_at(next);
    }

    pub fn activate_previous_tab(&mut self) {
        if self.opened_tabs.is_empty() {
            return;
        }
        let previous = self.active_tab_index().map_or(0, |index| {
            if index == 0 {
                self.opened_tabs.len() - 1
            } else {
                index - 1
            }
        });
        self.activate_tab_at(previous);
    }

    pub fn open_tab(&mut self, title: String, url: String) {
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;
        self.opened_tabs.push(Tab {
            id: tab_id,
            title,
            engine: Engine::load_url(&url),
            url,
        });
        self.active_tab = Some(tab_id);
    }

    pub fn close_active_tab(&mut self) {
        if let Some(index) = self.active_tab_index() {
            self.close_tab_at(index);
        }
    }

    pub fn close_tab_at(&mut self, index: usize) {
        if index >= self.opened_tabs.len() {
            return;
        }
        let was_active = self.active_tab == Some(self.opened_tabs[index].id);
        self.opened_tabs.remove(index);

        if self.opened_tabs.is_empty() {
            self.active_tab = None;
        } else if was_active {
            self.active_tab = Some(self.opened_tabs[index.min(self.opened_tabs.len() - 1)].id);
        }
    }
}

impl EventHandling for TabManager {
    fn handle_events(&mut self, event: &Event) {
        if let Some(id) = self.active_tab {
            if let Some(tab) = self.opened_tabs.iter_mut().find(|tab| tab.id == id) {
                tab.handle_events(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TabManager;
    use crate::engine::Engine;

    fn manager_with_tabs() -> TabManager {
        let mut manager = TabManager::new();
        manager.open_tab("First".to_string(), "https://first.invalid".to_string());
        manager.open_tab("Second".to_string(), "https://second.invalid".to_string());
        manager.open_tab("Third".to_string(), "https://third.invalid".to_string());
        manager
    }

    #[test]
    fn switching_tabs_keeps_active_tab_after_another_tab_is_closed() {
        let mut manager = manager_with_tabs();
        assert!(manager.activate_tab_at(2));

        manager.close_tab_at(0);

        assert_eq!(manager.active_tab().map(|tab| tab.title()), Some("Third"));
        assert_eq!(manager.active_tab_index(), Some(1));
    }

    #[test]
    fn closing_active_tab_selects_a_remaining_tab() {
        let mut manager = manager_with_tabs();
        assert!(manager.activate_tab_at(1));

        manager.close_active_tab();

        assert_eq!(manager.tab_count(), 2);
        assert_eq!(manager.active_tab().map(|tab| tab.title()), Some("Third"));
    }

    #[test]
    fn closing_last_tab_clears_active_tab() {
        let mut manager = TabManager::new();
        manager.open_tab("Only".to_string(), "https://only.invalid".to_string());

        manager.close_active_tab();

        assert_eq!(manager.tab_count(), 0);
        assert!(manager.active_tab().is_none());
    }

    #[test]
    fn tab_strip_uses_xml_title_instead_of_tab_number() {
        let mut manager = TabManager::new();
        let engine = Engine::load("<title>Dashboard</title>");
        manager.opened_tabs.push(super::Tab {
            id: 0,
            title: "https://example.invalid".to_string(),
            url: "https://example.invalid".to_string(),
            engine,
        });
        manager.active_tab = Some(0);

        assert_eq!(manager.tab_strip(), " › Dashboard ");
    }

    #[test]
    fn tab_strip_uses_url_as_the_fallback_name_without_a_document_title() {
        let mut manager = TabManager::new();
        manager.opened_tabs.push(super::Tab {
            id: 0,
            title: "https://example.invalid".to_string(),
            url: "https://example.invalid".to_string(),
            engine: Engine::load("<text value=\"page\"/>"),
        });
        manager.active_tab = Some(0);

        assert_eq!(manager.tab_strip(), " › https://example.invalid ");
    }
}
