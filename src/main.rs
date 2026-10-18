mod browser;
mod tab;
mod common;
mod engine;
mod xml_widget_parser;
mod widgets;
mod css;

fn main() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let result = browser::Browser::new().run(&mut terminal);
    ratatui::restore();
    result
}
