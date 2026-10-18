
use crate::css::Stylesheet;
use crate::widgets::*;
use crate::xml_widget_parser::XmlParser;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use crossterm::event::{Event, KeyCode, KeyEvent};

pub struct Engine {
    widgets: Vec<Widget>,
    stylesheet: Stylesheet,
    focused_index: Option<usize>,
}

impl Engine {
    pub fn load(xml: &str) -> Self {
        let (widgets, stylesheet) = match XmlParser::parse(xml) {
            Ok(doc) => (doc.widgets, doc.stylesheet),
            Err(_) => (Vec::new(), Stylesheet::default()),
        };
        let mut engine = Self {
            widgets,
            stylesheet,
            focused_index: None,
        };
        engine.focus_first_focusable();
        engine
    }

    fn get_all_focusable_widgets(&self) -> Vec<(Vec<usize>, &Widget)> {
        let mut focusable = Vec::new();
        for (i, widget) in self.widgets.iter().enumerate() {
            Self::collect_focusable(widget, vec![i], &mut focusable);
        }
        focusable
    }

    fn collect_focusable<'a>(widget: &'a Widget, path: Vec<usize>, result: &mut Vec<(Vec<usize>, &'a Widget)>) {
        match widget {
            Widget::Button(_) | Widget::Input(_) => {
                result.push((path, widget));
            }
            Widget::Div(div) => {
                for (i, child) in div.children.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(i);
                    Self::collect_focusable(child, child_path, result);
                }
            }
            Widget::Text(_) => {}
        }
    }

    fn focus_first_focusable(&mut self) {
        if !self.get_all_focusable_widgets().is_empty() {
            self.focused_index = Some(0);
            self.update_focus();
        }
    }

    fn update_focus(&mut self) {
        // Resolve the target path before taking a mutable borrow of the widgets.
        let target_path = self
            .focused_index
            .and_then(|index| self.get_all_focusable_widgets().get(index).map(|(path, _)| path.clone()));

        // Clear all focus
        for widget in &mut self.widgets {
            Self::clear_focus(widget);
        }

        // Set focus on selected widget
        if let Some(path) = target_path {
            Self::set_focus_by_path(&mut self.widgets, &path);
        }
    }

    fn clear_focus(widget: &mut Widget) {
        match widget {
            Widget::Button(btn) => btn.focused = false,
            Widget::Input(input) => input.focused = false,
            Widget::Div(div) => {
                for child in &mut div.children {
                    Self::clear_focus(child);
                }
            }
            Widget::Text(_) => {}
        }
    }

    fn set_focus_by_path(widgets: &mut [Widget], path: &[usize]) {
        if path.is_empty() {
            return;
        }

        if let Some(widget) = widgets.get_mut(path[0]) {
            if path.len() == 1 {
                // This is the target widget
                match widget {
                    Widget::Button(btn) => btn.focused = true,
                    Widget::Input(input) => input.focused = true,
                    _ => {}
                }
            } else if let Widget::Div(div) = widget {
                // Navigate deeper
                Self::set_focus_by_path(&mut div.children, &path[1..]);
            }
        }
    }

    pub fn handle_event(&mut self, event: &Event) {
        if let Event::Key(key_event) = event {
            self.handle_key_event(key_event);
        }
    }

    fn handle_key_event(&mut self, key: &KeyEvent) {
        let focusable_count = self.get_all_focusable_widgets().len();

        match key.code {
            KeyCode::Tab => {
                // Move focus forward
                if focusable_count > 0 {
                    self.focused_index = Some(
                        self.focused_index
                            .map(|i| (i + 1) % focusable_count)
                            .unwrap_or(0)
                    );
                    self.update_focus();
                }
            }
            KeyCode::BackTab => {
                // Move focus backward
                if focusable_count > 0 {
                    self.focused_index = Some(
                        self.focused_index
                            .map(|i| if i == 0 { focusable_count - 1 } else { i - 1 })
                            .unwrap_or(0)
                    );
                    self.update_focus();
                }
            }
            KeyCode::Enter => {
                // Interact with focused widget
                self.handle_widget_action();
            }
            KeyCode::Char(c) => {
                // Handle text input for focused input widget
                self.handle_text_input(c);
            }
            KeyCode::Backspace => {
                self.handle_backspace();
            }
            _ => {}
        }
    }

    fn handle_widget_action(&self) {
        let focusable = self.get_all_focusable_widgets();
        if let Some(index) = self.focused_index {
            if let Some((_, widget)) = focusable.get(index) {
                match widget {
                    Widget::Button(btn) => {
                        // Trigger button action
                        println!("Button clicked: {}", btn.label);
                        // You can emit events or call callbacks here
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_text_input(&mut self, c: char) {
        if let Some(index) = self.focused_index {
            if let Some(path) = self.get_all_focusable_widgets().get(index).map(|(path, _)| path.clone()) {
                Self::input_text_by_path(&mut self.widgets, &path, c);
            }
        }
    }

    fn handle_backspace(&mut self) {
        if let Some(index) = self.focused_index {
            if let Some(path) = self.get_all_focusable_widgets().get(index).map(|(path, _)| path.clone()) {
                Self::backspace_by_path(&mut self.widgets, &path);
            }
        }
    }

    fn input_text_by_path(widgets: &mut [Widget], path: &[usize], c: char) {
        if path.is_empty() {
            return;
        }

        if let Some(widget) = widgets.get_mut(path[0]) {
            if path.len() == 1 {
                if let Widget::Input(input) = widget {
                    input.value.push(c);
                }
            } else if let Widget::Div(div) = widget {
                Self::input_text_by_path(&mut div.children, &path[1..], c);
            }
        }
    }

    fn backspace_by_path(widgets: &mut [Widget], path: &[usize]) {
        if path.is_empty() {
            return;
        }

        if let Some(widget) = widgets.get_mut(path[0]) {
            if path.len() == 1 {
                if let Widget::Input(input) = widget {
                    input.value.pop();
                }
            } else if let Widget::Div(div) = widget {
                Self::backspace_by_path(&mut div.children, &path[1..]);
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.widgets.is_empty() {
            return;
        }

        // Simple vertical layout for the top-level widgets.
        let widget_height = area.height / self.widgets.len() as u16;
        for (i, widget) in self.widgets.iter().enumerate() {
            let widget_area = Rect {
                x: area.x,
                y: area.y + (i as u16 * widget_height),
                width: area.width,
                height: widget_height,
            };
            widget.render(widget_area, buf, &self.stylesheet);
        }
    }
}