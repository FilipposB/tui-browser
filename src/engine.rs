use crate::css::Stylesheet;
use crate::widgets::*;
use crate::xml_widget_parser::XmlParser;
use boa_engine::{
    js_string, Context as JsContext, JsNativeError, JsResult, JsString, JsValue, NativeFunction,
    Source,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget as RatatuiWidget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationKind {
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationKind {
    fn from_message(message: &str) -> Self {
        if message.starts_with("HTTP/") {
            let status = message
                .split_whitespace()
                .nth(1)
                .and_then(|status| status.parse::<u16>().ok());
            return if status.is_some_and(|status| (200..300).contains(&status)) {
                Self::Success
            } else {
                Self::Error
            };
        }
        if message.starts_with("JavaScript error:")
            || message.starts_with("JavaScript setup error:")
        {
            return Self::Error;
        }
        if message.starts_with("Please ") {
            return Self::Warning;
        }
        Self::Info
    }

    fn color(self) -> Color {
        match self {
            Self::Info => Color::Cyan,
            Self::Success => Color::Green,
            Self::Warning => Color::Yellow,
            Self::Error => Color::LightRed,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Info => " INFO ",
            Self::Success => " SUCCESS ",
            Self::Warning => " CHECK INPUT ",
            Self::Error => " ERROR ",
        }
    }
}

#[derive(Debug, Clone)]
struct Notification {
    message: String,
    kind: NotificationKind,
}

pub struct Engine {
    widgets: Vec<Widget>,
    stylesheet: Stylesheet,
    document_title: Option<String>,
    focused_index: Option<usize>,
    js_context: JsContext,
    notification: Option<Notification>,
}

struct HttpResponse {
    status_line: String,
    status_code: u16,
    body: String,
}

impl Engine {
    pub fn load(xml: &str) -> Self {
        let mut engine = Self::new();
        if let Err(error) = engine.replace_document(xml) {
            engine.show_notification(format!("XML error: {error}"));
        }
        engine
    }

    pub fn load_url(url: &str) -> Self {
        let mut engine = Self::new();
        match send_http_request("GET", url, "", "") {
            Ok(response) if (200..300).contains(&response.status_code) => {
                if let Err(error) = engine.replace_document(&response.body) {
                    engine.show_notification(format!("Server returned invalid XML: {error}"));
                }
            }
            Ok(response) => {
                engine.show_notification(format!("{} ({})", response.status_line, response.body));
            }
            Err(error) => engine.show_notification(error),
        }
        engine
    }

    fn new() -> Self {
        let mut engine = Self {
            widgets: Vec::new(),
            stylesheet: Stylesheet::default(),
            document_title: None,
            focused_index: None,
            js_context: JsContext::default(),
            notification: None,
        };
        engine.install_js_apis();
        engine.focus_first_focusable();
        engine
    }

    fn replace_document(&mut self, xml: &str) -> Result<(), String> {
        let document = XmlParser::parse(xml)?;
        self.widgets = document.widgets;
        self.stylesheet = document.stylesheet;
        self.document_title = document.title.filter(|title| !title.trim().is_empty());
        self.focused_index = None;
        self.notification = None;
        self.focus_first_focusable();
        Ok(())
    }

    pub fn document_title(&self) -> Option<&str> {
        self.document_title.as_deref()
    }

    fn install_js_apis(&mut self) {
        let get_input_value = NativeFunction::from_fn_ptr(get_input_value);
        let http_request = NativeFunction::from_fn_ptr(http_request);
        let base64_encode = NativeFunction::from_fn_ptr(base64_encode_js);
        let result = self
            .js_context
            .register_global_callable(js_string!("getInputValue"), 1, get_input_value)
            .and_then(|()| {
                self.js_context
                    .register_global_callable(js_string!("httpRequest"), 4, http_request)
            })
            .and_then(|()| {
                self.js_context.register_global_callable(
                    js_string!("base64Encode"),
                    1,
                    base64_encode,
                )
            });
        if let Err(error) = result {
            self.show_notification(format!("JavaScript setup error: {error}"));
        }
    }

    fn show_notification(&mut self, message: String) {
        let kind = NotificationKind::from_message(&message);
        self.notification = Some(Notification { message, kind });
    }

    fn get_all_focusable_widgets(&self) -> Vec<(Vec<usize>, &Widget)> {
        let mut focusable = Vec::new();
        for (i, widget) in self.widgets.iter().enumerate() {
            Self::collect_focusable(widget, vec![i], &mut focusable);
        }
        focusable
    }

    fn collect_focusable<'a>(
        widget: &'a Widget,
        path: Vec<usize>,
        result: &mut Vec<(Vec<usize>, &'a Widget)>,
    ) {
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
            Widget::Form(form) => {
                for (i, child) in form.children.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(i);
                    Self::collect_focusable(child, child_path, result);
                }
            }
            Widget::Text(_) | Widget::Divider(_) => {}
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
        let target_path = self.focused_index.and_then(|index| {
            self.get_all_focusable_widgets()
                .get(index)
                .map(|(path, _)| path.clone())
        });

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
            Widget::Form(form) => {
                for child in &mut form.children {
                    Self::clear_focus(child);
                }
            }
            Widget::Text(_) | Widget::Divider(_) => {}
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
            } else if let Widget::Form(form) = widget {
                Self::set_focus_by_path(&mut form.children, &path[1..]);
            }
        }
    }

    pub fn handle_event(&mut self, event: &Event) {
        if let Event::Key(key_event) = event {
            if key_event.kind != KeyEventKind::Release {
                self.handle_key_event(key_event);
            }
        }
    }

    fn handle_key_event(&mut self, key: &KeyEvent) {
        if key.code == KeyCode::Esc && self.notification.is_some() {
            self.notification = None;
            return;
        }

        let focusable_count = self.get_all_focusable_widgets().len();

        match key.code {
            KeyCode::Tab => {
                // Move focus forward
                if focusable_count > 0 {
                    self.focused_index = Some(
                        self.focused_index
                            .map(|i| (i + 1) % focusable_count)
                            .unwrap_or(0),
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
                            .unwrap_or(0),
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

    fn handle_widget_action(&mut self) {
        let focused = self.focused_index.and_then(|index| {
            self.get_all_focusable_widgets()
                .get(index)
                .map(|(path, widget)| {
                    let onclick = match widget {
                        Widget::Button(button) => button.onclick.clone(),
                        _ => None,
                    };
                    (path.clone(), matches!(widget, Widget::Input(_)), onclick)
                })
        });
        let Some((path, is_input, onclick)) = focused else {
            return;
        };
        let form_submit = Self::form_submit_for_path(&self.widgets, &path);
        let script = form_submit.or_else(|| if is_input { None } else { onclick });
        if let Some(script) = script {
            self.run_script(&script);
        }
    }

    fn run_script(&mut self, script: &str) {
        let inputs = self.input_values();
        let global = self.js_context.global_object();
        let mut setup_error = None;
        for (id, value) in inputs {
            let property = format!("__tui_input_{id}");
            if let Err(error) = global.set(
                JsString::from(property),
                JsValue::from(JsString::from(value)),
                true,
                &mut self.js_context,
            ) {
                setup_error = Some(error);
                break;
            }
        }

        if let Some(error) = setup_error {
            self.show_notification(format!("JavaScript setup error: {error}"));
            return;
        }

        match self.js_context.eval(Source::from_bytes(script)) {
            Ok(result) => {
                let message = result
                    .as_string()
                    .map(|message| message.to_std_string_escaped())
                    .unwrap_or_else(|| "Action completed.".to_string());
                self.handle_action_result(message);
            }
            Err(error) => {
                eprintln!("onclick JavaScript error: {error}");
                self.show_notification(format!("JavaScript error: {error}"));
            }
        }
    }

    fn form_submit_for_path(widgets: &[Widget], path: &[usize]) -> Option<String> {
        let (&index, rest) = path.split_first()?;
        let widget = widgets.get(index)?;
        match widget {
            Widget::Form(form) => {
                let script = form
                    .onsubmit
                    .clone()
                    .or_else(|| Self::first_button_action(&form.children));
                if rest.is_empty() {
                    script
                } else {
                    Self::form_submit_for_path(&form.children, rest).or(script)
                }
            }
            Widget::Div(div) => Self::form_submit_for_path(&div.children, rest),
            _ => None,
        }
    }

    fn first_button_action(widgets: &[Widget]) -> Option<String> {
        for widget in widgets {
            match widget {
                Widget::Button(button) => {
                    if button.onclick.is_some() {
                        return button.onclick.clone();
                    }
                }
                Widget::Div(div) => {
                    if let Some(script) = Self::first_button_action(&div.children) {
                        return Some(script);
                    }
                }
                Widget::Form(_) => {}
                Widget::Input(_) | Widget::Text(_) | Widget::Divider(_) => {}
            }
        }
        None
    }

    fn handle_action_result(&mut self, result: String) {
        if let Some((status_line, body)) = result.split_once('\n') {
            if status_line.starts_with("HTTP/") {
                let status = status_line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|status| status.parse::<u16>().ok());
                if status.is_some_and(|status| (200..300).contains(&status)) {
                    let body = body.trim_start_matches(['\r', '\n']);
                    if body.trim_start().starts_with('<') {
                        match self.replace_document(body) {
                            Ok(()) => return,
                            Err(error) => {
                                self.show_notification(format!("{status_line}: {error}"));
                                return;
                            }
                        }
                    }
                }
                self.show_notification(format!("{status_line} {}", body.trim()));
                return;
            }
        }
        self.show_notification(result);
    }

    fn input_values(&self) -> Vec<(String, String)> {
        let mut values = Vec::new();
        for widget in &self.widgets {
            Self::collect_input_values(widget, &mut values);
        }
        values
    }

    fn collect_input_values(widget: &Widget, values: &mut Vec<(String, String)>) {
        match widget {
            Widget::Input(input) => {
                if let Some(id) = &input.attrs.id {
                    values.push((id.clone(), input.value.clone()));
                }
            }
            Widget::Div(div) => {
                for child in &div.children {
                    Self::collect_input_values(child, values);
                }
            }
            Widget::Form(form) => {
                for child in &form.children {
                    Self::collect_input_values(child, values);
                }
            }
            Widget::Button(_) | Widget::Text(_) | Widget::Divider(_) => {}
        }
    }

    fn handle_text_input(&mut self, c: char) {
        if let Some(index) = self.focused_index {
            if let Some(path) = self
                .get_all_focusable_widgets()
                .get(index)
                .map(|(path, _)| path.clone())
            {
                Self::input_text_by_path(&mut self.widgets, &path, c);
            }
        }
    }

    fn handle_backspace(&mut self) {
        if let Some(index) = self.focused_index {
            if let Some(path) = self
                .get_all_focusable_widgets()
                .get(index)
                .map(|(path, _)| path.clone())
            {
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
            } else {
                match widget {
                    Widget::Div(div) => Self::input_text_by_path(&mut div.children, &path[1..], c),
                    Widget::Form(form) => {
                        Self::input_text_by_path(&mut form.children, &path[1..], c)
                    }
                    _ => {}
                }
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
            } else {
                match widget {
                    Widget::Div(div) => Self::backspace_by_path(&mut div.children, &path[1..]),
                    Widget::Form(form) => Self::backspace_by_path(&mut form.children, &path[1..]),
                    _ => {}
                }
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if !self.widgets.is_empty() {
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

        if let Some(notification) = &self.notification {
            if area.width < 8 || area.height < 3 {
                return;
            }
            let width = area.width.min(64);
            let popup = Rect {
                x: area.x + (area.width - width) / 2,
                y: area.y.saturating_add(1),
                width,
                height: 3.min(area.height.saturating_sub(1)),
            };
            let color = notification.kind.color();
            let block = Block::default()
                .title(format!("{} · Esc to dismiss ", notification.kind.title()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(Color::Black));
            let content = block.inner(popup);
            RatatuiWidget::render(block, popup, buf);
            RatatuiWidget::render(
                Paragraph::new(notification.message.as_str())
                    .style(Style::default().fg(Color::White).bg(Color::Black)),
                content,
                buf,
            );
        }
    }
}

fn get_input_value(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut JsContext,
) -> JsResult<JsValue> {
    let Some(id) = args.first().and_then(JsValue::as_string) else {
        return Err(JsNativeError::typ()
            .with_message("getInputValue expects an input id")
            .into());
    };
    let global = context.global_object();
    global.get(
        JsString::from(format!("__tui_input_{}", id.to_std_string_escaped())),
        context,
    )
}

fn http_request(_this: &JsValue, args: &[JsValue], _context: &mut JsContext) -> JsResult<JsValue> {
    let Some(method) = args.first().and_then(JsValue::as_string) else {
        return Err(JsNativeError::typ()
            .with_message("httpRequest expects method, URL, headers, and body")
            .into());
    };
    let Some(url) = args.get(1).and_then(JsValue::as_string) else {
        return Err(JsNativeError::typ()
            .with_message("httpRequest expects method, URL, headers, and body")
            .into());
    };

    let headers = args
        .get(2)
        .and_then(JsValue::as_string)
        .map(|value| value.to_std_string_escaped())
        .unwrap_or_default();
    let body = args
        .get(3)
        .and_then(JsValue::as_string)
        .map(|value| value.to_std_string_escaped())
        .unwrap_or_default();

    match send_http_request(
        &method.to_std_string_escaped(),
        &url.to_std_string_escaped(),
        &headers,
        &body,
    ) {
        Ok(response) => {
            let response_text = format!("{}\n{}", response.status_line, response.body);
            Ok(JsValue::from(JsString::from(response_text)))
        }
        Err(error) => Err(JsNativeError::error().with_message(error).into()),
    }
}

fn base64_encode_js(
    _this: &JsValue,
    args: &[JsValue],
    _context: &mut JsContext,
) -> JsResult<JsValue> {
    let Some(value) = args.first().and_then(JsValue::as_string) else {
        return Err(JsNativeError::typ()
            .with_message("base64Encode expects a string")
            .into());
    };
    let encoded = base64_encode(value.to_std_string_escaped().as_bytes());
    Ok(JsValue::from(JsString::from(encoded)))
}

fn send_http_request(
    method: &str,
    url: &str,
    headers: &str,
    body: &str,
) -> Result<HttpResponse, String> {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    if method.is_empty()
        || !method
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
    {
        return Err("Invalid HTTP method".to_string());
    }
    let Some(url) = url.strip_prefix("http://") else {
        return Err("Only http:// URLs are supported".to_string());
    };
    let split_at = url.find(['/', '?', '#']).unwrap_or(url.len());
    let authority = &url[..split_at];
    let remainder = &url[split_at..];
    let path = if remainder.starts_with('/') {
        remainder.split('#').next().unwrap_or("/").to_string()
    } else if remainder.starts_with('?') {
        format!("/{}", remainder.split('#').next().unwrap_or(""))
    } else if remainder.starts_with('#') || remainder.is_empty() {
        "/".to_string()
    } else {
        return Err("Invalid HTTP URL".to_string());
    };
    if authority.is_empty()
        || authority
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b'@' || byte == b' ')
        || path
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
    {
        return Err("Invalid HTTP URL".to_string());
    }
    let mut request_headers = String::new();
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            return Err("Invalid HTTP header list".to_string());
        };
        let name = name.trim();
        if name.is_empty()
            || !name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#'
                            | b'$'
                            | b'%'
                            | b'&'
                            | b'\''
                            | b'*'
                            | b'+'
                            | b'-'
                            | b'.'
                            | b'^'
                            | b'_'
                            | b'`'
                            | b'|'
                            | b'~'
                    )
            })
            || value
                .bytes()
                .any(|byte| byte.is_ascii_control() && byte != b'\t')
        {
            return Err("Invalid HTTP header list".to_string());
        }
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "host" | "connection" | "content-length"
        ) {
            return Err(format!("The {name} header is managed by the request API"));
        }
        request_headers.push_str(name);
        request_headers.push_str(": ");
        request_headers.push_str(value.trim());
        request_headers.push_str("\r\n");
    }

    let address = authority
        .to_socket_addrs()
        .map_err(|error| format!("HTTP request failed: {error}"))?
        .next()
        .ok_or_else(|| "HTTP URL did not resolve to an address".to_string())?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))
        .map_err(|error| format!("HTTP request failed: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("HTTP request failed: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("HTTP request failed: {error}"))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {authority}\r\n{request_headers}Connection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .map_err(|error| format!("HTTP request failed: {error}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("HTTP response failed: {error}"))?;
    if response.is_empty() {
        return Err("HTTP endpoint returned an empty response".to_string());
    }
    let response = String::from_utf8(response)
        .map_err(|error| format!("HTTP response is not valid UTF-8: {error}"))?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "HTTP response is missing its header terminator".to_string())?;
    let status_line = headers
        .lines()
        .next()
        .ok_or_else(|| "HTTP response is missing its status line".to_string())?
        .to_string();
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "HTTP response has an invalid status line".to_string())?;
    Ok(HttpResponse {
        status_line,
        status_code,
        body: body.to_string(),
    })
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        encoded.push(TABLE[usize::from(first >> 2)] as char);
        encoded.push(TABLE[usize::from(((first & 0x03) << 4) | (second >> 4))] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[usize::from(((second & 0x0f) << 2) | (third >> 6))] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[usize::from(third & 0x3f)] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{send_http_request, Engine, NotificationKind};
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn tab_release_does_not_advance_focus() {
        let mut engine = Engine::load(r#"<div><input/><input/><button label="Sign in"/></div>"#);

        engine.handle_event(&Event::Key(KeyEvent::new_with_kind(
            KeyCode::Tab,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )));

        assert_eq!(engine.focused_index, Some(0));
    }

    #[test]
    fn tab_press_advances_focus_once() {
        let mut engine = Engine::load(r#"<div><input/><input/><button label="Sign in"/></div>"#);

        engine.handle_event(&Event::Key(KeyEvent::new_with_kind(
            KeyCode::Tab,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )));

        assert_eq!(engine.focused_index, Some(1));
    }

    #[test]
    fn enter_runs_focused_button_onclick_script_and_shows_its_result() {
        let mut engine = Engine::load(r#"<button label="Sign in" onclick="'Sign-in clicked!'"/>"#);

        engine.handle_event(&Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )));

        assert_eq!(
            engine
                .notification
                .as_ref()
                .map(|item| item.message.as_str()),
            Some("Sign-in clicked!")
        );
        assert_eq!(
            engine.notification.as_ref().map(|item| item.kind),
            Some(NotificationKind::Info)
        );
    }

    #[test]
    fn login_script_rejects_empty_inputs() {
        let mut engine = Engine::load(
            r#"<div><input id="username"/><input id="password"/><button onclick="(() => { const username = getInputValue('username').trim(); const password = getInputValue('password'); if (!username || !password.trim()) return 'Please fill in both fields.'; const headers = 'Authorization: Basic ' + base64Encode(username + ':' + password); return httpRequest('GET', 'http://127.0.0.1:5000/', headers, ''); })()"/></div>"#,
        );
        engine.focused_index = Some(2);
        engine.update_focus();

        engine.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert_eq!(
            engine
                .notification
                .as_ref()
                .map(|item| item.message.as_str()),
            Some("Please fill in both fields.")
        );
        assert_eq!(
            engine.notification.as_ref().map(|item| item.kind),
            Some(NotificationKind::Warning)
        );
    }

    #[test]
    fn enter_in_form_input_runs_onsubmit() {
        let mut engine = Engine::load(
            r#"<form onsubmit="'form submitted'"><input id="email"/><button label="Submit"/></form>"#,
        );

        engine.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert_eq!(
            engine
                .notification
                .as_ref()
                .map(|item| item.message.as_str()),
            Some("form submitted")
        );
    }

    #[test]
    fn enter_in_form_input_runs_first_button_when_onsubmit_is_missing() {
        let mut engine = Engine::load(
            r#"<form><input id="email"/><button label="Submit" onclick="'default submit'"/></form>"#,
        );

        engine.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert_eq!(
            engine
                .notification
                .as_ref()
                .map(|item| item.message.as_str()),
            Some("default submit")
        );
    }

    #[test]
    fn notification_classifies_http_responses_and_errors() {
        assert_eq!(
            NotificationKind::from_message("HTTP/1.1 200 OK"),
            NotificationKind::Success
        );
        assert_eq!(
            NotificationKind::from_message("HTTP/1.1 401 Unauthorized"),
            NotificationKind::Error
        );
        assert_eq!(
            NotificationKind::from_message("JavaScript error: failed"),
            NotificationKind::Error
        );
    }

    #[test]
    fn escape_dismisses_notification() {
        let mut engine = Engine::load(r#"<button onclick="'Done'"/>"#);
        engine.show_notification("Done".to_string());

        engine.handle_event(&Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));

        assert!(engine.notification.is_none());
    }

    #[test]
    fn server_xml_replaces_current_document_and_resets_focus() {
        let mut engine =
            Engine::load(r#"<div><input id="username"/><button onclick="'unused'"/></div>"#);

        engine.handle_action_result(
            "HTTP/1.1 200 OK\n<text value=\"Welcome from server\"/><button label=\"Log out\"/>"
                .to_string(),
        );

        assert_eq!(engine.focused_index, Some(0));
        assert_eq!(engine.widgets.len(), 2);
        assert!(engine.notification.is_none());
    }

    #[test]
    fn load_url_fetches_and_parses_server_xml() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            assert!(request.starts_with("GET /page HTTP/1.1"));
            let body = r#"<text value="Loaded from server"/>"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });

        let engine = Engine::load_url(&format!("http://{address}/page"));
        server.join().unwrap();
        assert_eq!(engine.widgets.len(), 1);
        assert!(engine.notification.is_none());
    }

    #[test]
    fn notification_renders_as_a_titled_banner() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let mut engine = Engine::load(r#"<button onclick="'Done'"/>"#);
        engine.show_notification("HTTP/1.1 200 OK".to_string());
        let area = Rect::new(0, 0, 40, 10);
        let mut buffer = Buffer::empty(area);

        engine.render(area, &mut buffer);

        let rendered: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(rendered.contains("SUCCESS"));
        assert!(rendered.contains("HTTP/1.1 200 OK"));
        assert!(rendered.contains("Esc to dismiss"));
    }

    #[test]
    fn http_request_uses_xml_selected_method_url_headers_and_body() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                request.push_str(&line);
            }
            let body = "payload";
            let mut request_body = vec![0; body.len()];
            reader.read_exact(&mut request_body).unwrap();
            let mut stream = reader.into_inner();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
            (request, String::from_utf8(request_body).unwrap())
        });

        let response = send_http_request(
            "POST",
            &format!("http://{address}/submit?source=xml"),
            "Authorization: Basic YWxpY2U6c2VjcmV0\r\nX-Custom: yes",
            "payload",
        )
        .unwrap();
        assert_eq!(response.status_line, "HTTP/1.1 200 OK");
        let (request, body) = server.join().unwrap();
        assert!(request.contains("POST /submit?source=xml HTTP/1.1"));
        assert!(request.contains("Authorization: Basic YWxpY2U6c2VjcmV0"));
        assert!(request.contains("X-Custom: yes"));
        assert_eq!(body, "payload");
    }
}
