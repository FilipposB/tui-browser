# TUI Browser

**A small, keyboard-first browser for XML-powered pages, right in your terminal.**

TUI Browser turns simple XML documents into interactive terminal interfaces. Open a page in a tab, move between tabs, fill in forms, and revisit sites from persistent history—all without leaving the keyboard.

```text
┌────────────────────────────────────────────────────────────────────────────┐
│  Welcome  ·  http://127.0.0.1:5000  │  Login                              │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│                          ┌──────────────────────┐                          │
│                          │    Welcome back      │                          │
│                          │                      │                          │
│                          │  Username: ________  │                          │
│                          │  Password: ********  │                          │
│                          │       [ Sign in ]    │                          │
│                          └──────────────────────┘                          │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ Ctrl+L New URL   Ctrl+H History   Ctrl+T Manage tabs   Ctrl+Q Quit         │
└────────────────────────────────────────────────────────────────────────────┘
```

## What it can do

- **Browse by keyboard:** open HTTP addresses, switch between tabs, and interact with page controls.
- **Keep tabs organized:** see the active page's title and URL inline with its tab.
- **Suggest from history:** recent, distinct visits appear as soon as the URL screen opens. Type to filter by page title or URL; select a suggestion to complete or open it.
- **Remember your visits:** history is saved between runs with the URL, page title, and Unix timestamp. Open the in-app history preview to browse entries and find the JSON file.
- **Render interactive XML pages:** build pages from containers, text, buttons, inputs, dividers, and forms.
- **Add behavior and styling:** page scripts can read inputs and make HTTP requests; a lightweight CSS-like layer styles widgets.

> **This is an intentionally small browser, not a general-purpose web browser.** It currently supports `http://` (not HTTPS) and its own XML widget format rather than HTML.

## Get started

You'll need a [Rust toolchain](https://www.rust-lang.org/tools/install) with Cargo.

```powershell
git clone https://github.com/FilipposB/tui-browser.git
cd tui-browser
cargo run
```

The browser opens on the URL entry screen. Try the demo site in a second terminal:

```powershell
cargo run --bin dummy_server
```

Then enter `http://127.0.0.1:5000` in the browser. The demo server shows a login form; any non-empty username and password will sign in.

## Keyboard controls

| Key | Action |
| --- | --- |
| Type a URL, then `Enter` | Open the address in a new tab |
| `Up` / `Down` | Select a history suggestion |
| `Tab` | Complete the selected suggestion |
| `Ctrl+L` | Enter a new URL |
| `Ctrl+T` | Manage open tabs |
| `Left` / `Right` | Switch tabs |
| `Ctrl+W` | Close the active tab |
| `Ctrl+H` | Preview browsing history |
| `Esc` | Back from the current screen |
| `Ctrl+Q` | Quit |

In the tab manager, use `Up` / `Down` to select a tab, `Enter` to switch to it, `x` or `Delete` to close it, and `n` to enter a new URL.

## Pages

Pages are XML documents. The browser supports `<title>` plus widgets including `<div>`, `<form>`, `<text>`, `<button>`, `<input>`, and `<divider>`. For example:

```xml
<title>Welcome</title>
<div style="padding: 1; border: true; border-color: cyan;">
    <text value="Hello from the terminal!" style="bold: true; align: center;" />
    <form onsubmit="getInputValue('name')">
        <input id="name" label="Your name" />
        <button label="Continue" />
    </form>
</div>
```

Forms can use `onsubmit`, and buttons can use `onclick`. Page scripts run in an embedded JavaScript engine and have these browser-provided helpers:

| Helper | Purpose |
| --- | --- |
| `getInputValue(id)` | Read the current value of an input |
| `httpRequest(method, url, headers, body)` | Make an HTTP request and get its response |
| `base64Encode(value)` | Base64-encode a string |

The CSS-like styling layer supports tag, class, and ID selectors; `:focus`; and inline `style` attributes. Available properties include color, background, bold, alignment, borders, spacing, width, and height.

## Browsing history

History is written as JSON when an address is opened:

- **Windows:** `%APPDATA%\tui-browser\history.json`
- **Linux and macOS:** `$XDG_DATA_HOME/tui-browser/history.json`, or `~/.local/share/tui-browser/history.json` when `XDG_DATA_HOME` is unset

Each entry contains `url`, `title`, and `visited_at_unix` (seconds since the Unix epoch). Press `Ctrl+H` inside the browser to preview the saved entries and see the file location.

## Development

Run the test suite with:

```powershell
cargo test
```

The project uses [Ratatui](https://ratatui.rs/) and Crossterm for the terminal interface, Quick XML for parsing page documents, and Boa for embedded JavaScript.

## License

No license file is currently included in this repository.
