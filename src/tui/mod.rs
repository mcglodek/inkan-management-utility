use anyhow::{anyhow, Context, Result};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Terminal,
};
use std::{fs, io};

use crate::abi::load_abi;
use crate::commands;
use crate::process::{process_item, BatchOpts};

/* ───────────────────────────── App State ───────────────────────────── */

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Screen {
    MainMenu,
    KeygenForm,
    BatchForm,
    Result,
}

#[derive(Copy, Clone, Debug)]
enum MenuItem {
    Keygen,
    BatchSign,
    Quit,
}
impl MenuItem {
    fn all() -> Vec<MenuItem> {
        vec![MenuItem::Keygen, MenuItem::BatchSign, MenuItem::Quit]
    }
    fn label(&self) -> &'static str {
        match self {
            MenuItem::Keygen => "Generate Keys",
            MenuItem::BatchSign => "Batch Sign Transactions",
            MenuItem::Quit => "Quit",
        }
    }
}

/* ─────────────────────────── Text Field Model ─────────────────────────── */

#[derive(Clone, Default)]
struct TextField {
    text: String,
    cursor: usize, // byte index into text
}
impl TextField {
    fn with(text: &str) -> Self {
        Self {
            text: text.into(),
            cursor: text.len(),
        }
    }
    fn clamp(&mut self) {
        if self.cursor > self.text.len() {
            self.cursor = self.text.len();
        }
    }
    fn insert_char(&mut self, c: char) {
        // simple ASCII-ish insert (OK for paths/nums)
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }
    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // move left by 1 byte and remove
        self.cursor -= 1;
        self.text.remove(self.cursor);
    }
    fn delete(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        self.text.remove(self.cursor);
    }
    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }
    fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += 1;
        }
    }
    fn home(&mut self) {
        self.cursor = 0;
    }
    fn end(&mut self) {
        self.cursor = self.text.len();
    }
}

/* ─────────────────────────── Form State ─────────────────────────── */

#[derive(Default)]
struct KeygenFormState {
    count: TextField,   // numeric
    save_to_file: bool, // checkbox
    out_path: TextField,
    field_index: usize, // 0=count, 1=save_to_file, 2=out_path (if visible), SUBMIT at end
}

#[derive(Default)]
struct BatchFormState {
    batch_path: TextField,
    out_path: TextField,
    gas_limit: TextField,
    max_fee_per_gas: TextField,
    max_priority_fee_per_gas: TextField,
    field_index: usize, // 0..4 are text, SUBMIT at 5
}

struct App {
    screen: Screen,
    menu_index: usize,
    keygen: KeygenFormState,
    batch: BatchFormState,
    result_text: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::MainMenu,
            menu_index: 0,
            keygen: KeygenFormState {
                count: TextField::with("1"),
                save_to_file: false,
                out_path: TextField::with("keys.json"),
                field_index: 0,
            },
            batch: BatchFormState {
                batch_path: TextField::with("my_input.json"),
                out_path: TextField::with("batch_output.json"),
                gas_limit: TextField::with("30000000"),
                max_fee_per_gas: TextField::with("30000000000"),
                max_priority_fee_per_gas: TextField::with("2000000000"),
                field_index: 0,
            },
            result_text: String::new(),
        }
    }
}

/* ─────────────────────────── App helpers ─────────────────────────── */

impl App {
    /// Returns a mutable reference to the selected batch text field (0..=4).
    fn batch_text_field_mut(&mut self, idx: usize) -> &mut TextField {
        match idx {
            0 => &mut self.batch.batch_path,
            1 => &mut self.batch.out_path,
            2 => &mut self.batch.gas_limit,
            3 => &mut self.batch.max_fee_per_gas,
            4 => &mut self.batch.max_priority_fee_per_gas,
            _ => unreachable!(),
        }
    }
}

/* ───────────────────────────── Entry ───────────────────────────── */

pub async fn run_menu() -> Result<()> {
    // terminal init
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::default();

    loop {
        terminal.draw(|f| {
            let size = f.size();

            match app.screen {
                Screen::MainMenu => draw_main_menu(f, size, &app),
                Screen::KeygenForm => draw_keygen_form(f, size, &app),
                Screen::BatchForm => draw_batch_form(f, size, &app),
                Screen::Result => draw_result(f, size, &app),
            }
        })?;

        // input handling
        if event::poll(std::time::Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    if handle_key(&mut app, k).await? {
                        break; // quit
                    }
                }
                _ => {}
            }
        }
    }

    // restore
    disable_raw_mode()?;
    let out = terminal.backend_mut();
    execute!(out, LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

/* ─────────────────────────── Drawing ─────────────────────────── */

fn draw_frame_title(title: &str) -> Block<'_> {
    Block::default().borders(Borders::ALL).title(title)
}

fn draw_main_menu(f: &mut Frame<'_>, size: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(2),
            ]
            .as_ref(),
        )
        .split(size);

    let header = Paragraph::new("Inkan Management Utility — Menu").block(draw_frame_title("Welcome"));
    let items = MenuItem::all();
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let selected = i == app.menu_index;
            let prefix = if selected { "▶ " } else { "  " };
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::raw(it.label()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(list_items)
        .block(draw_frame_title("Select an action"))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    let footer = Paragraph::new("↑/↓ = Navigate • Enter = Select • q = Quit")
        .block(draw_frame_title(""));

    f.render_widget(header, chunks[0]);
    f.render_widget(list, chunks[1]);
    f.render_widget(footer, chunks[2]);
}

/* ────────────── helpers for form lines ────────────── */

fn field_line_text<'a>(label: &str, field: &TextField, focused: bool) -> Line<'a> {
    let label_s = format!("{label}: ");
    let (left, right) = field.text.split_at(field.cursor.min(field.text.len()));
    let cursor = if focused { "▏" } else { "" };

    // Ensure something visible for empty fields
    let left_span = if left.is_empty() && right.is_empty() {
        Span::styled(" ", Style::default())
    } else {
        Span::raw(left.to_string())
    };

    Line::from(vec![
        Span::styled(label_s, Style::default().fg(Color::Yellow)),
        left_span,
        Span::styled(cursor, Style::default().fg(Color::Cyan)),
        Span::raw(right.to_string()),
    ])
}

fn bool_field_line<'a>(label: &str, val: bool, focused: bool) -> Line<'a> {
    let label = format!("{label}: ");
    let mark = if val { "[x] Yes" } else { "[ ] No " };
    let cursor = if focused { " ▏" } else { "" };
    Line::from(vec![
        Span::styled(label, Style::default().fg(Color::Yellow)),
        Span::raw(mark.to_string()),
        Span::styled(cursor, Style::default().fg(Color::Cyan)),
    ])
}

fn submit_line<'a>(focused: bool, label: &'a str) -> Line<'a> {
    // Render as a button-looking line: [ Submit ]
    let (lbr, rbr): (&'a str, &'a str) = ("[ ", " ]");
    let inner = if focused {
        Span::styled(label, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::raw(label.to_string())
    };
    Line::from(vec![
        Span::styled(lbr, Style::default().fg(Color::DarkGray)),
        inner,
        Span::styled(rbr, Style::default().fg(Color::DarkGray)),
    ])
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

/* ─────────────────────────── Keygen form ─────────────────────────── */

fn draw_keygen_form(f: &mut Frame<'_>, size: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(2),
            ]
            .as_ref(),
        )
        .split(size);

    let header =
        Paragraph::new("Generate Ethereum/Nostr keypairs (offline)").block(draw_frame_title("Keygen"));

    // Submit index depends on whether out_path is visible
    let submit_idx = if app.keygen.save_to_file { 3 } else { 2 };

    // Fields
    let mut lines: Vec<Line> = vec![
        field_line_text("Count", &app.keygen.count, app.keygen.field_index == 0),
        bool_field_line(
            "Save to file?",
            app.keygen.save_to_file,
            app.keygen.field_index == 1,
        ),
    ];

    if app.keygen.save_to_file {
        lines.push(field_line_text(
            "Output path",
            &app.keygen.out_path,
            app.keygen.field_index == 2,
        ));
    }

    // Submit "button"
    lines.push(Line::from("")); // small spacer
    lines.push(submit_line(app.keygen.field_index == submit_idx, "Submit"));

    let help = Paragraph::new(
        "Tab/Shift+Tab or ↑/↓ move fields • ←/→ move cursor • Home/End • Backspace/Delete • Space/Y/N toggle checkbox • Move to [ Submit ] and press Enter • Esc = Back",
    )
    .block(draw_frame_title("Help"));

    let form = Paragraph::new(lines)
        .block(draw_frame_title("Inputs"))
        .style(Style::default());

    f.render_widget(header, chunks[0]);
    f.render_widget(form, chunks[1]);
    f.render_widget(help, chunks[2]);
}

async fn handle_keygen_keys(app: &mut App, k: KeyEvent) -> Result<bool> {
    // where is the submit "button" index?
    let submit_idx = if app.keygen.save_to_file { 3 } else { 2 };
    // helper: is current focus a text field?
    let is_text_field = |idx: usize, save_to_file: bool| -> bool {
        idx == 0 || (save_to_file && idx == 2)
    };

    match k.code {
        KeyCode::Esc => app.screen = Screen::MainMenu,

        KeyCode::Up | KeyCode::BackTab => {
            if app.keygen.field_index == 0 {
                app.keygen.field_index = submit_idx;
            } else {
                app.keygen.field_index -= 1;
            }
        }

        KeyCode::Down | KeyCode::Tab => {
            app.keygen.field_index = (app.keygen.field_index + 1) % (submit_idx + 1);
        }

        // Toggle checkbox with Space or Y/N when on the checkbox field
        KeyCode::Char(' ') | KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('n') | KeyCode::Char('N')
            if app.keygen.field_index == 1 =>
        {
            match k.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => app.keygen.save_to_file = true,
                KeyCode::Char('n') | KeyCode::Char('N') => app.keygen.save_to_file = false,
                _ => app.keygen.save_to_file = !app.keygen.save_to_file,
            }
        }

        // Optional: allow Enter to toggle the checkbox (but NOT submit)
        KeyCode::Enter if app.keygen.field_index == 1 => {
            app.keygen.save_to_file = !app.keygen.save_to_file;
        }

        // Cursor movement on text fields
        KeyCode::Left if is_text_field(app.keygen.field_index, app.keygen.save_to_file) => {
            match app.keygen.field_index {
                0 => app.keygen.count.move_left(),
                2 => app.keygen.out_path.move_left(),
                _ => {}
            }
        }
        KeyCode::Right if is_text_field(app.keygen.field_index, app.keygen.save_to_file) => {
            match app.keygen.field_index {
                0 => app.keygen.count.move_right(),
                2 => app.keygen.out_path.move_right(),
                _ => {}
            }
        }
        KeyCode::Home if is_text_field(app.keygen.field_index, app.keygen.save_to_file) => {
            match app.keygen.field_index {
                0 => app.keygen.count.home(),
                2 => app.keygen.out_path.home(),
                _ => {}
            }
        }
        KeyCode::End if is_text_field(app.keygen.field_index, app.keygen.save_to_file) => {
            match app.keygen.field_index {
                0 => app.keygen.count.end(),
                2 => app.keygen.out_path.end(),
                _ => {}
            }
        }

        // Editing
        KeyCode::Backspace if is_text_field(app.keygen.field_index, app.keygen.save_to_file) => {
            match app.keygen.field_index {
                0 => app.keygen.count.backspace(),
                2 => app.keygen.out_path.backspace(),
                _ => {}
            }
        }
        KeyCode::Delete if is_text_field(app.keygen.field_index, app.keygen.save_to_file) => {
            match app.keygen.field_index {
                0 => app.keygen.count.delete(),
                2 => app.keygen.out_path.delete(),
                _ => {}
            }
        }
        KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL)
            && is_text_field(app.keygen.field_index, app.keygen.save_to_file) =>
        {
            match app.keygen.field_index {
                0 => app.keygen.count.insert_char(c),
                2 => app.keygen.out_path.insert_char(c),
                _ => {}
            }
        }

        // Submit ONLY when [ Submit ] is focused
        KeyCode::Enter if app.keygen.field_index == submit_idx => {
            let count: u32 = app
                .keygen
                .count
                .text
                .trim()
                .parse()
                .map_err(|_| anyhow!("Count must be a positive integer"))?;
            let records = commands::keygen::generate(count)?;
            if app.keygen.save_to_file {
                let p = app.keygen.out_path.text.trim();
                commands::keygen::emit(records, Some(p.into()))
                    .with_context(|| format!("writing {}", p))?;
                app.result_text = format!("✓ Wrote {}", p);
            } else {
                let json = serde_json::to_string_pretty(&records)?;
                app.result_text = json;
            }
            app.screen = Screen::Result;
        }

        _ => {}
    }
    Ok(false)
}

/* ─────────────────────────── Batch form ─────────────────────────── */

fn draw_batch_form(f: &mut Frame<'_>, size: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(11),
                Constraint::Length(2),
            ]
            .as_ref(),
        )
        .split(size);

    let header = Paragraph::new("Sign a JSON array of EIP-1559 calls (offline)")
        .block(draw_frame_title("Batch"));

    // fixed submit index = 5
    let submit_idx = 5;

    let mut lines: Vec<Line> = vec![
        field_line_text("Batch path", &app.batch.batch_path, app.batch.field_index == 0),
        field_line_text("Output path", &app.batch.out_path, app.batch.field_index == 1),
        field_line_text("Gas limit", &app.batch.gas_limit, app.batch.field_index == 2),
        field_line_text(
            "Max fee per gas (wei)",
            &app.batch.max_fee_per_gas,
            app.batch.field_index == 3,
        ),
        field_line_text(
            "Max priority fee per gas (wei)",
            &app.batch.max_priority_fee_per_gas,
            app.batch.field_index == 4,
        ),
    ];

    lines.push(Line::from("")); // spacer
    lines.push(submit_line(app.batch.field_index == submit_idx, "Submit"));

    let help = Paragraph::new(
        "Tab/Shift+Tab or ↑/↓ move fields • ←/→ move cursor • Home/End • Backspace/Delete • Move to [ Submit ] and press Enter • Esc = Back",
    )
    .block(draw_frame_title("Help"));

    let form = Paragraph::new(lines)
        .block(draw_frame_title("Inputs"))
        .style(Style::default());

    f.render_widget(header, chunks[0]);
    f.render_widget(form, chunks[1]);
    f.render_widget(help, chunks[2]);
}

async fn handle_batch_keys(app: &mut App, k: KeyEvent) -> Result<bool> {
    let idx = app.batch.field_index;
    let submit_idx = 5; // after fields 0..4

    match k.code {
        KeyCode::Esc => app.screen = Screen::MainMenu,

        KeyCode::Up | KeyCode::BackTab => {
            if app.batch.field_index == 0 {
                app.batch.field_index = submit_idx;
            } else {
                app.batch.field_index -= 1;
            }
        }
        KeyCode::Down | KeyCode::Tab => {
            app.batch.field_index = (app.batch.field_index + 1) % (submit_idx + 1);
        }

        // Cursor movement (only for text fields 0..4)
        KeyCode::Left if idx <= 4 => app.batch_text_field_mut(idx).move_left(),
        KeyCode::Right if idx <= 4 => app.batch_text_field_mut(idx).move_right(),
        KeyCode::Home if idx <= 4 => app.batch_text_field_mut(idx).home(),
        KeyCode::End if idx <= 4 => app.batch_text_field_mut(idx).end(),

        // Editing (only for text fields 0..4)
        KeyCode::Backspace if idx <= 4 => app.batch_text_field_mut(idx).backspace(),
        KeyCode::Delete if idx <= 4 => app.batch_text_field_mut(idx).delete(),
        KeyCode::Char(c) if idx <= 4 && !k.modifiers.contains(KeyModifiers::CONTROL) => {
            app.batch_text_field_mut(idx).insert_char(c)
        }

        // Submit ONLY when [ Submit ] is focused
        KeyCode::Enter if idx == submit_idx => {
            let batch_path = app.batch.batch_path.text.trim().to_string();
            let out_path = app.batch.out_path.text.trim().to_string();

            let abi = load_abi()?;
            let text = fs::read_to_string(&batch_path)
                .with_context(|| format!("reading {}", batch_path))?;
            let items: Vec<crate::types::Item> =
                serde_json::from_str(&text).context("parsing batch JSON (array)")?;

            let opts = BatchOpts {
                gas_limit: app.batch.gas_limit.text.trim().to_string(),
                max_fee_per_gas: app.batch.max_fee_per_gas.text.trim().to_string(),
                max_priority_fee_per_gas: app
                    .batch
                    .max_priority_fee_per_gas
                    .text
                    .trim()
                    .to_string(),
            };

            let mut out_vec: Vec<crate::types::BatchEntryOut> = Vec::with_capacity(items.len());
            for (i, it) in items.iter().enumerate() {
                let res = process_item(&abi, &opts, it)
                    .await
                    .with_context(|| format!("processing item #{} ({})", i, it.function_to_call));
                match res {
                    Ok(entry) => out_vec.push(entry),
                    Err(e) => {
                        app.result_text = format!("Error: {e:#}");
                        app.screen = Screen::Result;
                        return Ok(false);
                    }
                }
            }

            fs::write(&out_path, serde_json::to_string_pretty(&out_vec)?)
                .with_context(|| format!("writing {}", out_path))?;

            app.result_text = format!("✓ Wrote {}", out_path);
            app.screen = Screen::Result;
        }

        _ => {}
    }
    Ok(false)
}

/* ───────────── Result screen keys ───────────── */

fn draw_result(f: &mut Frame<'_>, size: Rect, app: &App) {
    // draw centered modal with result text
    let area = centered_rect(80, 70, size);
    let block = Block::default().borders(Borders::ALL).title("Result");
    let text = Paragraph::new(app.result_text.as_str()).block(block);
    f.render_widget(Clear, area);
    f.render_widget(text, area);
}

fn handle_result_keys(app: &mut App, k: KeyEvent) -> Result<bool> {
    match k.code {
        KeyCode::Esc | KeyCode::Enter => {
            app.screen = Screen::MainMenu;
        }
        _ => {}
    }
    Ok(false)
}

/* ─────────────────────────── Input handling switch ─────────────────────────── */

async fn handle_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    // return true to quit
    match app.screen {
        Screen::MainMenu => handle_main_menu_keys(app, k),
        Screen::KeygenForm => handle_keygen_keys(app, k).await,
        Screen::BatchForm => handle_batch_keys(app, k).await,
        Screen::Result => handle_result_keys(app, k),
    }
}

fn handle_main_menu_keys(app: &mut App, k: KeyEvent) -> Result<bool> {
    match k.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Up => {
            if app.menu_index == 0 {
                app.menu_index = MenuItem::all().len() - 1;
            } else {
                app.menu_index -= 1;
            }
        }
        KeyCode::Down => app.menu_index = (app.menu_index + 1) % MenuItem::all().len(),
        KeyCode::Enter => match MenuItem::all()[app.menu_index] {
            MenuItem::Keygen => app.screen = Screen::KeygenForm,
            MenuItem::BatchSign => app.screen = Screen::BatchForm,
            MenuItem::Quit => return Ok(true),
        },
        _ => {}
    }
    Ok(false)
}
