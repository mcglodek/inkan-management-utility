use anyhow::{Context, Result};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use textwrap::wrap;

use std::path::PathBuf;

use crate::app::{AppCtx, ScreenWidget, Transition};
use crate::ui::layout::{three_box_layout, Margins};
use crate::ui::style::{span_key, span_sep, span_text, button_spans};
use crate::ui::common_nav::esc_to_back;
use crate::ui::components::{TextField, field_line_text};
use crate::defaults::Defaults;

// Generic OK-only modal
use crate::screens::{ConfirmOkScreen, AfterOk};

// ABI / processor / types / writer
use crate::abi::load_abi;
use crate::process::{process_item, BatchOpts};
use crate::types::Item;
use crate::write_signed_transactions_to_file::{
    write_single_signed_transaction,
    build_filename_for_any_tx,
};

// Load-from-file flow (directory picker) — invalidation version
use crate::screens::ChoosePermanentInvalidationInfoDirScreen;

pub struct CreatePermanentInvalidationScreen {
    // 0 privkey_to_be_invalidated, 1 nonce,
    // 2 gas_limit, 3 max_fee_per_gas, 4 max_priority_fee_per_gas,
    // 5 out_dir, 6 submit, 7 load_from_file, 8 back
    field_index: usize,
    privkey_to_be_invalidated: TextField,
    nonce: TextField,
    gas_limit: TextField,
    max_fee_per_gas: TextField,
    max_priority_fee_per_gas: TextField,
    out_dir: TextField,
}

impl CreatePermanentInvalidationScreen {
    pub fn new() -> Self {
        Self {
            field_index: 0,
            privkey_to_be_invalidated: TextField::with(""),
            nonce: TextField::with(""),
            gas_limit: TextField::with(Defaults::GAS_LIMIT),
            max_fee_per_gas: TextField::with(Defaults::MAX_FEE_PER_GAS),
            max_priority_fee_per_gas: TextField::with(Defaults::MAX_PRIORITY_FEE_PER_GAS),
            out_dir: TextField::with(Defaults::CREATE_REVOCATION_OUT_DIR),
        }
    }

    fn is_text(&self) -> bool {
        matches!(self.field_index, 0 | 1 | 2 | 3 | 4 | 5)
    }

    fn tf_ref(&self, idx: usize) -> &TextField {
        match idx {
            0 => &self.privkey_to_be_invalidated,
            1 => &self.nonce,
            2 => &self.gas_limit,
            3 => &self.max_fee_per_gas,
            4 => &self.max_priority_fee_per_gas,
            5 => &self.out_dir,
            _ => unreachable!("tf_ref called on non-text field"),
        }
    }

    fn tf_mut(&mut self, idx: usize) -> &mut TextField {
        match idx {
            0 => &mut self.privkey_to_be_invalidated,
            1 => &mut self.nonce,
            2 => &mut self.gas_limit,
            3 => &mut self.max_fee_per_gas,
            4 => &mut self.max_priority_fee_per_gas,
            5 => &mut self.out_dir,
            _ => unreachable!("tf_mut called on non-text field"),
        }
    }

    // Small helper: set text and move cursor to end.
    fn set_textfield(tf: &mut TextField, val: &str) {
        tf.text = val.to_string();
        tf.end();
    }

    // Apply pending prefill (consumes ctx.pending_invalidation_prefill if present)
    fn apply_prefill_if_any(&mut self, ctx: &mut AppCtx) {
        if let Some(prefill) = ctx.pending_permanent_invalidation_prefill.take() {
            if let Some(v) = prefill.map.get("PRIVKEY_TO_BE_INVALIDATED") {
                Self::set_textfield(&mut self.privkey_to_be_invalidated, v);
            }
            if let Some(v) = prefill.map.get("NONCE") {
                Self::set_textfield(&mut self.nonce, v);
            }
            if let Some(v) = prefill.map.get("GAS_LIMIT") {
                Self::set_textfield(&mut self.gas_limit, v);
            }
            if let Some(v) = prefill.map.get("MAX_FEE_PER_GAS") {
                Self::set_textfield(&mut self.max_fee_per_gas, v);
            }
            if let Some(v) = prefill.map.get("MAX_PRIORITY_FEE_PER_GAS") {
                Self::set_textfield(&mut self.max_priority_fee_per_gas, v);
            }
            if let Some(v) = prefill.map.get("OUTPUT_DIRECTORY") {
                Self::set_textfield(&mut self.out_dir, v);
            }
        }
    }

    // < Create Permanent Invalidation >   < Load From File >   < Back >
    fn buttons_line(submit_selected: bool, load_selected: bool, back_selected: bool) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(button_spans("Create Permanent Invalidation", submit_selected));
        spans.push(Span::raw("   "));
        spans.extend(button_spans("Load From File", load_selected));
        spans.push(Span::raw("   "));
        spans.extend(button_spans("Back", back_selected));
        Line::from(spans)
    }

    fn ensure_out_dir_nonempty(&self) -> Result<PathBuf> {
        let out_dir = self.out_dir.text.trim();
        if out_dir.is_empty() {
            anyhow::bail!("Output Directory cannot be empty.");
        }
        Ok(PathBuf::from(out_dir))
    }

    /// Create, sign, and write a single invalidation tx using process_item() + writer.
    async fn create_and_write_invalidation(&self) -> Result<PathBuf> {
        // Validate required secret
        let pk = self.privkey_to_be_invalidated.text.trim();
        if pk.is_empty() {
            anyhow::bail!("PrivKey To Be Invalidated cannot be empty.");
        }

        // Parse nonce
        let nonce_str = self.nonce.text.trim();
        let nonce: u64 = nonce_str.parse().context("Nonce must be an integer")?;

        // Gas options
        let opts = BatchOpts {
            gas_limit: self.gas_limit.text.trim().to_string(),
            max_fee_per_gas: self.max_fee_per_gas.text.trim().to_string(),
            max_priority_fee_per_gas: self.max_priority_fee_per_gas.text.trim().to_string(),
        };

        // ABI
        let abi = load_abi()?;

        // Assemble Item for createPermanentInvalidationEvent
        let item = Item {
            function_to_call: "createPermanentInvalidationEvent".to_string(),
            nonce: Some(nonce),
            chain_id: Some(Defaults::CHAIN_ID),
            contract_address: Defaults::CONTRACT_ADDRESS.to_string(),

            // Type A (unused)
            type_a_privkey_x: None,
            type_a_privkey_y: None,
            type_a_pubkey_y: None,
            type_a_uint_x: None,
            type_a_uint_y: None,
            type_a_boolean: None,

            // Type B (unused)
            type_b_privkey_x: None,
            type_b_privkey_y: None,
            type_b_pubkey_y: None,
            type_b_uint_x: None,
            type_b_uint_y: None,

            // Type C
            type_c_privkey_x: Some(pk.to_string()),
        };

        // Build & sign the transaction
        let entry = process_item(&abi, &opts, &item)
            .await
            .context("failed to construct and sign permanent invalidation transaction")?;

        // Filename per helper: "[X]_invalidation_nonce_[nonce].txt"
        let filename = build_filename_for_any_tx(&entry.decoded_tx);
        let mut out_path = self.ensure_out_dir_nonempty()?;
        out_path.push(filename);
        let written = write_single_signed_transaction(&out_path, &entry, true)
            .context("failed to write signed transaction file")?;

        Ok(written)
    }

    fn validate_gas_limit(&self) -> Result<()> {
        let max_str = Defaults::GAS_LIMIT.trim();
        let max: u64 = max_str.parse().context("Defaults::GAS_LIMIT must be an integer")?;

        let user_str = self.gas_limit.text.trim();
        let user: u64 = user_str.parse().context("Gas limit must be an integer")?;

        if user == 0 {
            anyhow::bail!("Gas limit must be greater than zero.");
        }
        if user > max {
            anyhow::bail!(format!(
                "Gas limit {} exceeds the maximum allowed {}.",
                user, max
            ));
        }
        Ok(())
    }

    fn validate_fee_caps(&self) -> Result<()> {
        // maxFeePerGas cap
        let max_fee_cap_str = Defaults::MAX_FEE_PER_GAS.trim();
        let max_fee_cap: u64 = max_fee_cap_str
            .parse()
            .context("Defaults::MAX_FEE_PER_GAS must be an integer (wei)")?;

        let user_max_fee_str = self.max_fee_per_gas.text.trim();
        let user_max_fee: u64 = user_max_fee_str
            .parse()
            .context("Maximum Fee Per Gas must be an integer (wei)")?;
        if user_max_fee == 0 {
            anyhow::bail!("Maximum Fee Per Gas must be greater than zero.");
        }
        if user_max_fee > max_fee_cap {
            anyhow::bail!(format!(
                "Maximum Fee Per Gas {} exceeds the allowed maximum {} wei.",
                user_max_fee, max_fee_cap
            ));
        }

        // maxPriorityFeePerGas cap
        let max_prio_cap_str = Defaults::MAX_PRIORITY_FEE_PER_GAS.trim();
        let max_prio_cap: u64 = max_prio_cap_str
            .parse()
            .context("Defaults::MAX_PRIORITY_FEE_PER_GAS must be an integer (wei)")?;

        let user_prio_str = self.max_priority_fee_per_gas.text.trim();
        let user_prio: u64 = user_prio_str
            .parse()
            .context("Maximum Priority Fee Per Gas must be an integer (wei)")?;

        // priority fee can be zero, but not above cap
        if user_prio > max_prio_cap {
            anyhow::bail!(format!(
                "Maximum Priority Fee Per Gas {} exceeds the allowed maximum {} wei.",
                user_prio, max_prio_cap
            ));
        }

        // Sanity: priority <= max fee
        if user_prio > user_max_fee {
            anyhow::bail!("Maximum Priority Fee Per Gas cannot exceed Maximum Fee Per Gas.");
        }

        Ok(())
    }
}

impl Default for CreatePermanentInvalidationScreen {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl ScreenWidget for CreatePermanentInvalidationScreen {
    fn apply_prefill(&mut self, ctx: &mut AppCtx) {
        self.apply_prefill_if_any(ctx); // consumes ctx.pending_invalidation_prefill exactly once
    }

    fn title(&self) -> &str { "" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let header_text = "Create Permanent Invalidation";
        let explanation_paras = [
            "Enter the fields below. The app will create and sign an EIP-1559 transaction",
            "for createPermanentInvalidationEvent and save a one-element JSON array (pretty-printed)",
            "to your chosen output directory. The filename will be:",
            "[invalidatedX]_invalidation_nonce_[nonce].txt",
        ];

        // === TOP BOX ===
        let top_inner_width = size.width.saturating_sub(2*2 + 2 + 2*3) as usize;
        let header_lines = wrap(header_text, top_inner_width).len() as u16;

        let mut exp_lines = 0usize;
        for p in explanation_paras { exp_lines += wrap(p, top_inner_width).len(); }
        let explanation_lines = exp_lines as u16 + (explanation_paras.len().saturating_sub(1) as u16);

        let top_needed = 2 + 2 + header_lines + 1 + explanation_lines;

        // Middle: 9 focusable positions (0..=8) plus spacer
        let middle_rows: u16 = 9 + 1;
        let middle_needed = 2 + 2 + middle_rows;

        let footer_height = 3;

        let regions = three_box_layout(
            size, top_needed, middle_needed, footer_height,
            Margins { page: 2, inner_top: 3, inner_middle: 3, inner_bottom: 3 }
        );

        // TOP
        f.render_widget(Block::default().borders(Borders::ALL), regions.top);
        let top_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_lines.max(1)),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(regions.top_inner);

        let header_para = Paragraph::new(header_text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        let mut expl_lines: Vec<Line> = Vec::new();
        for (i, p) in explanation_paras.iter().enumerate() {
            for seg in wrap(p, top_inner_width) { expl_lines.push(Line::from(seg.to_string())); }
            if i + 1 < explanation_paras.len() { expl_lines.push(Line::from("")); }
        }
        let explanation_para = Paragraph::new(expl_lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });

        f.render_widget(header_para, top_chunks[0]);
        f.render_widget(explanation_para, top_chunks[2]);

        // === MIDDLE BOX ===
        f.render_widget(Block::default().borders(Borders::ALL), regions.middle);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from("")); // spacer above first field
        lines.push(field_line_text("PrivKey To Be Invalidated", self.tf_ref(0), self.field_index == 0));
        lines.push(field_line_text("Transaction Nonce", self.tf_ref(1), self.field_index == 1));

        // Gas limit (cap label)
        let gas_label = format!("Gas limit (maximum {} gas)", Defaults::GAS_LIMIT);
        lines.push(field_line_text(&gas_label, self.tf_ref(2), self.field_index == 2));

        // Max fee per gas (cap label)
        let mfg_label = format!(
            "Maximum Fee Per Gas (maximum {} wei)",
            Defaults::MAX_FEE_PER_GAS
        );
        lines.push(field_line_text(&mfg_label, self.tf_ref(3), self.field_index == 3));

        // Max priority fee per gas (cap label)
        let mpfg_label = format!(
            "Maximum Priority Fee Per Gas (maximum {} wei)",
            Defaults::MAX_PRIORITY_FEE_PER_GAS
        );
        lines.push(field_line_text(&mpfg_label, self.tf_ref(4), self.field_index == 4));

        // Output directory
        lines.push(field_line_text("Output Directory", self.tf_ref(5), self.field_index == 5));

        lines.push(Line::from("")); // spacer
        lines.push(Self::buttons_line(
            self.field_index == 6,
            self.field_index == 7,
            self.field_index == 8
        ));

        let middle_para = Paragraph::new(lines);
        f.render_widget(middle_para, regions.middle_inner);

        // === BOTTOM BOX (legend) ===
        f.render_widget(Block::default().borders(Borders::ALL), regions.bottom);
        let footer_line = Line::from(vec![
            span_key("↑/↓/Tab"), span_text(" Navigate"), span_sep(),
            span_key("Enter"),   span_text(" Select"),  span_sep(),
            span_key("Esc"),     span_text(" Back"),    span_sep(),
            span_key("Ctrl+Q"),  span_text(" Quit"),
        ]);
        f.render_widget(Paragraph::new(footer_line).wrap(Wrap { trim: true }), regions.bottom_inner);
    }

    async fn on_key(&mut self, k: KeyEvent, ctx: &mut AppCtx) -> Result<Transition> {
        // Apply pending prefill if any
        self.apply_prefill_if_any(ctx);

        if let Some(t) = esc_to_back(k) { return Ok(t); }

        if let KeyCode::Char('q') = k.code {
            if k.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(Transition::Push(Box::new(crate::screens::ConfirmQuitScreen::new())));
            }
        }

        match k.code {
            // Navigation
            KeyCode::Up => {
                if self.field_index == 0 { self.field_index = 8; } else { self.field_index -= 1; }
            }
            KeyCode::Down | KeyCode::Tab => {
                self.field_index = (self.field_index + 1) % 9;
            }

            // Enter on [Create Permanent Invalidation]
            KeyCode::Enter if self.field_index == 6 => {
                // Enforce caps first
                if let Err(e) = self.validate_gas_limit() {
                    return Ok(Transition::Push(Box::new(
                        ConfirmOkScreen::new(&format!("Error: {e}")).with_after_ok(AfterOk::Pop)
                    )));
                }
                if let Err(e) = self.validate_fee_caps() {
                    return Ok(Transition::Push(Box::new(
                        ConfirmOkScreen::new(&format!("Error: {e}")).with_after_ok(AfterOk::Pop)
                    )));
                }

                // Create, sign, and write the single-entry JSON
                match self.create_and_write_invalidation().await {
                    Ok(path) => {
                        let lines = vec![
                            "Saved signed permanent invalidation transaction:".to_string(),
                            "".to_string(),
                            path.display().to_string(),
                        ];
                        return Ok(Transition::Push(Box::new(
                            ConfirmOkScreen::with_lines(lines).with_after_ok(AfterOk::Pop)
                        )));
                    }
                    Err(e) => {
                        return Ok(Transition::Push(Box::new(
                            ConfirmOkScreen::new(&format!("Error: {e:#}"))
                                .with_after_ok(AfterOk::Pop)
                        )));
                    }
                }
            }

            // Enter on [Load From File]
            KeyCode::Enter if self.field_index == 7 => {
                return Ok(Transition::Push(Box::new(
                    ChoosePermanentInvalidationInfoDirScreen::new()
                )));
            }

            // Enter on [Back]
            KeyCode::Enter if self.field_index == 8 => {
                return Ok(Transition::Pop); // Back
            }

            // Cursor movement in text fields
            KeyCode::Left  if self.is_text() => self.tf_mut(self.field_index).move_left(),
            KeyCode::Right if self.is_text() => self.tf_mut(self.field_index).move_right(),
            KeyCode::Home  if self.is_text() => self.tf_mut(self.field_index).home(),
            KeyCode::End   if self.is_text() => self.tf_mut(self.field_index).end(),

            // Editing
            KeyCode::Backspace if self.is_text() => self.tf_mut(self.field_index).backspace(),
            KeyCode::Delete    if self.is_text() => self.tf_mut(self.field_index).delete(),
            KeyCode::Char(c) if self.is_text() && !k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.tf_mut(self.field_index).insert_char(c)
            }

            _ => {}
        }
        Ok(Transition::Stay)
    }
}

/* ---------- helpers ---------- */

fn split_name_ext(filename: &str) -> (&str, &str) {
    match filename.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => (stem, ext),
        _ => (filename, ""),
    }
}

