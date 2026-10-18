// widgets.rs
use crate::css::{ComputedStyle, ElementInfo, Stylesheet};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget as RatatuiWidget};

#[derive(Debug, Clone)]
pub enum Widget {
    Div(DivWidget),
    Button(ButtonWidget),
    Input(InputWidget),
    Text(TextWidget),
}

/// Styling metadata shared by every element: an optional id, any classes, and
/// an inline `style="..."` block (highest precedence).
#[derive(Debug, Clone, Default)]
pub struct StyleAttrs {
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub inline: ComputedStyle,
}

#[derive(Debug, Clone)]
pub struct DivWidget {
    pub attrs: StyleAttrs,
    pub children: Vec<Widget>,
}

#[derive(Debug, Clone)]
pub struct ButtonWidget {
    pub attrs: StyleAttrs,
    pub label: String,
    pub focused: bool,
}

#[derive(Debug, Clone)]
pub struct InputWidget {
    pub attrs: StyleAttrs,
    pub value: String,
    pub focused: bool,
}

#[derive(Debug, Clone)]
pub struct TextWidget {
    pub attrs: StyleAttrs,
    pub value: String,
}

impl Widget {
    pub fn tag(&self) -> &'static str {
        match self {
            Widget::Div(_) => "div",
            Widget::Button(_) => "button",
            Widget::Input(_) => "input",
            Widget::Text(_) => "text",
        }
    }

    fn attrs(&self) -> &StyleAttrs {
        match self {
            Widget::Div(w) => &w.attrs,
            Widget::Button(w) => &w.attrs,
            Widget::Input(w) => &w.attrs,
            Widget::Text(w) => &w.attrs,
        }
    }

    fn focused(&self) -> bool {
        match self {
            Widget::Button(w) => w.focused,
            Widget::Input(w) => w.focused,
            Widget::Div(_) | Widget::Text(_) => false,
        }
    }

    /// Resolve this element's final style: cascade from the stylesheet, then
    /// overlay the inline style.
    pub fn resolved_style(&self, sheet: &Stylesheet) -> ComputedStyle {
        let attrs = self.attrs();
        let el = ElementInfo {
            tag: self.tag(),
            id: attrs.id.as_deref(),
            classes: &attrs.classes,
            focused: self.focused(),
        };
        let mut style = sheet.resolve(&el);
        style.merge(&attrs.inline);
        style
    }

    /// Height this element wants to occupy in a vertical layout.
    fn preferred_height(&self, sheet: &Stylesheet) -> u16 {
        let style = self.resolved_style(sheet);
        if let Some(h) = style.height {
            return h;
        }
        match self {
            // A div with no explicit height falls back to fitting its children.
            Widget::Div(div) => div.content_height(sheet, &style),
            _ => 1,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, sheet: &Stylesheet) {
        let style = self.resolved_style(sheet);
        match self {
            Widget::Div(div) => div.render(area, buf, sheet, &style),
            Widget::Button(button) => button.render(area, buf, &style),
            Widget::Input(input) => input.render(area, buf, &style),
            Widget::Text(text) => text.render(area, buf, &style),
        }
    }
}

/// Turn a `ComputedStyle` into a ratatui `Style` (fg/bg/bold only).
fn to_ratatui_style(style: &ComputedStyle) -> Style {
    let mut s = Style::default();
    if let Some(fg) = style.fg {
        s = s.fg(fg);
    }
    if let Some(bg) = style.bg {
        s = s.bg(bg);
    }
    if style.bold == Some(true) {
        s = s.add_modifier(Modifier::BOLD);
    }
    s
}

/// Center a box of the given width/height inside `area`, clamping to fit.
fn centered_box(area: Rect, width: Option<u16>, height: Option<u16>) -> Rect {
    let w = width.unwrap_or(area.width).min(area.width);
    let h = height.unwrap_or(area.height).min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

impl DivWidget {
    /// Sum of children heights plus their top margins — used when a div has no
    /// explicit height. Does not include the div's own border/padding.
    fn content_height(&self, sheet: &Stylesheet, _style: &ComputedStyle) -> u16 {
        self.children
            .iter()
            .map(|c| c.resolved_style(sheet).margin.unwrap_or(0) + c.preferred_height(sheet))
            .sum()
    }

    fn render(&self, area: Rect, buf: &mut Buffer, sheet: &Stylesheet, style: &ComputedStyle) {
        // A sized div becomes a centered box; otherwise it fills its area.
        let box_area = centered_box(area, style.width, style.height);
        if box_area.width == 0 || box_area.height == 0 {
            return;
        }

        // Background + optional border/title via a Block.
        let mut block = Block::default().style(to_ratatui_style(style));
        if style.border == Some(true) {
            let mut border_style = Style::default();
            if let Some(c) = style.border_color {
                border_style = border_style.fg(c);
            }
            block = block.borders(Borders::ALL).border_style(border_style);
            if let Some(title) = &style.title {
                block = block.title(title.clone());
            }
        }
        let inner = block.inner(box_area);
        RatatuiWidget::render(block, box_area, buf);

        // Apply padding inside the (possibly bordered) box.
        let pad = style.padding.unwrap_or(0);
        let content = Rect {
            x: inner.x + pad,
            y: inner.y + pad,
            width: inner.width.saturating_sub(pad * 2),
            height: inner.height.saturating_sub(pad * 2),
        };
        if content.width == 0 || content.height == 0 {
            return;
        }

        // Stack children vertically, honoring each child's top margin and height.
        let mut y = content.y;
        let bottom = content.y + content.height;
        for child in &self.children {
            let child_style = child.resolved_style(sheet);
            y += child_style.margin.unwrap_or(0);
            if y >= bottom {
                break;
            }
            let height = child.preferred_height(sheet).min(bottom - y);
            let child_area = Rect {
                x: content.x,
                y,
                width: content.width,
                height,
            };
            child.render(child_area, buf, sheet);
            y += height;
        }
    }
}

impl ButtonWidget {
    fn render(&self, area: Rect, buf: &mut Buffer, style: &ComputedStyle) {
        let text = format!("[ {} ]", self.label);
        let paragraph = Paragraph::new(text)
            .style(to_ratatui_style(style))
            .alignment(style.align.unwrap_or(Alignment::Center));
        RatatuiWidget::render(paragraph, area, buf);
    }
}

impl InputWidget {
    fn render(&self, area: Rect, buf: &mut Buffer, style: &ComputedStyle) {
        // Show a block cursor at the end while focused.
        let text = if self.focused {
            format!("{}\u{2588}", self.value)
        } else {
            self.value.clone()
        };
        let paragraph = Paragraph::new(text)
            .style(to_ratatui_style(style))
            .alignment(style.align.unwrap_or(Alignment::Left));
        RatatuiWidget::render(paragraph, area, buf);
    }
}

impl TextWidget {
    fn render(&self, area: Rect, buf: &mut Buffer, style: &ComputedStyle) {
        let paragraph = Paragraph::new(self.value.as_str())
            .style(to_ratatui_style(style))
            .alignment(style.align.unwrap_or(Alignment::Left));
        RatatuiWidget::render(paragraph, area, buf);
    }
}
