mod browser;
mod tab;
mod common;
mod engine;
mod xml_widget_parser;
mod widgets;
mod css;

fn main() -> std::io::Result<()> {
    let mut browser = browser::Browser::new()?;
    let mut terminal = ratatui::init();
    let result = browser.run(&mut terminal);
    ratatui::restore();
    result
}
