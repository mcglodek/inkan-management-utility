use anyhow::{Result, Context};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}
};
use ratatui::{backend::CrosstermBackend, Terminal, layout::{Constraint, Direction, Layout}, widgets::{Block, Borders, List, ListItem, Paragraph}, style::{Style, Color}};
use dialoguer::{Input, Confirm};
use std::io;

use crate::commands;
use crate::abi::load_abi;
use crate::process::{process_item, BatchOpts};
use std::fs;

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

pub async fn run_menu() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut index = 0usize;
    let items = MenuItem::all();

    // Main loop
    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(2),
                ].as_ref())
                .split(f.size());

            let header = Paragraph::new("Inkan Management Utility — Menu")
                .block(Block::default().borders(Borders::ALL).title("Welcome"));
            f.render_widget(header, chunks[0]);

            let list_items: Vec<ListItem> = items.iter().enumerate()
                .map(|(i, it)| {
                    let mut text = String::from(it.label());
                    if i == index { text = format!("▶ {}", text); }
                    ListItem::new(text)
                })
                .collect();

            let list = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title("Select an action"))
                .highlight_style(Style::default().fg(Color::Cyan));
            f.render_widget(list, chunks[1]);

            let footer = Paragraph::new("↑/↓ = Navigate • Enter = Select • q = Quit")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        })?;

        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    match k.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Up => {
                            if index == 0 { index = items.len() - 1; } else { index -= 1; }
                        }
                        KeyCode::Down => { index = (index + 1) % items.len(); }
                        KeyCode::Enter => {
                            match items[index] {
                                MenuItem::Keygen => {
                                    // temporarily leave raw UI for prompts
                                    leave_ui(&mut terminal)?;
                                    run_keygen_wizard()?;
                                    reenter_ui(&mut terminal)?;
                                }
                                MenuItem::BatchSign => {
                                    leave_ui(&mut terminal)?;
                                    run_batch_wizard().await?;
                                    reenter_ui(&mut terminal)?;
                                }
                                MenuItem::Quit => break,
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    let out = terminal.backend_mut();
    execute!(out, LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

fn leave_ui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    let out = terminal.backend_mut();
    execute!(out, LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
fn reenter_ui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    *terminal = Terminal::new(CrosstermBackend::new(out))?;
    terminal.clear()?;
    Ok(())
}

fn str_default<S: Into<String>>(prompt: &str, default: S) -> String {
    Input::<String>::new()
        .with_prompt(prompt)
        .default(default.into())
        .interact_text()
        .unwrap()
}

fn u32_default(prompt: &str, default: u32) -> u32 {
    Input::<u32>::new()
        .with_prompt(prompt)
        .default(default)
        .interact_text()
        .unwrap()
}

/// Wizard to generate keys using your existing logic
fn run_keygen_wizard() -> Result<()> {
    println!();
    println!("=== Key Generator ===");
    let count = u32_default("How many keys?", 1);
    let save = Confirm::new().with_prompt("Save to file?").default(false).interact()?;
    let out = if save {
        Some(Input::<String>::new().with_prompt("Output path").default("keys.json".into()).interact_text()?)
    } else { None };

    let records = commands::keygen::generate(count)?;
    if let Some(path) = out {
        commands::keygen::emit(records, Some(path.into()))?;
    } else {
        commands::keygen::emit(records, None)?;
    }

    println!("(Press Enter to return to menu)"); let _ = std::io::stdin().read_line(&mut String::new());
    Ok(())
}

/// Wizard to batch-sign using your existing process_item() pipeline
async fn run_batch_wizard() -> Result<()> {
    use crate::types::Item;

    println!();
    println!("=== Batch Signer ===");
    let batch_path = str_default("Path to batch input JSON", "my_input.json");
    let out_path = str_default("Path to write output JSON", "batch_output.json");
    let gas_limit = str_default("Gas limit", "30000000");
    let max_fee = str_default("Max fee per gas (wei)", "30000000000");
    let max_prio = str_default("Max priority fee per gas (wei)", "2000000000");

    let abi = load_abi()?;
    let text = fs::read_to_string(&batch_path).with_context(|| format!("reading {batch_path}"))?;
    let items: Vec<Item> = serde_json::from_str(&text).context("parsing batch JSON (array)")?;

    let opts = BatchOpts {
        gas_limit,
        max_fee_per_gas: max_fee,
        max_priority_fee_per_gas: max_prio,
    };

    let mut out_vec: Vec<crate::types::BatchEntryOut> = Vec::with_capacity(items.len());
    for (i, it) in items.iter().enumerate() {
        let res = process_item(&abi, &opts, it)
            .await
            .with_context(|| format!("processing item #{} ({})", i, it.function_to_call));
        match res {
            Ok(entry) => out_vec.push(entry),
            Err(e) => return Err(e),
        }
    }

    fs::write(&out_path, serde_json::to_string_pretty(&out_vec)?)
        .with_context(|| format!("writing {}", out_path))?;
    println!("✓ Wrote {}", out_path);

    println!("(Press Enter to return to menu)"); let _ = std::io::stdin().read_line(&mut String::new());
    Ok(())
}

