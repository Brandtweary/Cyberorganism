//! GUI rendering implementation using egui.
//!
//! This module handles the rendering of the task manager interface using egui.
//! It provides a minimalist interface similar to the previous TUI implementation.

#![allow(clippy::missing_const_for_fn)]

use crate::App;
use crate::commands::AppMode;
use crate::display_container::TaskIndex;
use crate::gui::keyhandler::KeyHandler;
use crate::taskstore::{Task, TaskStatus};
use eframe::egui;

/// The primary accent color used throughout the UI
const ACCENT_COLOR: egui::Color32 = egui::Color32::from_rgb(57, 255, 20);

/// Run the application with egui
pub fn run_app(app: App) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Cyberorganism",
        options,
        Box::new(|cc| {
            // Set up global visuals with our accent color
            let mut visuals = cc.egui_ctx.style().visuals.clone();
            visuals.selection.bg_fill = ACCENT_COLOR;
            visuals.selection.stroke.color = ACCENT_COLOR;
            visuals.widgets.noninteractive.fg_stroke.color = ACCENT_COLOR.linear_multiply(0.7);
            visuals.widgets.inactive.fg_stroke.color = ACCENT_COLOR.linear_multiply(0.8);
            visuals.widgets.active.fg_stroke.color = ACCENT_COLOR;
            visuals.widgets.hovered.fg_stroke.color = ACCENT_COLOR;
            cc.egui_ctx.set_visuals(visuals);

            // Increase font size for all text styles
            let mut style = (*cc.egui_ctx.style()).clone();
            for font_id in style.text_styles.values_mut() {
                font_id.size *= 1.2;
            }
            cc.egui_ctx.set_style(style);

            Box::new(GuiApp::new(app))
        }),
    )
}

/// GUI application state wrapper
struct GuiApp {
    /// The main application state
    app: App,
    /// Input field text
    input_text: String,
    /// Key handler for input processing
    key_handler: KeyHandler,
}

impl GuiApp {
    fn new(app: App) -> Self {
        Self {
            app,
            input_text: String::new(),
            key_handler: KeyHandler::new(),
        }
    }

    /// Format a task for display, including indentation, index, status, etc.
    fn format_task_text(&self, task: &Task, task_index: &TaskIndex, depth: usize) -> String {
        let mut task_text = String::new();

        // Add indentation
        for _ in 0..depth {
            task_text.push_str("  "); // Two spaces per level
        }

        // Add task index
        task_text.push_str(&format!("{task_index}"));

        // Add period after index for top-level tasks
        if depth == 0 {
            task_text.push_str(". ");
        } else {
            task_text.push(' ');
        }

        // Note: We no longer add completion status indicator here since we'll use the Checkbox widget

        // Add expansion indicator if task has children
        if !task.child_ids.is_empty() {
            task_text.push_str(
                if self.app.display_container_state.is_task_expanded(task.id) {
                    // "▼ "  # doesn't render correctly in egui for some reason
                    ""
                } else {
                    "▶ "
                },
            );
        }

        // Add task content
        task_text.push_str(&task.content);
        task_text
    }

    /// Render a single task (without handling interactions)
    fn render_single_task(
        &self,
        ui: &mut egui::Ui,
        task: &Task,
        task_index: &TaskIndex,
        depth: usize,
        is_focused: bool,
    ) -> (egui::Response, Option<u32>) {
        let task_text = self.format_task_text(task, task_index, depth);
        let task_id = task.id;
        let mut task_to_complete = None;

        // Only apply highlighting if we're in PKM mode
        let should_highlight =
            is_focused && matches!(self.app.app_mode, crate::commands::AppMode::Pkm);

        // Create a frame that will have the background color if focused
        let frame = if should_highlight {
            egui::Frame::none()
                .fill(ACCENT_COLOR)
                .inner_margin(egui::style::Margin::symmetric(4.0, 0.0))
        } else {
            egui::Frame::none()
        };

        // Use the frame to create a container with the right background
        let response = frame
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Create a mutable copy of the task's status for the checkbox
                    let mut is_checked = task.status == TaskStatus::Done;
                    let checkbox_response = ui.checkbox(&mut is_checked, "");

                    // If the checkbox was clicked and the status changed, mark for completion
                    if checkbox_response.changed() && is_checked {
                        task_to_complete = Some(task_id);
                    }

                    // Render the task text with the appropriate style and ensure it wraps
                    let text = if should_highlight {
                        egui::RichText::new(task_text).color(egui::Color32::BLACK)
                    } else {
                        egui::RichText::new(task_text)
                    };

                    // Use a label with wrapping to ensure text stays within bounds
                    ui.add(
                        egui::Label::new(text).wrap(true), // Enable text wrapping
                    );
                })
                .response
            })
            .response;

        (response, task_to_complete)
    }

    /// Render the task list
    fn render_tasks(&mut self, ui: &mut egui::Ui) {
        // Use accent color for the scrollable area
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Create a header for the task list with consistent padding
            egui::Frame::none()
                .inner_margin(egui::style::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    let container_name = self
                        .app
                        .display_container_state
                        .active_container
                        .display_name();
                    let capitalized_name = container_name
                        .chars()
                        .next()
                        .unwrap_or_default()
                        .to_uppercase()
                        .collect::<String>()
                        + &container_name[1..];
                    ui.heading(capitalized_name);
                });

            ui.separator();

            // Add a margin to all content to prevent hugging the edge
            egui::Frame::none()
                .inner_margin(egui::style::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    // Get the current focused index
                    let focused_index = self.app.display_container_state.focused_index;

                    // Render the "Create new task" option (index 0)
                    let is_focused = focused_index == Some(0);
                    let text = "<Create new task or enter commands>";

                    let response = if is_focused {
                        ui.selectable_label(
                            true,
                            egui::RichText::new(text)
                                .background_color(ACCENT_COLOR)
                                .color(egui::Color32::BLACK),
                        )
                    } else {
                        ui.selectable_label(false, text)
                    };

                    // Handle clicks on "Create new task"
                    if response.clicked() {
                        self.app.display_container_state.focused_index = Some(0);
                        self.input_text = String::new();
                    }

                    // Collect all tasks and their metadata before rendering
                    // This avoids borrowing issues during the recursive rendering
                    let mut task_data = Vec::new();
                    let active_container = self.app.display_container_state.active_container;

                    // First collect top-level tasks
                    for (idx, task) in self
                        .app
                        .tasks
                        .iter()
                        .filter(|t| t.container == active_container && t.parent_id.is_none())
                        .enumerate()
                    {
                        let mut current_index = vec![idx + 1]; // 1-based index
                        self.collect_task_data(task, &mut current_index, &mut task_data);
                    }

                    // Now render all tasks and collect responses
                    let mut display_index = 1; // Start at 1 because 0 is "Create new task"
                    let mut all_responses = Vec::new();

                    // We need to handle each task one at a time to avoid multiple mutable borrows
                    for (task_id, task_index, depth) in &task_data {
                        let task = self.app.tasks.iter().find(|t| t.id == *task_id).unwrap();
                        let is_focused = focused_index == Some(display_index);

                        // Render the task and get the response
                        let (response, task_to_complete) =
                            self.render_single_task(ui, task, task_index, *depth, is_focused);

                        all_responses.push((response, task_to_complete, display_index));
                        display_index += 1;
                    }

                    // Handle UI interactions after rendering is complete
                    for (response, task_to_complete, display_idx) in all_responses {
                        // Handle click to select task
                        if response.clicked() {
                            self.app.display_container_state.focused_index = Some(display_idx);

                            // Find the task to update input text
                            if let Some(task) = self
                                .app
                                .tasks
                                .iter()
                                .find(|t| t.id == task_to_complete.unwrap_or(0))
                            {
                                self.input_text.clone_from(&task.content);
                            }
                        }

                        // Handle double-click to toggle expansion
                        if response.double_clicked() {
                            if let Some(task) = self
                                .app
                                .tasks
                                .iter()
                                .find(|t| t.id == task_to_complete.unwrap_or(0))
                            {
                                if !task.child_ids.is_empty() {
                                    self.app
                                        .display_container_state
                                        .toggle_task_expansion(task.id, &self.app.tasks);
                                }
                            }
                        }

                        // Handle task completion
                        if let Some(task_id) = task_to_complete {
                            crate::commands::execute_complete_by_id_command(&mut self.app, task_id);
                        }
                    }
                });
        });
    }

    /// Collect task data for rendering
    /// This avoids borrow checker issues by collecting all data before rendering
    fn collect_task_data(
        &self,
        task: &Task,
        current_index: &mut Vec<usize>,
        task_data: &mut Vec<(u32, TaskIndex, usize)>,
    ) {
        // Store task ID, index, and depth
        let depth = current_index.len() - 1;
        task_data.push((
            task.id,
            TaskIndex {
                path: current_index.clone(),
            },
            depth,
        ));

        // Recursively collect child tasks if expanded
        if self.app.display_container_state.is_task_expanded(task.id) {
            for (child_idx, &child_id) in task.child_ids.iter().enumerate() {
                if let Some(child_task) = self.app.tasks.iter().find(|t| t.id == child_id) {
                    current_index.push(child_idx + 1); // 1-based index
                    self.collect_task_data(child_task, current_index, task_data);
                    current_index.pop();
                }
            }
        }
    }

    /// Render the activity log
    fn render_activity_log(&self, ui: &mut egui::Ui) {
        if let Some(message) = self.app.activity_log.latest_message() {
            // Add consistent margins to match the task list and input field
            egui::Frame::none()
                .inner_margin(egui::style::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    // Use a label with explicit wrapping to ensure text stays within bounds
                    ui.add(
                        egui::Label::new(message).wrap(true), // Enable text wrapping
                    );
                });
        }
    }

    /// Render the help text
    #[allow(clippy::unused_self)]
    fn render_help(&self, ui: &mut egui::Ui) {
        // Add a margin to match the task list
        egui::Frame::none()
            .inner_margin(egui::style::Margin::symmetric(8.0, 4.0))
            .show(ui, |ui| {
                // Use a label with explicit wrapping to ensure text stays within bounds
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Help: Enter = execute | Shift+Enter = subtask | Ctrl+Enter = toggle done | Ctrl+Up/Down = expand/collapse")
                            .color(ACCENT_COLOR)
                    )
                    .wrap(true) // Enable text wrapping
                );
            });
    }

    /// Render the input field
    fn render_input(&mut self, ui: &mut egui::Ui) {
        // Add consistent margins to match the task list
        egui::Frame::none()
            .inner_margin(egui::style::Margin::symmetric(8.0, 4.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    // Customize the visuals to make the border always visible
                    // Store the original visuals
                    let original_inactive = ui.visuals().widgets.inactive;
                    let original_active = ui.visuals().widgets.active;

                    // Modify the visuals for this scope
                    ui.visuals_mut().widgets.inactive.bg_stroke =
                        egui::Stroke::new(1.0, ACCENT_COLOR);
                    ui.visuals_mut().widgets.active.bg_stroke =
                        egui::Stroke::new(2.0, ACCENT_COLOR);

                    // Store the current text length for cursor positioning
                    let cursor_pos = self.input_text.len();

                    // Use a custom text edit with a visible background
                    let text_edit = egui::TextEdit::singleline(&mut self.input_text)
                        .desired_width(ui.available_width()) // Make it take full available width
                        .hint_text("Enter task or command...")
                        .id(egui::Id::new("main_input_field")); // Use a consistent ID

                    // Request focus on the text edit
                    let response = text_edit.show(ui).response;

                    // Restore the original visuals
                    ui.visuals_mut().widgets.inactive = original_inactive;
                    ui.visuals_mut().widgets.active = original_active;

                    // Only request focus during initial startup or when explicitly requested
                    if self.app.display_container_state.initial_startup {
                        response.request_focus();
                        self.app.display_container_state.initial_startup = false;
                    } else if self.app.display_container_state.request_focus_next_frame {
                        response.request_focus();
                        self.app.display_container_state.request_focus_next_frame = false;
                    }

                    // Synchronize input text with DisplayContainerState if needed
                    if self.app.display_container_state.sync_input_with_gui {
                        self.input_text =
                            self.app.display_container_state.input_value().to_string();
                        self.app.display_container_state.sync_input_with_gui = false;
                    }

                    // Handle cursor positioning
                    if self.app.display_container_state.request_cursor_at_end {
                        // Set cursor position to the end of the text
                        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                            let new_ccursor = egui::text::CCursor::new(cursor_pos);
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(new_ccursor)));
                            state.store(ui.ctx(), response.id);

                            // Try a second time to ensure the cursor position sticks
                            // This is a workaround for the cursor position issue with up arrow key
                            if let Some(mut state) =
                                egui::TextEdit::load_state(ui.ctx(), response.id)
                            {
                                let new_ccursor = egui::text::CCursor::new(cursor_pos);
                                state
                                    .cursor
                                    .set_char_range(Some(egui::text::CCursorRange::one(
                                        new_ccursor,
                                    )));
                                state.store(ui.ctx(), response.id);
                            }
                        }
                        self.app.display_container_state.request_cursor_at_end = false;
                    }

                    // NOTE: Enter key handling is done in keyhandler.rs
                    // Do not handle Enter key here to avoid conflicts

                    // Display current mode below the input field
                    let mode_text = "PKM Mode";
                    ui.label(egui::RichText::new(mode_text).small().weak());
                });
            });
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle keyboard input
        // We don't need to check the return value since we're only in PKM mode now
        self.key_handler
            .handle_input(&mut self.app, ctx, &mut self.input_text);

        // Set up the main layout with a central panel
        egui::CentralPanel::default().show(ctx, |ui| {
            // Calculate available height and allocate space for components
            let available_height = ui.available_height();

            // Reserve space for fixed-height components
            let activity_log_height = 30.0; // Approximate height for activity log
            let help_text_height = 30.0; // Approximate height for help text
            let input_field_height = 35.0; // Slightly increased to provide bottom padding
            let bottom_padding = 5.0; // Extra padding at the bottom of the screen

            // Calculate remaining height for the task list
            let task_list_height = available_height
                - activity_log_height
                - help_text_height
                - input_field_height
                - bottom_padding;

            ui.vertical(|ui| {
                // Render the task list with a fixed height (only in PKM mode)
                if matches!(self.app.app_mode, AppMode::Pkm) {
                    // Set a minimum height to ensure the task list is always visible
                    let task_list_height = task_list_height.max(200.0);

                    // Create a container with fixed height for the task list
                    egui::Frame::none()
                        .fill(ui.visuals().extreme_bg_color)
                        .inner_margin(0.0)
                        .outer_margin(0.0)
                        .show(ui, |ui| {
                            ui.set_min_height(task_list_height);
                            ui.set_max_height(task_list_height);
                            self.render_tasks(ui);
                        });
                }

                // Render the activity log (only in PKM mode)
                if matches!(self.app.app_mode, AppMode::Pkm) {
                    self.render_activity_log(ui);
                }

                // Render the help text
                self.render_help(ui);

                // Render the input field
                self.render_input(ui);
            });
        });

        // Request focus to the input field if needed
        if self.app.display_container_state.request_focus_next_frame {
            ctx.memory_mut(|mem| mem.request_focus(egui::Id::new("main_input")));
            self.app.display_container_state.request_focus_next_frame = false;
        }
    }
}
