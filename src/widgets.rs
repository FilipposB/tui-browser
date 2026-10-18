// widgets.rs
use crate::css::{ComputedStyle, ElementInfo, Stylesheet};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget as RatatuiWidget};

#[derive(Debug, Clone)]
pub enum Widget {
    Div(DivWidget),
    Form(FormWidget),
    Button(ButtonWidget),
    Input(InputWidget),
    Text(TextWidget),
    Divider(DividerWidget),
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
pub struct FormWidget {
    pub attrs: StyleAttrs,
    pub onsubmit: Option<String>,
    pub children: Vec<Widget>,
}

#[derive(Debug, Clone)]
pub struct ButtonWidget {
    pub attrs: StyleAttrs,
    pub label: String,
    pub onclick: Option<String>,
    pub focused: bool,
}

#[derive(Debug, Clone)]
pub struct InputWidget {
    pub attrs: StyleAttrs,
    pub value: String,
    pub label: Option<String>,
    pub password: bool,
    pub focused: bool,
}

#[derive(Debug, Clone)]
pub struct DividerWidget {
    pub attrs: StyleAttrs,
    pub label: Option<String>,
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
            Widget::Form(_) => "form",
            Widget::Button(_) => "button",
            Widget::Input(_) => "input",
            Widget::Text(_) => "text",
            Widget::Divider(_) => "divider",
        }
    }

    fn attrs(&self) -> &StyleAttrs {
        match self {
            Widget::Div(w) => &w.attrs,
            Widget::Form(w) => &w.attrs,
            Widget::Button(w) => &w.attrs,
            Widget::Input(w) => &w.attrs,
            Widget::Text(w) => &w.attrs,
            Widget::Divider(w) => &w.attrs,
        }
    }

    fn focused(&self) -> bool {
        match self {
            Widget::Button(w) => w.focused,
            Widget::Input(w) => w.focused,
            Widget::Div(_) | Widget::Form(_) | Widget::Text(_) | Widget::Divider(_) => false,
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
            Widget::Form(form) => form.content_height(sheet, &style),
            Widget::Button(_) => 3,
            Widget::Input(input) if input.label.is_some() => 3,
            _ => 1,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, sheet: &Stylesheet) {
        let style = self.resolved_style(sheet);
        match self {
            Widget::Div(div) => div.render(area, buf, sheet, &style),
            Widget::Form(form) => form.render(area, buf, sheet, &style),
            Widget::Button(button) => button.render(area, buf, &style),
            Widget::Input(input) => input.render(area, buf, &style),
            Widget::Text(text) => text.render(area, buf, &style),
            Widget::Divider(divider) => divider.render(area, buf, &style),
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
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

impl DivWidget {
    /// Sum of children heights plus their top margins — used when a div has no
    /// explicit height. Does not include the div's own border/padding.
    fn content_height(&self, sheet: &Stylesheet, _style: &ComputedStyle) -> u16 {
        children_height(&self.children, sheet)
    }

    fn render(&self, area: Rect, buf: &mut Buffer, sheet: &Stylesheet, style: &ComputedStyle) {
        render_children(&self.children, area, buf, sheet, style);
    }
}

impl FormWidget {
    fn content_height(&self, sheet: &Stylesheet, _style: &ComputedStyle) -> u16 {
        children_height(&self.children, sheet)
    }

    fn render(&self, area: Rect, buf: &mut Buffer, sheet: &Stylesheet, style: &ComputedStyle) {
        render_children(&self.children, area, buf, sheet, style);
    }
}

fn children_height(children: &[Widget], sheet: &Stylesheet) -> u16 {
    children
        .iter()
        .map(|child| {
            child.resolved_style(sheet).margin.unwrap_or(0) + child.preferred_height(sheet)
        })
        .sum()
}

fn render_children(
    children: &[Widget],
    area: Rect,
    buf: &mut Buffer,
    sheet: &Stylesheet,
    style: &ComputedStyle,
) {
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
    for child in children {
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

impl ButtonWidget {
    fn render(&self, area: Rect, buf: &mut Buffer, style: &ComputedStyle) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let label_width = u16::try_from(self.label.chars().count()).unwrap_or(u16::MAX);
        let width = style
            .width
            .unwrap_or_else(|| label_width.saturating_add(6))
            .min(area.width);
        let height = area.height.min(3);
        let button_area = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        };
        let fg = style.fg.unwrap_or(ratatui::style::Color::White);
        let bg = style.bg.unwrap_or(ratatui::style::Color::Reset);
        let default_border = if self.focused {
            ratatui::style::Color::Cyan
        } else {
            ratatui::style::Color::DarkGray
        };
        let border = style.border_color.unwrap_or(default_border);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(border))
            .style(Style::default().fg(fg).bg(bg));
        let content_area = block.inner(button_area);
        RatatuiWidget::render(block, button_area, buf);
        if content_area.height > 0 && content_area.width > 0 {
            RatatuiWidget::render(
                Paragraph::new(format!(" {} ", self.label))
                    .style(Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD))
                    .alignment(Alignment::Center),
                content_area,
                buf,
            );
        }
    }
}

impl InputWidget {
    fn render(&self, area: Rect, buf: &mut Buffer, style: &ComputedStyle) {
        if let Some(label) = &self.label {
            if area.height == 0 {
                return;
            }
            let mut label_style = style.clone();
            label_style.fg = style
                .border_color
                .or(style.fg)
                .or(Some(ratatui::style::Color::Cyan));
            label_style.bold = Some(true);
            let label_area = Rect { height: 1, ..area };
            RatatuiWidget::render(
                Paragraph::new(label.as_str()).style(to_ratatui_style(&label_style)),
                label_area,
                buf,
            );
            if area.height < 2 {
                return;
            }
            let field_area = Rect {
                y: area.y + 1,
                height: 1,
                ..area
            };
            self.render_value(field_area, buf, style);
            if area.height >= 3 {
                let divider_color = style
                    .border_color
                    .or(style.fg)
                    .unwrap_or(ratatui::style::Color::DarkGray);
                let divider_style = Style::default().fg(divider_color);
                let divider_area = Rect {
                    y: area.y + 2,
                    height: 1,
                    ..area
                };
                RatatuiWidget::render(
                    Paragraph::new("─".repeat(usize::from(area.width))).style(divider_style),
                    divider_area,
                    buf,
                );
            }
            return;
        }

        self.render_value(area, buf, style);
    }

    fn render_value(&self, area: Rect, buf: &mut Buffer, style: &ComputedStyle) {
        let value = if self.password {
            "•".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        };
        let text = if self.focused {
            format!("{value}\u{2588}")
        } else {
            value
        };
        let paragraph = Paragraph::new(text)
            .style(to_ratatui_style(style))
            .alignment(style.align.unwrap_or(Alignment::Left));
        RatatuiWidget::render(paragraph, area, buf);
    }
}

impl DividerWidget {
    fn render(&self, area: Rect, buf: &mut Buffer, style: &ComputedStyle) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let line_color = style
            .border_color
            .or(style.fg)
            .unwrap_or(ratatui::style::Color::DarkGray);
        let line_style = Style::default().fg(line_color);
        let text = match self.label.as_deref().filter(|label| !label.is_empty()) {
            Some(label) => {
                let label = format!(" {label} ");
                let remaining = area
                    .width
                    .saturating_sub(u16::try_from(label.chars().count()).unwrap_or(u16::MAX));
                let left = remaining / 2;
                let right = remaining - left;
                format!(
                    "{}{}{}",
                    "─".repeat(usize::from(left)),
                    label,
                    "─".repeat(usize::from(right))
                )
            }
            None => "─".repeat(usize::from(area.width)),
        };
        RatatuiWidget::render(
            Paragraph::new(text)
                .style(line_style)
                .alignment(Alignment::Center),
            Rect { height: 1, ..area },
            buf,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{ButtonWidget, DividerWidget, InputWidget};
    use crate::css::ComputedStyle;
    use crate::widgets::StyleAttrs;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn password_input_renders_masked_value_and_cursor() {
        let input = InputWidget {
            attrs: StyleAttrs::default(),
            value: "secret".to_string(),
            label: None,
            password: true,
            focused: true,
        };
        let area = Rect::new(0, 0, 10, 1);
        let mut buffer = Buffer::empty(area);

        input.render(area, &mut buffer, &ComputedStyle::default());

        let rendered: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert_eq!(rendered.trim_end(), "••••••█");
        assert!(!rendered.contains("secret"));
    }

    #[test]
    fn labeled_input_renders_label_value_and_separator() {
        let input = InputWidget {
            attrs: StyleAttrs::default(),
            value: "alice".to_string(),
            label: Some("Username".to_string()),
            password: false,
            focused: true,
        };
        let area = Rect::new(0, 0, 12, 3);
        let mut buffer = Buffer::empty(area);

        input.render(area, &mut buffer, &ComputedStyle::default());

        let rendered: Vec<String> = buffer
            .content()
            .chunks(12)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect();
        assert_eq!(rendered[0].trim_end(), "Username");
        assert!(rendered[1].contains("alice█"));
        assert!(rendered[2].contains('─'));
    }

    #[test]
    fn divider_centers_its_label_between_lines() {
        let divider = DividerWidget {
            attrs: StyleAttrs::default(),
            label: Some("OR".to_string()),
        };
        let area = Rect::new(0, 0, 12, 1);
        let mut buffer = Buffer::empty(area);

        divider.render(area, &mut buffer, &ComputedStyle::default());

        let rendered: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("OR"));
        assert!(rendered.starts_with('─'));
        assert!(rendered.trim_end().ends_with('─'));
    }

    #[test]
    fn button_renders_as_rounded_control_with_focus_color() {
        let button = ButtonWidget {
            attrs: StyleAttrs::default(),
            label: "Submit".to_string(),
            onclick: None,
            focused: true,
        };
        let area = Rect::new(0, 0, 16, 3);
        let mut buffer = Buffer::empty(area);

        button.render(area, &mut buffer, &ComputedStyle::default());

        let rendered: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains('╭'));
        assert!(rendered.contains('╰'));
        assert!(rendered.contains("Submit"));
        assert_eq!(buffer[(2, 0)].fg, ratatui::style::Color::Cyan);
        assert_eq!(buffer[(3, 1)].fg, ratatui::style::Color::White);
        assert_eq!(buffer[(3, 1)].bg, ratatui::style::Color::Reset);
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
