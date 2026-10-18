// css.rs
//
// A tiny CSS-like styling layer. It supports:
//   * selectors:   tag ("input"), id ("#submit"), class (".label"),
//                  and the `:focus` pseudo-class on any of the above.
//   * declarations: color, background / background-color, bold, align,
//                  border, border-color, title, padding, margin, width, height.
//   * colors:      named (white, blue, ...) and hex (#rrggbb / #rgb).
//   * inline styles via a `style="..."` attribute.
//
// Resolution is a simple source-order cascade: every matching rule is merged
// in the order it appears, and inline styles always win.

use ratatui::layout::Alignment;
use ratatui::style::Color;

/// A resolved set of style properties. `None` means "not specified"; callers
/// fall back to their own defaults for anything left unset.
#[derive(Debug, Clone, Default)]
pub struct ComputedStyle {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: Option<bool>,
    pub align: Option<Alignment>,
    pub border: Option<bool>,
    pub border_color: Option<Color>,
    pub title: Option<String>,
    pub padding: Option<u16>,
    pub margin: Option<u16>,
    pub width: Option<u16>,
    pub height: Option<u16>,
}

impl ComputedStyle {
    /// Overlay `other` on top of `self`: any property set in `other` wins.
    pub fn merge(&mut self, other: &ComputedStyle) {
        if other.fg.is_some() {
            self.fg = other.fg;
        }
        if other.bg.is_some() {
            self.bg = other.bg;
        }
        if other.bold.is_some() {
            self.bold = other.bold;
        }
        if other.align.is_some() {
            self.align = other.align;
        }
        if other.border.is_some() {
            self.border = other.border;
        }
        if other.border_color.is_some() {
            self.border_color = other.border_color;
        }
        if other.title.is_some() {
            self.title = other.title.clone();
        }
        if other.padding.is_some() {
            self.padding = other.padding;
        }
        if other.margin.is_some() {
            self.margin = other.margin;
        }
        if other.width.is_some() {
            self.width = other.width;
        }
        if other.height.is_some() {
            self.height = other.height;
        }
    }

    /// Parse a declaration block body, e.g. `color: white; bold: true`.
    pub fn parse_declarations(body: &str) -> ComputedStyle {
        let mut style = ComputedStyle::default();
        for decl in body.split(';') {
            let decl = decl.trim();
            if decl.is_empty() {
                continue;
            }
            let Some((prop, value)) = decl.split_once(':') else {
                continue;
            };
            style.apply(prop.trim(), value.trim());
        }
        style
    }

    fn apply(&mut self, prop: &str, value: &str) {
        match prop {
            "color" => self.fg = parse_color(value),
            "background" | "background-color" => self.bg = parse_color(value),
            "bold" => self.bold = Some(parse_bool(value)),
            "align" | "text-align" => self.align = parse_align(value),
            "border" => self.border = Some(parse_bool(value)),
            "border-color" => self.border_color = parse_color(value),
            "title" => self.title = Some(value.trim_matches(['"', '\'']).to_string()),
            "padding" => self.padding = value.parse().ok(),
            "margin" => self.margin = value.parse().ok(),
            "width" => self.width = value.parse().ok(),
            "height" => self.height = value.parse().ok(),
            _ => {}
        }
    }
}

fn parse_bool(value: &str) -> bool {
    matches!(value, "true" | "yes" | "1")
}

fn parse_align(value: &str) -> Option<Alignment> {
    match value {
        "left" => Some(Alignment::Left),
        "center" | "centre" => Some(Alignment::Center),
        "right" => Some(Alignment::Right),
        _ => None,
    }
}

fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex_color(hex);
    }
    match value.to_ascii_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "white" => Some(Color::White),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        "reset" | "none" | "transparent" => Some(Color::Reset),
        _ => None,
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let expanded;
    let hex = if hex.len() == 3 {
        // #rgb -> #rrggbb
        expanded = hex.chars().flat_map(|c| [c, c]).collect::<String>();
        expanded.as_str()
    } else {
        hex
    };
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// What an element exposes for selector matching.
pub struct ElementInfo<'a> {
    pub tag: &'a str,
    pub id: Option<&'a str>,
    pub classes: &'a [String],
    pub focused: bool,
}

#[derive(Debug, Clone)]
enum SelectorKind {
    Tag(String),
    Id(String),
    Class(String),
}

#[derive(Debug, Clone)]
struct Selector {
    kind: SelectorKind,
    focus: bool,
}

impl Selector {
    fn parse(text: &str) -> Option<Selector> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let (base, focus) = match text.strip_suffix(":focus") {
            Some(rest) => (rest, true),
            None => (text, false),
        };
        let kind = if let Some(id) = base.strip_prefix('#') {
            SelectorKind::Id(id.to_string())
        } else if let Some(class) = base.strip_prefix('.') {
            SelectorKind::Class(class.to_string())
        } else {
            SelectorKind::Tag(base.to_string())
        };
        Some(Selector { kind, focus })
    }

    fn matches(&self, el: &ElementInfo) -> bool {
        if self.focus && !el.focused {
            return false;
        }
        match &self.kind {
            SelectorKind::Tag(t) => el.tag == t,
            SelectorKind::Id(id) => el.id == Some(id.as_str()),
            SelectorKind::Class(c) => el.classes.iter().any(|cls| cls == c),
        }
    }
}

#[derive(Debug, Clone)]
struct Rule {
    selectors: Vec<Selector>,
    style: ComputedStyle,
}

/// A parsed collection of CSS rules.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    rules: Vec<Rule>,
}

impl Stylesheet {
    /// Parse a stylesheet from CSS text. Comments (`/* ... */`) are stripped.
    pub fn parse(css: &str) -> Stylesheet {
        let css = strip_comments(css);
        let mut rules = Vec::new();
        let mut rest = css.as_str();

        while let Some(open) = rest.find('{') {
            let prelude = &rest[..open];
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                break;
            };
            let body = &after[..close];

            let selectors: Vec<Selector> = prelude
                .split(',')
                .filter_map(Selector::parse)
                .collect();
            if !selectors.is_empty() {
                rules.push(Rule {
                    selectors,
                    style: ComputedStyle::parse_declarations(body),
                });
            }

            rest = &after[close + 1..];
        }

        Stylesheet { rules }
    }

    /// Resolve the style for an element by merging every matching rule in
    /// source order.
    pub fn resolve(&self, el: &ElementInfo) -> ComputedStyle {
        let mut style = ComputedStyle::default();
        for rule in &self.rules {
            if rule.selectors.iter().any(|s| s.matches(el)) {
                style.merge(&rule.style);
            }
        }
        style
    }
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out, // unterminated comment: drop the remainder
        }
    }
    out.push_str(rest);
    out
}
