// xml_parser.rs
use crate::css::{ComputedStyle, Stylesheet};
use crate::widgets::*;
use quick_xml::events::{BytesStart, Event as XmlEvent};
use quick_xml::Reader;

pub struct XmlParser;

/// The outcome of parsing a document: the widget tree plus its stylesheet.
pub struct ParsedDocument {
    pub widgets: Vec<Widget>,
    pub stylesheet: Stylesheet,
    pub title: Option<String>,
}

impl XmlParser {
    pub fn parse(xml: &str) -> Result<ParsedDocument, String> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut widgets = Vec::new();
        let mut stylesheet = Stylesheet::default();
        let mut title = None;
        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(XmlEvent::Start(e)) => {
                    let tag = tag_name(&e);
                    if tag == "style" {
                        stylesheet = Self::parse_style_block(&mut reader)?;
                    } else if tag == "title" {
                        title = Some(Self::parse_title(&mut reader, &e)?);
                    } else if let Some(widget) = Self::parse_element(&mut reader, &e, &tag, false)?
                    {
                        widgets.push(widget);
                    }
                }
                Ok(XmlEvent::Empty(e)) => {
                    let tag = tag_name(&e);
                    if tag == "title" {
                        title = get_attr(&e, "value")?;
                    } else if let Some(widget) = Self::parse_element(&mut reader, &e, &tag, true)? {
                        widgets.push(widget);
                    }
                }
                Ok(XmlEvent::Eof) => break,
                Err(e) => return Err(format!("XML parse error: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(ParsedDocument {
            widgets,
            stylesheet,
            title,
        })
    }

    fn parse_title(reader: &mut Reader<&[u8]>, event: &BytesStart) -> Result<String, String> {
        let mut title = get_attr(event, "value")?.unwrap_or_default();
        if !title.is_empty() {
            let mut buf = Vec::new();
            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(XmlEvent::End(e)) if e.name().as_ref() == b"title" => break,
                    Ok(XmlEvent::Eof) => return Err("Unexpected EOF in <title>".to_string()),
                    Err(error) => return Err(format!("XML parse error: {error}")),
                    _ => {}
                }
                buf.clear();
            }
            return Ok(title);
        }

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(XmlEvent::Text(text)) => {
                    title.push_str(
                        &text
                            .unescape()
                            .map_err(|error| format!("Title text error: {error}"))?,
                    );
                }
                Ok(XmlEvent::End(e)) if e.name().as_ref() == b"title" => break,
                Ok(XmlEvent::Eof) => return Err("Unexpected EOF in <title>".to_string()),
                Err(error) => return Err(format!("XML parse error: {error}")),
                _ => {}
            }
            buf.clear();
        }
        Ok(title.trim().to_string())
    }

    /// Read the text content of a `<style>...</style>` element and parse it.
    fn parse_style_block(reader: &mut Reader<&[u8]>) -> Result<Stylesheet, String> {
        let mut css = String::new();
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(XmlEvent::Text(t)) => {
                    css.push_str(&t.unescape().map_err(|e| format!("CSS text error: {}", e))?);
                }
                // Ok(XmlEvent::End(e)) if tag_name(&e) == "style" => break,
                Ok(XmlEvent::Eof) => return Err("Unexpected EOF in <style>".to_string()),
                Err(e) => return Err(format!("XML parse error: {}", e)),
                _ => {}
            }
            buf.clear();
        }
        Ok(Stylesheet::parse(&css))
    }

    /// Dispatch a single element. For container elements (`div`) with a start
    /// tag, children are read until the matching end tag.
    fn parse_element(
        reader: &mut Reader<&[u8]>,
        event: &BytesStart,
        tag: &str,
        empty: bool,
    ) -> Result<Option<Widget>, String> {
        let attrs = Self::parse_attrs(event)?;
        match tag {
            "div" => {
                let children = if empty {
                    Vec::new()
                } else {
                    Self::parse_children(reader)?
                };
                Ok(Some(Widget::Div(DivWidget { attrs, children })))
            }
            "form" => {
                let onsubmit = get_attr(event, "onsubmit")?;
                let children = if empty {
                    Vec::new()
                } else {
                    Self::parse_children(reader)?
                };
                Ok(Some(Widget::Form(FormWidget {
                    attrs,
                    onsubmit,
                    children,
                })))
            }
            "button" => {
                let label = get_attr(event, "label")?.unwrap_or_default();
                let onclick = get_attr(event, "onclick")?;
                Ok(Some(Widget::Button(ButtonWidget {
                    attrs,
                    label,
                    onclick,
                    focused: false,
                })))
            }
            "input" => {
                let value = get_attr(event, "value")?.unwrap_or_default();
                let label = get_attr(event, "label")?;
                let password = get_attr(event, "type")?
                    .is_some_and(|input_type| input_type.eq_ignore_ascii_case("password"));
                Ok(Some(Widget::Input(InputWidget {
                    attrs,
                    value,
                    label,
                    password,
                    focused: false,
                })))
            }
            "divider" => {
                let label = get_attr(event, "label")?;
                Ok(Some(Widget::Divider(DividerWidget { attrs, label })))
            }
            "text" => {
                let value = get_attr(event, "value")?.unwrap_or_default();
                Ok(Some(Widget::Text(TextWidget { attrs, value })))
            }
            _ => Ok(None),
        }
    }

    /// Read child elements of a container until its matching end tag.
    fn parse_children(reader: &mut Reader<&[u8]>) -> Result<Vec<Widget>, String> {
        let mut children = Vec::new();
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(XmlEvent::Start(e)) => {
                    let tag = tag_name(&e);
                    if let Some(widget) = Self::parse_element(reader, &e, &tag, false)? {
                        children.push(widget);
                    }
                }
                Ok(XmlEvent::Empty(e)) => {
                    let tag = tag_name(&e);
                    if let Some(widget) = Self::parse_element(reader, &e, &tag, true)? {
                        children.push(widget);
                    }
                }
                Ok(XmlEvent::End(_)) => break,
                Ok(XmlEvent::Eof) => return Err("Unexpected EOF inside container".to_string()),
                Err(e) => return Err(format!("XML parse error: {}", e)),
                _ => {}
            }
            buf.clear();
        }
        Ok(children)
    }

    /// Collect the styling-related attributes (`id`, `class`, inline `style`).
    fn parse_attrs(event: &BytesStart) -> Result<StyleAttrs, String> {
        let mut attrs = StyleAttrs::default();
        for attr in event.attributes() {
            let attr = attr.map_err(|e| format!("Attribute error: {}", e))?;
            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
            let value = String::from_utf8_lossy(&attr.value).to_string();
            match key.as_str() {
                "id" => attrs.id = Some(value),
                "class" => {
                    attrs.classes = value.split_whitespace().map(str::to_string).collect();
                }
                "style" => attrs.inline = ComputedStyle::parse_declarations(&value),
                _ => {}
            }
        }
        Ok(attrs)
    }
}

fn tag_name(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.name().as_ref()).to_string()
}

fn get_attr(event: &BytesStart, name: &str) -> Result<Option<String>, String> {
    for attr in event.attributes() {
        let attr = attr.map_err(|e| format!("Attribute error: {}", e))?;
        if attr.key.as_ref() == name.as_bytes() {
            return Ok(Some(String::from_utf8_lossy(&attr.value).to_string()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::XmlParser;
    use crate::widgets::Widget;

    #[test]
    fn parses_input_and_divider_labels() {
        let document = XmlParser::parse(
            r#"<input label="Email" value="user@example.com"/><divider label="OR"/>"#,
        )
        .unwrap();

        match &document.widgets[0] {
            Widget::Input(input) => assert_eq!(input.label.as_deref(), Some("Email")),
            _ => panic!("expected input widget"),
        }
        match &document.widgets[1] {
            Widget::Divider(divider) => assert_eq!(divider.label.as_deref(), Some("OR")),
            _ => panic!("expected divider widget"),
        }
    }

    #[test]
    fn parses_document_title_from_text_or_value_attribute() {
        let text_title = XmlParser::parse("<title>My page</title><text value=\"body\"/>").unwrap();
        assert_eq!(text_title.title.as_deref(), Some("My page"));

        let attribute_title =
            XmlParser::parse("<title value=\"Another page\"/><text value=\"body\"/>").unwrap();
        assert_eq!(attribute_title.title.as_deref(), Some("Another page"));
    }
}
