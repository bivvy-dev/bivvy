//! Initialization screen with detection and step template selection.
//!
//! This screen displays detected technologies and provides a selectable
//! list of suggested step templates based on the detection results.
//!
//! ## Bivvy Components Used
//!
//! - `HeaderWidget` — Branding, progress steps (center), action buttons (right)
//! - `ProgressSteps` — Step indicator built into the header center
//! - `ChipGroup` — Display detected technologies as chips
//! - `CardList` — Selectable step templates with checkbox indicators
//! - `SectionCard` — Bordered sections for DETECTED, SUGGESTED STEPS, SUMMARY
//! - `FooterWidget` — Keyboard hints from BindingSet
//! - `Button` — Action bar confirm/cancel buttons (in header)
//!
//! ## Interaction Compliance
//!
//! - Screens handle semantic `Action`s, never raw key events
//! - `bindings()` returns the active `BindingSet` for key resolution & footer
//! - `render()` registers `HitRegion`s in the `HitMap` for mouse click targets
//! - `handle_click()` implements FocusThenFire for the step template list
//!
//! ## Layout
//!
//! Uses `screen_layouts::init()` + `allocate_regions()`:
//! ```text
//! ┌────────────────────────────────────────┐
//! │ Header — branding+progress+btns Pin(3) │
//! ├────────────────────────────────────────┤
//! │ Detection — chips (Flex 5/3)           │
//! ├────────────────────────────────────────┤
//! │ Steps — selection list (Flex 14/5)     │
//! ├────────────────────────────────────────┤
//! │ Summary (Flex 4/2)                     │
//! ├────────────────────────────────────────┤
//! │ Footer (Pin 2)                         │
//! └────────────────────────────────────────┘
//! ```

use std::cell::Cell;

use ratatui::prelude::*;

use crate::ui::tui::app::AppState;
use ratatui::widgets::{Block, Borders, Clear};

use crate::ui::tui::components::{
    Button, ButtonColor, CardList, CardListItem, CardListSpacing, ChipGroup, ChipGroupItem,
    FooterWidget, HeaderData, HeaderWidget, ProgressSteps, SectionCard,
};
use crate::ui::tui::interaction::{bindings, Action, BindingSet, ClickBehavior, HitMap, HitRegion};
use crate::ui::tui::layouts::screen_layouts;
use crate::ui::tui::layouts::{allocate_regions, DegradeMode};
use crate::ui::tui::screen::{KeyHint, Screen, ScreenResult};
use crate::ui::tui::theme::{Palette, Theme};

/// Progress step in the init flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressStep {
    Detect,
    Configure,
    Confirm,
}

impl ProgressStep {
    fn all() -> [ProgressStep; 3] {
        [
            ProgressStep::Detect,
            ProgressStep::Configure,
            ProgressStep::Confirm,
        ]
    }

    fn label(&self) -> &'static str {
        match self {
            ProgressStep::Detect => "Detect",
            ProgressStep::Configure => "Configure",
            ProgressStep::Confirm => "Confirm",
        }
    }

    fn index(&self) -> usize {
        match self {
            ProgressStep::Detect => 0,
            ProgressStep::Configure => 1,
            ProgressStep::Confirm => 2,
        }
    }
}

/// A detected technology.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Name of the detected technology (e.g., "Rust", "Node").
    pub name: String,
    /// Additional detail (e.g., "Cargo.toml", "package.json").
    pub detail: Option<String>,
    /// Whether the technology was found.
    pub found: bool,
}

impl DetectionResult {
    /// Create a new detection result for a found technology.
    pub fn new(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            detail: Some(detail.into()),
            found: true,
        }
    }

    /// Create a detection result for a technology that was not found.
    pub fn not_found(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            detail: None,
            found: false,
        }
    }
}

/// A suggested step template detected for the project.
#[derive(Debug, Clone)]
pub struct StepTemplate {
    /// Name of the step template (e.g., "brew", "mise", "cargo").
    pub name: String,
    /// Why this template is suggested (e.g., "Detected Cargo.toml").
    pub reason: String,
    /// Whether this step is selected for inclusion.
    pub selected: bool,
}

impl StepTemplate {
    /// Create a new step template option.
    pub fn new(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reason: reason.into(),
            selected: false,
        }
    }

    /// Create a new step template option that is pre-selected.
    pub fn new_selected(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reason: reason.into(),
            selected: true,
        }
    }
}

/// Result of the init screen interaction.
#[derive(Debug, Clone)]
pub enum InitResult {
    /// User confirmed selections.
    Confirmed(Vec<String>),
    /// User cancelled.
    Cancelled,
}

/// The init screen.
pub struct InitScreen {
    /// Detected technologies.
    detections: Vec<DetectionResult>,
    /// Suggested step templates.
    step_templates: Vec<StepTemplate>,
    /// Currently focused step template index.
    pub selected_index: usize,
    /// Scroll offset for the template list (first visible item index).
    scroll_offset: usize,
    /// Number of visible items (updated during render via interior mutability).
    visible_count: Cell<usize>,
    /// Current progress step.
    current_step: ProgressStep,
    /// Project name (for header).
    project_name: String,
    /// Whether the user has confirmed.
    completed: bool,
    /// Whether the user cancelled.
    cancelled: bool,
    /// Whether the help overlay is showing.
    show_help: bool,
    /// Theme for styling.
    theme: Theme,
}

impl InitScreen {
    pub fn new() -> Self {
        Self {
            detections: Vec::new(),
            step_templates: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            visible_count: Cell::new(0),
            current_step: ProgressStep::Configure,
            project_name: "my-project".to_string(),
            completed: false,
            cancelled: false,
            show_help: false,
            theme: Theme::default(),
        }
    }

    /// Set the project name.
    pub fn project_name(mut self, name: impl Into<String>) -> Self {
        self.project_name = name.into();
        self
    }

    /// Set the current progress step.
    pub fn current_step(mut self, step: ProgressStep) -> Self {
        self.current_step = step;
        self
    }

    pub fn add_detection(&mut self, result: DetectionResult) {
        self.detections.push(result);
    }

    pub fn add_step_template(&mut self, template: StepTemplate) {
        self.step_templates.push(template);
    }

    /// Deprecated: Use add_step_template instead.
    pub fn add_workflow(&mut self, option: StepTemplate) {
        self.step_templates.push(option);
    }

    pub fn move_selection(&mut self, delta: i32) {
        let new_index = self.selected_index as i32 + delta;
        if new_index >= 0 && (new_index as usize) < self.step_templates.len() {
            self.selected_index = new_index as usize;
            self.ensure_visible();
        }
    }

    /// Adjust scroll_offset to ensure selected_index is visible.
    fn ensure_visible(&mut self) {
        let visible = self.visible_count.get();
        if visible == 0 {
            return;
        }

        // If selected is above visible area, scroll up
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }

        // If selected is below visible area, scroll down
        let last_visible = self.scroll_offset + visible.saturating_sub(1);
        if self.selected_index > last_visible {
            self.scroll_offset = self.selected_index.saturating_sub(visible - 1);
        }
    }

    pub fn toggle_selection(&mut self) {
        // Multi-select behavior: toggle the current item
        if let Some(template) = self.step_templates.get_mut(self.selected_index) {
            template.selected = !template.selected;
        }
    }

    pub fn get_selected_steps(&self) -> Vec<String> {
        self.step_templates
            .iter()
            .filter(|t| t.selected)
            .map(|t| t.name.clone())
            .collect()
    }

    pub fn is_complete(&self) -> bool {
        self.completed || self.cancelled
    }

    pub fn result(&self) -> InitResult {
        if self.cancelled {
            InitResult::Cancelled
        } else {
            InitResult::Confirmed(self.get_selected_steps())
        }
    }

    // ========================================================================
    // Rendering
    // ========================================================================

    /// Build the progress steps line for the header center.
    fn build_progress_line(&self) -> Line<'static> {
        let labels: Vec<String> = ProgressStep::all()
            .iter()
            .map(|s| s.label().into())
            .collect();
        let progress = ProgressSteps::new(labels, self.current_step.index());
        progress.to_line()
    }

    fn render_header(&self, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let header_data = HeaderData::new(&self.project_name, None::<&str>).with_subtitle("init");

        let progress_line = self.build_progress_line();

        let cancel_btn = Button::new("Cancel").color(ButtonColor::Muted);
        let confirm_btn = Button::new("Confirm & Run")
            .icon("⏎")
            .color(ButtonColor::Accent);

        let widget = HeaderWidget::new(&header_data, &self.theme)
            .center(progress_line)
            .actions(vec![cancel_btn, confirm_btn]);

        let button_rects = widget.render_to_frame(frame, area);

        // Register hit regions for action buttons
        if let Some(cancel_rect) = button_rects.first() {
            hits.register(HitRegion {
                area: *cancel_rect,
                action: Action::Cancel,
                index: None,
                click: ClickBehavior::Fire,
            });
        }
        if let Some(confirm_rect) = button_rects.get(1) {
            hits.register(HitRegion {
                area: *confirm_rect,
                action: Action::Confirm,
                index: None,
                click: ClickBehavior::Fire,
            });
        }
    }

    fn render_help_overlay(&self, frame: &mut Frame, area: Rect) {
        let bindings = self.bindings();
        let footer = bindings.footer_bindings();

        // Build help lines
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::default()); // blank line after title
        for binding in &footer {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<12}", binding.key_label),
                    Style::default()
                        .fg(Palette::LIME)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(binding.description, Style::default().fg(Palette::TEXT)),
            ]));
        }
        lines.push(Line::default()); // blank line before close hint
        lines.push(Line::styled(
            "Press any key to close",
            Style::default().fg(Palette::TEXT_DIM),
        ));

        let content_height = lines.len() as u16 + 2; // +2 for border
        let content_width = 36u16;

        let overlay_width = content_width.min(area.width.saturating_sub(4));
        let overlay_height = content_height.min(area.height.saturating_sub(2));

        let x = area.x + (area.width.saturating_sub(overlay_width)) / 2;
        let y = area.y + (area.height.saturating_sub(overlay_height)) / 2;

        let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

        // Clear background
        frame.render_widget(Clear, overlay_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(Palette::LIME))
            .title(" Help ")
            .title_style(
                Style::default()
                    .fg(Palette::LIME)
                    .add_modifier(Modifier::BOLD),
            )
            .style(Style::default().bg(Palette::BG_ELEVATED));

        let inner = block.inner(overlay_area);
        frame.render_widget(block, overlay_area);

        // Render lines inside the block
        for (i, line) in lines.iter().enumerate() {
            let ly = inner.y + i as u16;
            if ly >= inner.y + inner.height {
                break;
            }
            frame
                .buffer_mut()
                .set_line(inner.x + 1, ly, line, inner.width.saturating_sub(2));
        }
    }

    fn render_detections(&self, frame: &mut Frame, area: Rect, mode: DegradeMode) {
        let found: Vec<_> = self.detections.iter().filter(|d| d.found).collect();

        // Create card with title — DegradeMode flows through from allocator
        let card = SectionCard::new().title("DETECTED").degrade_mode(mode);
        let inner = card.render(frame, area);

        if inner.height == 0 {
            return;
        }

        // Render count badge in top-right of card (only when Full mode has a border)
        if mode == DegradeMode::Full && inner.height > 0 {
            let count = format!("{} items", found.len());
            let badge_x = inner.x + inner.width - count.len() as u16 - 1;
            frame.buffer_mut().set_string(
                badge_x,
                area.y, // In the border row
                &count,
                Style::default().fg(Palette::TEAL),
            );
        }

        if found.is_empty() {
            let empty = Line::styled(
                "No technologies detected",
                Style::default().fg(Palette::TEXT_DIM),
            );
            frame
                .buffer_mut()
                .set_line(inner.x, inner.y, &empty, inner.width);
            return;
        }

        // Create chip group items
        let chip_items: Vec<ChipGroupItem> = found
            .iter()
            .map(|d| ChipGroupItem::new(&d.name, d.detail.as_deref()))
            .collect();

        let chip_group = ChipGroup::new(chip_items).degrade_mode(mode);
        chip_group.render(frame, inner);

        // Render "Based on:" line if we have space
        if inner.height >= 4 {
            let details: Vec<_> = found.iter().filter_map(|d| d.detail.as_deref()).collect();

            if !details.is_empty() {
                let based_on = format!("Based on: {}", details.join(", "));
                let y = inner.y + inner.height - 1;

                // Draw subtle separator
                for x in inner.x..inner.x + inner.width {
                    if let Some(cell) = frame
                        .buffer_mut()
                        .cell_mut(Position::new(x, y.saturating_sub(1)))
                    {
                        cell.set_style(Style::default().fg(Palette::BORDER_SUBTLE));
                    }
                }

                frame.buffer_mut().set_string(
                    inner.x,
                    y,
                    &based_on,
                    Style::default().fg(Palette::TEXT_DIM),
                );
            }
        }
    }

    fn render_step_templates(
        &self,
        frame: &mut Frame,
        area: Rect,
        hits: &mut HitMap,
        mode: DegradeMode,
    ) {
        // Create card — DegradeMode flows through from allocator
        let card = SectionCard::new()
            .title("Add templates?")
            .degrade_mode(mode);
        let inner = card.render(frame, area);

        if inner.height == 0 || self.step_templates.is_empty() {
            return;
        }

        // Only show helper text in Full mode — Compact/Minimal need all space for the list
        let mut y = inner.y;
        let mut lines_used = 0u16;

        if mode == DegradeMode::Full {
            let helper_text = "Bivvy has templates available for commonly used package managers \
                               and tools found in your project to help you get started quickly. \
                               Select any you'd like to include. Leave out the ones you don't.";

            let helper_style = Style::default().fg(Palette::TEXT_DIM);
            let max_width = inner.width as usize;
            let words: Vec<&str> = helper_text.split_whitespace().collect();
            let mut current_line = String::new();

            for word in words {
                let test_line = if current_line.is_empty() {
                    word.to_string()
                } else {
                    format!("{} {}", current_line, word)
                };

                if test_line.len() <= max_width {
                    current_line = test_line;
                } else {
                    if y < inner.y + inner.height {
                        frame
                            .buffer_mut()
                            .set_string(inner.x, y, &current_line, helper_style);
                        y += 1;
                        lines_used += 1;
                    }
                    current_line = word.to_string();
                }
            }
            if !current_line.is_empty() && y < inner.y + inner.height {
                frame
                    .buffer_mut()
                    .set_string(inner.x, y, &current_line, helper_style);
                y += 1;
                lines_used += 1;
            }

            // Spacing after helper text
            y += 1;
        }

        // Calculate remaining area for card list
        let spacing_offset = if mode == DegradeMode::Full {
            lines_used + 2
        } else {
            0
        };
        let list_area = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: inner.height.saturating_sub(spacing_offset),
        };

        if list_area.height == 0 {
            return;
        }

        // Build card list items (multi-select behavior is in toggle_selection)
        let items: Vec<CardListItem> = self
            .step_templates
            .iter()
            .enumerate()
            .map(|(i, t)| {
                CardListItem::new(&t.name, &t.reason, t.selected, i == self.selected_index)
            })
            .collect();

        let card_list = CardList::new(items)
            .spacing(CardListSpacing::Compact)
            .scroll_offset(self.scroll_offset);

        // Update visible_count for scrolling calculations
        let visible = card_list.items_that_fit(list_area.height);
        self.visible_count.set(visible);

        card_list.render(frame, list_area);

        // Register hit regions for each visible step template item.
        // CardList default card_height is 4; compact spacing has gap 0.
        let card_height = 4u16;
        let gap = CardListSpacing::Compact.gap();
        let start = self.scroll_offset;
        let end = (start + visible).min(self.step_templates.len());
        let mut item_y = list_area.y;
        for i in start..end {
            let item_rect = Rect::new(list_area.x, item_y, list_area.width, card_height);
            hits.register(HitRegion {
                area: item_rect,
                action: Action::Toggle,
                index: Some(i),
                click: ClickBehavior::FocusThenFire,
            });
            item_y += card_height + gap;
        }
    }

    fn render_summary(&self, frame: &mut Frame, area: Rect, mode: DegradeMode) {
        let card = SectionCard::new().title("SUMMARY").degrade_mode(mode);
        let inner = card.render(frame, area);

        if inner.height == 0 {
            return;
        }

        let selected_steps = self.get_selected_steps();
        let step_count = selected_steps.len();
        let total_count = self.step_templates.len();

        // Two column grid
        let col_width = inner.width / 2;
        let y = inner.y;

        // Column 1: Selected Steps
        frame.buffer_mut().set_string(
            inner.x,
            y,
            "Selected Steps",
            Style::default().fg(Palette::TEXT_DIM),
        );
        let steps_display = if step_count == 0 {
            "none".to_string()
        } else {
            format!("{} of {}", step_count, total_count)
        };
        frame.buffer_mut().set_string(
            inner.x,
            y + 1,
            &steps_display,
            Style::default()
                .fg(if step_count > 0 {
                    Palette::TEAL
                } else {
                    Palette::TEXT_MUTED
                })
                .add_modifier(Modifier::BOLD),
        );

        // Column 2: Step names (if any selected)
        frame.buffer_mut().set_string(
            inner.x + col_width,
            y,
            "Templates",
            Style::default().fg(Palette::TEXT_DIM),
        );
        let names = if selected_steps.is_empty() {
            "—".to_string()
        } else {
            selected_steps.join(", ")
        };
        // Truncate if too long
        let max_len = (col_width as usize).saturating_sub(1);
        let names_display = if names.len() > max_len {
            format!("{}…", &names[..max_len.saturating_sub(1)])
        } else {
            names
        };
        frame.buffer_mut().set_string(
            inner.x + col_width,
            y + 1,
            &names_display,
            Style::default().fg(Palette::TEXT),
        );
    }
}

impl Default for InitScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for InitScreen {
    fn render(&self, frame: &mut Frame, area: Rect, _app_state: &AppState, hits: &mut HitMap) {
        // Clear with main background
        frame
            .buffer_mut()
            .set_style(area, Style::default().bg(Palette::BG));

        // Global padding (2 columns horizontal, 1 row vertical)
        let padded = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(2),
        };

        // 1. Get vertical region definitions from screen_layouts
        let regions = screen_layouts::init();

        // 2. Allocate heights
        let allocations = allocate_regions(padded.height, &regions);

        // 3. Compute Rect + DegradeMode for each region
        let mut y = padded.y;
        let mut region_areas: Vec<(&str, Rect, DegradeMode)> = Vec::new();
        for alloc in &allocations {
            let rect = Rect::new(padded.x, y, padded.width, alloc.height);
            region_areas.push((alloc.name, rect, alloc.mode));
            y += alloc.height;
        }

        // 4. Render each region, passing DegradeMode to components
        for &(name, rect, mode) in &region_areas {
            if rect.height == 0 {
                continue;
            }
            match name {
                "header" => {
                    self.render_header(frame, rect, hits);
                }
                "detection" => {
                    self.render_detections(frame, rect, mode);
                }
                "steps" => {
                    self.render_step_templates(frame, rect, hits, mode);
                }
                "summary" => {
                    self.render_summary(frame, rect, mode);
                }
                "footer" => {
                    let binding_set = self.bindings();
                    let footer_hints: Vec<KeyHint> = binding_set
                        .footer_bindings()
                        .iter()
                        .map(|b| KeyHint::new(b.key_label, b.description))
                        .collect();
                    let footer = FooterWidget::new(footer_hints);
                    footer.render(rect, frame.buffer_mut(), &self.theme);
                }
                _ => {}
            }
        }

        // 5. Render help overlay on top of everything
        if self.show_help {
            self.render_help_overlay(frame, area);
        }
    }

    fn bindings(&self) -> BindingSet {
        let mut set = BindingSet::new();
        set.add(bindings::NAVIGATE);
        set.add(bindings::TOGGLE);
        set.add(bindings::CONFIRM);
        set.add(bindings::CANCEL);
        set.add(bindings::QUIT);
        set.add(bindings::HELP);
        set
    }

    fn handle_action(&mut self, action: Action, _state: &mut AppState) -> ScreenResult {
        // When help is showing, Help toggles it off; any other action dismisses it
        if self.show_help {
            self.show_help = false;
            return ScreenResult::Continue;
        }

        match action {
            Action::Quit | Action::Cancel => {
                self.cancelled = true;
                ScreenResult::Quit
            }
            Action::Up => {
                self.move_selection(-1);
                ScreenResult::Continue
            }
            Action::Down => {
                self.move_selection(1);
                ScreenResult::Continue
            }
            Action::Toggle => {
                self.toggle_selection();
                ScreenResult::Continue
            }
            Action::Confirm => {
                self.completed = true;
                ScreenResult::Quit
            }
            Action::ScrollUp => {
                self.move_selection(-1);
                ScreenResult::Continue
            }
            Action::ScrollDown => {
                self.move_selection(1);
                ScreenResult::Continue
            }
            Action::Help => {
                self.show_help = true;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_click(
        &mut self,
        action: Action,
        index: Option<usize>,
        click: ClickBehavior,
        state: &mut AppState,
    ) -> ScreenResult {
        match click {
            ClickBehavior::Fire => self.handle_action(action, state),
            ClickBehavior::FocusThenFire => {
                if let Some(idx) = index {
                    if idx == self.selected_index {
                        // Already focused — fire the action (toggle)
                        self.handle_action(action, state)
                    } else {
                        // Focus first
                        self.selected_index = idx;
                        self.ensure_visible();
                        ScreenResult::Continue
                    }
                } else {
                    ScreenResult::Continue
                }
            }
            ClickBehavior::FocusOnly => {
                if let Some(idx) = index {
                    self.selected_index = idx;
                    self.ensure_visible();
                }
                ScreenResult::Continue
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_result_creation() {
        let result = DetectionResult::new("Ruby", "Gemfile");
        assert_eq!(result.name, "Ruby");
        assert_eq!(result.detail, Some("Gemfile".to_string()));
        assert!(result.found);
    }

    #[test]
    fn detection_result_not_found() {
        let result = DetectionResult::not_found("Python");
        assert_eq!(result.name, "Python");
        assert!(result.detail.is_none());
        assert!(!result.found);
    }

    #[test]
    fn step_template_creation() {
        let template = StepTemplate::new("cargo", "Detected Cargo.toml");
        assert_eq!(template.name, "cargo");
        assert_eq!(template.reason, "Detected Cargo.toml");
        assert!(!template.selected);
    }

    #[test]
    fn step_template_new_selected() {
        let template = StepTemplate::new_selected("bundler", "Detected Gemfile");
        assert_eq!(template.name, "bundler");
        assert!(template.selected);
    }

    #[test]
    fn init_screen_navigation() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("brew", "System package manager"));
        screen.add_step_template(StepTemplate::new("mise", "Version manager"));

        assert_eq!(screen.selected_index, 0);

        screen.move_selection(1);
        assert_eq!(screen.selected_index, 1);

        screen.move_selection(1);
        assert_eq!(screen.selected_index, 1); // Can't go past end

        screen.move_selection(-1);
        assert_eq!(screen.selected_index, 0);
    }

    #[test]
    fn init_screen_confirm_action() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new_selected("bundler", "Detected Gemfile"));
        let mut app_state = AppState::default();

        let result = screen.handle_action(Action::Confirm, &mut app_state);

        assert!(matches!(result, ScreenResult::Quit));
        assert!(screen.completed);
        assert!(!screen.cancelled);

        if let InitResult::Confirmed(steps) = screen.result() {
            assert_eq!(steps, vec!["bundler".to_string()]);
        } else {
            panic!("Expected Confirmed result");
        }
    }

    #[test]
    fn init_screen_quit_action() {
        let mut screen = InitScreen::new();
        let mut app_state = AppState::default();

        let result = screen.handle_action(Action::Quit, &mut app_state);

        assert!(matches!(result, ScreenResult::Quit));
        assert!(screen.cancelled);
    }

    #[test]
    fn init_screen_cancel_action() {
        let mut screen = InitScreen::new();
        let mut app_state = AppState::default();

        let result = screen.handle_action(Action::Cancel, &mut app_state);

        assert!(matches!(result, ScreenResult::Quit));
        assert!(screen.cancelled);
    }

    #[test]
    fn init_screen_navigation_actions() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("brew", "System package manager"));
        screen.add_step_template(StepTemplate::new("mise", "Version manager"));
        let mut app_state = AppState::default();

        assert_eq!(screen.selected_index, 0);

        screen.handle_action(Action::Down, &mut app_state);
        assert_eq!(screen.selected_index, 1);

        screen.handle_action(Action::Up, &mut app_state);
        assert_eq!(screen.selected_index, 0);
    }

    #[test]
    fn init_screen_toggle_action() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("bundler", "Detected Gemfile"));
        let mut app_state = AppState::default();

        assert!(!screen.step_templates[0].selected);

        screen.handle_action(Action::Toggle, &mut app_state);
        assert!(screen.step_templates[0].selected);

        screen.handle_action(Action::Toggle, &mut app_state);
        assert!(!screen.step_templates[0].selected);
    }

    #[test]
    fn init_screen_scroll_actions() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("brew", "System package manager"));
        screen.add_step_template(StepTemplate::new("mise", "Version manager"));
        let mut app_state = AppState::default();

        screen.handle_action(Action::ScrollDown, &mut app_state);
        assert_eq!(screen.selected_index, 1);

        screen.handle_action(Action::ScrollUp, &mut app_state);
        assert_eq!(screen.selected_index, 0);
    }

    #[test]
    fn init_screen_toggle_selection_multiselect() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("bundler", "Detected Gemfile"));
        screen.add_step_template(StepTemplate::new("cargo", "Detected Cargo.toml"));

        assert!(!screen.step_templates[0].selected);
        screen.toggle_selection();
        assert!(screen.step_templates[0].selected);
        assert!(!screen.step_templates[1].selected);

        // Multi-select: selecting another does NOT deselect previous
        screen.move_selection(1);
        screen.toggle_selection();
        assert!(screen.step_templates[0].selected); // Still selected
        assert!(screen.step_templates[1].selected); // Also selected

        // Toggling again deselects
        screen.toggle_selection();
        assert!(screen.step_templates[0].selected);
        assert!(!screen.step_templates[1].selected);
    }

    #[test]
    fn init_screen_get_selected_steps() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new_selected("bundler", "Detected Gemfile"));
        screen.add_step_template(StepTemplate::new_selected("cargo", "Detected Cargo.toml"));
        screen.add_step_template(StepTemplate::new("yarn", "Detected package.json"));

        let selected = screen.get_selected_steps();
        assert_eq!(selected.len(), 2);
        assert!(selected.contains(&"bundler".to_string()));
        assert!(selected.contains(&"cargo".to_string()));
        assert!(!selected.contains(&"yarn".to_string()));
    }

    #[test]
    fn init_result_variants() {
        let confirmed = InitResult::Confirmed(vec!["test".to_string()]);
        let cancelled = InitResult::Cancelled;

        assert!(matches!(confirmed, InitResult::Confirmed(_)));
        assert!(matches!(cancelled, InitResult::Cancelled));
    }

    #[test]
    fn init_screen_default() {
        let screen = InitScreen::default();
        assert!(screen.step_templates.is_empty());
        assert!(screen.detections.is_empty());
        assert_eq!(screen.selected_index, 0);
    }

    #[test]
    fn init_screen_is_complete() {
        let mut screen = InitScreen::new();
        assert!(!screen.is_complete());

        screen.completed = true;
        assert!(screen.is_complete());

        screen.completed = false;
        screen.cancelled = true;
        assert!(screen.is_complete());
    }

    #[test]
    fn progress_step_all() {
        let steps = ProgressStep::all();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], ProgressStep::Detect);
        assert_eq!(steps[1], ProgressStep::Configure);
        assert_eq!(steps[2], ProgressStep::Confirm);
    }

    #[test]
    fn progress_step_label() {
        assert_eq!(ProgressStep::Detect.label(), "Detect");
        assert_eq!(ProgressStep::Configure.label(), "Configure");
        assert_eq!(ProgressStep::Confirm.label(), "Confirm");
    }

    #[test]
    fn progress_step_index() {
        assert_eq!(ProgressStep::Detect.index(), 0);
        assert_eq!(ProgressStep::Configure.index(), 1);
        assert_eq!(ProgressStep::Confirm.index(), 2);
    }

    #[test]
    fn init_screen_project_name() {
        let screen = InitScreen::new().project_name("test-project");
        assert_eq!(screen.project_name, "test-project");
    }

    #[test]
    fn init_screen_current_step() {
        let screen = InitScreen::new().current_step(ProgressStep::Confirm);
        assert_eq!(screen.current_step, ProgressStep::Confirm);
    }

    #[test]
    fn init_screen_bindings_resolve_keys() {
        let screen = InitScreen::new();
        let bindings = screen.bindings();

        // Should resolve standard keys
        let q = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::empty(),
        );
        assert_eq!(bindings.resolve(&q), Some(Action::Quit));

        let enter = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::empty(),
        );
        assert_eq!(bindings.resolve(&enter), Some(Action::Confirm));

        let space = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(' '),
            crossterm::event::KeyModifiers::empty(),
        );
        assert_eq!(bindings.resolve(&space), Some(Action::Toggle));
    }

    #[test]
    fn init_screen_bindings_footer() {
        let screen = InitScreen::new();
        let bindings = screen.bindings();
        let footer = bindings.footer_bindings();

        // Should have NAVIGATE, TOGGLE, CONFIRM, CANCEL, QUIT, HELP
        assert_eq!(footer.len(), 6);
    }

    #[test]
    fn init_screen_unknown_action_continues() {
        let mut screen = InitScreen::new();
        let mut app_state = AppState::default();

        // Actions not handled by init screen fall through to Continue
        let result = screen.handle_action(Action::Expand, &mut app_state);
        assert!(matches!(result, ScreenResult::Continue));
    }

    #[test]
    fn build_progress_line_has_spans() {
        let screen = InitScreen::new().current_step(ProgressStep::Configure);
        let line = screen.build_progress_line();
        // 3 steps × 2 spans (indicator + label) + 2 separators = 8
        assert_eq!(line.spans.len(), 8);
    }

    // ========================================================================
    // handle_click tests
    // ========================================================================

    #[test]
    fn handle_click_fire_delegates_to_action() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("brew", "test"));
        let mut app_state = AppState::default();

        let result =
            screen.handle_click(Action::Confirm, None, ClickBehavior::Fire, &mut app_state);
        assert!(matches!(result, ScreenResult::Quit));
        assert!(screen.completed);
    }

    #[test]
    fn handle_click_focus_then_fire_focuses_first() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("brew", "test"));
        screen.add_step_template(StepTemplate::new("mise", "test"));
        let mut app_state = AppState::default();

        assert_eq!(screen.selected_index, 0);

        // Click on item 1 — should focus, not toggle
        let result = screen.handle_click(
            Action::Toggle,
            Some(1),
            ClickBehavior::FocusThenFire,
            &mut app_state,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.selected_index, 1);
        assert!(!screen.step_templates[1].selected); // Not toggled yet
    }

    #[test]
    fn handle_click_focus_then_fire_fires_when_focused() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("brew", "test"));
        let mut app_state = AppState::default();

        assert_eq!(screen.selected_index, 0);

        // Click on already-focused item 0 — should toggle
        let result = screen.handle_click(
            Action::Toggle,
            Some(0),
            ClickBehavior::FocusThenFire,
            &mut app_state,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.step_templates[0].selected);
    }

    #[test]
    fn handle_click_focus_only_moves_selection() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("brew", "test"));
        screen.add_step_template(StepTemplate::new("mise", "test"));
        let mut app_state = AppState::default();

        let result = screen.handle_click(
            Action::Down,
            Some(1),
            ClickBehavior::FocusOnly,
            &mut app_state,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.selected_index, 1);
    }

    #[test]
    fn handle_click_focus_then_fire_no_index_continues() {
        let mut screen = InitScreen::new();
        screen.add_step_template(StepTemplate::new("brew", "test"));
        let mut app_state = AppState::default();

        let result = screen.handle_click(
            Action::Toggle,
            None,
            ClickBehavior::FocusThenFire,
            &mut app_state,
        );
        assert!(matches!(result, ScreenResult::Continue));
    }

    #[test]
    fn help_toggle() {
        let mut screen = InitScreen::new();
        let mut app_state = AppState::default();

        // Initially help is off
        assert!(!screen.show_help);

        // Help action opens the overlay
        let result = screen.handle_action(Action::Help, &mut app_state);
        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.show_help);

        // Any action while help is showing dismisses it
        let result = screen.handle_action(Action::Down, &mut app_state);
        assert!(matches!(result, ScreenResult::Continue));
        assert!(!screen.show_help);

        // The Down action was consumed by dismissing help, not navigation
        assert_eq!(screen.selected_index, 0);
    }

    #[test]
    fn help_toggle_via_help_key_dismisses() {
        let mut screen = InitScreen::new();
        let mut app_state = AppState::default();

        screen.handle_action(Action::Help, &mut app_state);
        assert!(screen.show_help);

        // Help key again also dismisses
        screen.handle_action(Action::Help, &mut app_state);
        assert!(!screen.show_help);
    }
}
