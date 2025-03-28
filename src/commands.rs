//! Command processing for cyberorganism. Translates user
//! input into task management operations.

use chrono::Utc;

use crate::App;
use crate::debug::log_debug;
use crate::taskstore::{
    Task, TaskContainer, TaskStatus, find_task_by_content, find_task_by_id, save_tasks,
};

/// Represents the application interaction mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Default mode for PKM task management
    Pkm,
}

impl Default for AppMode {
    fn default() -> Self {
        Self::Pkm
    }
}

/// Commands that can be executed by the user
pub enum Command {
    Create(String),
    Complete(String),
    CompleteById(u32),
    Delete(String),
    MoveToTaskpad(String),
    MoveToBackburner(String),
    MoveToShelved(String),
    Edit(u32, String),          // (task_id, new_content)
    Focus(String),              // Focus on a task by index or content
    Show(TaskContainer),        // Switch active container
    AddSubtask(String, String), // (parent_query, subtask_content)
    Toggle(String),             // Toggle expansion state of a task
    ToggleAppMode,              // Toggle application mode
}

/// Parses the input string into a Command
#[allow(clippy::option_if_let_else)]
pub fn parse_command(input: String) -> Command {
    if let Some(task_query) = input.strip_prefix("complete ") {
        Command::Complete(task_query.to_string())
    } else if let Some(task_query) = input.strip_prefix("delete ") {
        Command::Delete(task_query.to_string())
    } else if let Some(task_query) = input.strip_prefix("move to taskpad ") {
        Command::MoveToTaskpad(task_query.to_string())
    } else if let Some(task_query) = input.strip_prefix("move to backburner ") {
        Command::MoveToBackburner(task_query.to_string())
    } else if let Some(task_query) = input.strip_prefix("move to shelved ") {
        Command::MoveToShelved(task_query.to_string())
    } else if let Some(task_query) = input.strip_prefix("focus ") {
        Command::Focus(task_query.to_string())
    } else if let Some(task_query) = input.strip_prefix("toggle ") {
        Command::Toggle(task_query.to_string())
    } else if let Some(container) = input.strip_prefix("show ") {
        match container {
            "taskpad" => Command::Show(TaskContainer::Taskpad),
            "backburner" => Command::Show(TaskContainer::Backburner),
            "shelved" => Command::Show(TaskContainer::Shelved),
            "archived" => Command::Show(TaskContainer::Archived),
            _ => Command::Create(input), // Invalid container, treat as task creation
        }
    } else if let Some(task_query) = input.strip_prefix("subtask ") {
        // Format: "subtask <parent_query> <content>"
        let parts: Vec<&str> = task_query.splitn(2, ' ').collect();
        if parts.len() == 2 {
            return Command::AddSubtask(parts[0].to_string(), parts[1].to_string());
        }
        Command::Create(input) // Invalid format, treat as task creation
    } else if input.to_lowercase() == "toggle app mode" {
        Command::ToggleAppMode
    } else {
        Command::Create(input)
    }
}

/// Result of executing a command
enum CommandResult {
    TaskCompleted { content: String },
    TaskAlreadyArchived(String),
    NoMatchingTask,
}

/// Finds a task by display index (including dot notation) or content match
fn find_task(app: &App, query: &str) -> Option<usize> {
    use regex::Regex;
    let query = query.trim();

    // Check for dot notation pattern (e.g., "1.2.3" or "1.2.")
    let dot_pattern = Regex::new(r"^\d+(\.\d+)*\.?$").unwrap();
    if dot_pattern.is_match(query) {
        // Remove trailing dot if present
        let clean_query = query.trim_end_matches('.');
        if let Some(task_id) = app
            .display_container_state
            .get_task_id_by_path(clean_query, &app.tasks)
        {
            return find_task_by_id(&app.tasks, task_id);
        }
    }
    // Check for simple index (backwards compatibility)
    else if query.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(index) = query.parse::<usize>() {
            // Convert single number to dot notation
            if let Some(task_id) = app
                .display_container_state
                .get_task_id_by_path(&index.to_string(), &app.tasks)
            {
                return find_task_by_id(&app.tasks, task_id);
            }
        }
    }

    // Fall back to fuzzy content match if no index match found
    find_task_by_content(
        &app.tasks,
        query,
        app.display_container_state.active_container,
    )
}

/// Completes a task by content match or task ID
fn complete_task(app: &mut App, query: &str, task_id: Option<u32>) -> CommandResult {
    // If task_id is provided, complete that task directly
    if let Some(id) = task_id {
        if let Some(task) = app.tasks.iter_mut().find(|t| t.id == id) {
            if matches!(task.container(), TaskContainer::Archived) {
                return CommandResult::TaskAlreadyArchived(task.content.clone());
            }
            let content = task.content.clone();
            task.complete();
            app.display_container_state.update_display_order(&app.tasks);
            return CommandResult::TaskCompleted { content };
        }
        return CommandResult::NoMatchingTask;
    }

    // Otherwise, search for task by content
    if let Some(idx) = find_task(app, query) {
        let task = &mut app.tasks[idx];
        if matches!(task.container(), TaskContainer::Archived) {
            return CommandResult::TaskAlreadyArchived(task.content.clone());
        }
        let content = task.content.clone();
        task.complete();
        app.display_container_state.update_display_order(&app.tasks);
        CommandResult::TaskCompleted { content }
    } else {
        CommandResult::NoMatchingTask
    }
}

/// Execute a create command
pub fn execute_create_command(app: &mut App, content: &str) -> u32 {
    let task = Task::new(app.next_id, content.to_string());
    let task_id = task.id;
    app.next_id += 1;
    app.add_task(task);
    app.log_activity("Task added".to_string());
    if let Err(e) = save_tasks(&app.tasks, &app.tasks_file) {
        log_debug(&format!("Failed to save tasks: {e}"));
    }
    task_id
}

/// Execute a complete command
fn execute_complete_command(app: &mut App, query: &str) {
    match complete_task(app, query, None) {
        CommandResult::TaskCompleted { content } => {
            app.log_activity(format!("Completed task: {content}"));
            if let Err(e) = save_tasks(&app.tasks, &app.tasks_file) {
                app.log_activity(format!("Error saving tasks: {e}"));
            }
        }
        CommandResult::TaskAlreadyArchived(content) => {
            app.log_activity(format!("Task '{content}' is already archived"));
        }
        CommandResult::NoMatchingTask => {
            app.log_activity("No matching task found".to_string());
        }
    }
}

/// Execute a complete by ID command
pub fn execute_complete_by_id_command(app: &mut App, task_id: u32) {
    match complete_task(app, "", Some(task_id)) {
        CommandResult::TaskCompleted { content } => {
            app.log_activity(format!("Completed task: {content}"));
            if let Err(e) = save_tasks(&app.tasks, &app.tasks_file) {
                app.log_activity(format!("Error saving tasks: {e}"));
            }
        }
        CommandResult::TaskAlreadyArchived(content) => {
            app.log_activity(format!("Task '{content}' is already archived"));
        }
        CommandResult::NoMatchingTask => {
            app.log_activity("No matching task found".to_string());
        }
    }
}

/// Execute a delete command
fn execute_delete_command(app: &mut App, query: &str) {
    if let Some(index) = find_task(app, query) {
        let task = &app.tasks[index];
        let content = task.content.clone();
        let task_id = task.id;

        // If this task has a parent, remove it from the parent's child_ids
        if let Some(parent_id) = task.parent_id {
            if let Some(parent_index) = app.tasks.iter().position(|t| t.id == parent_id) {
                app.remove_child_from_parent(parent_index, task_id);
            }
        }

        app.remove_task(index);
        app.log_activity(format!("Deleted task: {content}"));
        if let Err(e) = save_tasks(&app.tasks, &app.tasks_file) {
            log_debug(&format!("Failed to save tasks: {e}"));
        }
    } else {
        app.log_activity("No matching task found".to_string());
    }
}

/// Execute a move command
fn execute_move_command(app: &mut App, query: &str, target_container: TaskContainer) {
    if let Some(index) = find_task(app, query) {
        let task = &mut app.tasks[index];
        if task.container == target_container {
            app.log_activity(format!(
                "Task already in {}",
                target_container.display_name()
            ));
        } else {
            let content = task.content.clone();
            let container_name = target_container.display_name();
            task.container = target_container;

            // Save tasks after moving one
            if let Err(e) = save_tasks(&app.tasks, &app.tasks_file) {
                log_debug(&format!("Failed to save tasks: {e}"));
            }

            app.log_activity(format!("Moved task to {container_name}: {content}"));
        }
    } else {
        app.log_activity("No matching task found".to_string());
    }
}

/// Execute a move to taskpad command
fn execute_move_to_taskpad_command(app: &mut App, query: &str) {
    execute_move_command(app, query, TaskContainer::Taskpad);
}

/// Execute a move to backburner command
fn execute_move_to_backburner_command(app: &mut App, query: &str) {
    execute_move_command(app, query, TaskContainer::Backburner);
}

/// Execute a move to shelved command
fn execute_move_to_shelved_command(app: &mut App, query: &str) {
    execute_move_command(app, query, TaskContainer::Shelved);
}

/// Execute show container command
fn execute_show_command(app: &mut App, container: TaskContainer) {
    let display_name = container.display_name().to_string();
    app.display_container_state.active_container = container;
    app.display_container_state.update_display_order(&app.tasks);
    app.activity_log
        .add_message(format!("Showing {display_name} tasks"));
}

/// Result of focusing on a task
enum FocusResult {
    Focused { content: String },
    NoMatchingTask,
}

/// Focuses on a task by content match or display index
fn focus_task(app: &mut App, query: &str) -> FocusResult {
    if let Some(index) = find_task(app, query) {
        let task = &app.tasks[index];
        let content = task.content.clone();

        if app
            .display_container_state
            .focus_task_and_update_input(Some(task.id), &app.tasks)
        {
            FocusResult::Focused { content }
        } else {
            FocusResult::NoMatchingTask
        }
    } else {
        FocusResult::NoMatchingTask
    }
}

/// Executes the focus command
fn execute_focus_command(app: &mut App, query: &str) {
    match focus_task(app, query) {
        FocusResult::Focused { content } => {
            app.log_activity(format!("Focused on task: {content}"));
        }
        FocusResult::NoMatchingTask => {
            app.log_activity("No matching task found".to_string());
        }
    }
}

/// Execute edit command
fn execute_edit_command(app: &mut App, task_id: u32, content: String) {
    if let Some(_task) = app.tasks.iter_mut().find(|t| t.id == task_id) {
        app.update_task(
            app.tasks.iter().position(|t| t.id == task_id).unwrap(),
            |task| {
                task.update_content(content);
            },
        );
        app.activity_log.add_message("Task updated".to_string());
        if let Err(e) = save_tasks(&app.tasks, &app.tasks_file) {
            log_debug(&format!("Failed to save tasks: {e}"));
        }
    } else {
        app.activity_log
            .add_message(format!("No task found with ID {task_id}"));
    }
}

/// Execute add subtask command
///
/// Can be called with either:
/// - A query string to find the parent task
/// - A direct parent task ID
pub fn execute_add_subtask(app: &mut App, query_or_id: &str, content: &str) -> Option<u32> {
    // Check if query_or_id is a task ID (u32)
    let parent_idx = if let Ok(parent_id) = query_or_id.parse::<u32>() {
        // If it's a valid u32, find the task by ID
        find_task_by_id(&app.tasks, parent_id)
    } else {
        // Otherwise use the regular find_task function
        find_task(app, query_or_id)
    };

    // Create the subtask if parent was found
    if let Some(parent_idx) = parent_idx {
        let parent_id = app.tasks[parent_idx].id;
        let parent_content = app.tasks[parent_idx].content.clone();

        // Create a new subtask
        let subtask = Task {
            id: app.next_id,
            content: content.to_string(),
            created_at: Utc::now(),
            container: TaskContainer::Taskpad,
            status: TaskStatus::Todo,
            parent_id: Some(parent_id),
            child_ids: Vec::new(),
        };
        let subtask_id = subtask.id;
        app.next_id += 1;

        // Add subtask ID to parent's child_ids
        app.tasks[parent_idx].add_subtask(subtask_id);

        // Add the subtask to tasks list
        app.add_task(subtask);
        let msg = if content.is_empty() {
            format!("Added subtask to task '{parent_content}'")
        } else {
            format!("Added subtask to task '{parent_content}': {content}")
        };
        app.log_activity(msg);

        // Save updated task list
        if let Err(e) = save_tasks(&app.tasks, &app.tasks_file) {
            log_debug(&format!("Failed to save tasks: {e}"));
        }

        Some(subtask_id)
    } else {
        app.log_activity(format!("No task found matching '{query_or_id}'"));
        None
    }
}

/// Execute toggle expansion command
fn execute_toggle_command(app: &mut App, query: &str) {
    if let Some(index) = find_task(app, query) {
        let task_id = app.tasks[index].id;
        app.display_container_state
            .toggle_task_expansion(task_id, &app.tasks);
        app.log_activity(format!("Toggled task: {}", app.tasks[index].content));
    } else {
        app.log_activity("No matching task found".to_string());
    }
}

/// Toggle the application mode (now a no-op since we only have one mode)
pub fn toggle_app_mode(_app: &mut App, _current_mode: AppMode) -> AppMode {
    // Since we only have PKM mode now, always return PKM
    AppMode::Pkm
}

// Commenting out unused function to fix warning
/*
/// Get a string representation of the current app mode
pub fn app_mode_to_string(mode: AppMode) -> &'static str {
    match mode {
        AppMode::Pkm => "PKM",
    }
}
*/

/// Executes a command, updating the app state as needed
pub fn execute_command(app: &mut App, command: Option<Command>) -> Option<u32> {
    let result = match command {
        Some(Command::Create(content)) => Some(execute_create_command(app, &content)),
        Some(Command::Complete(query)) => {
            execute_complete_command(app, &query);
            None
        }
        Some(Command::CompleteById(id)) => {
            execute_complete_by_id_command(app, id);
            None
        }
        Some(Command::Delete(query)) => {
            execute_delete_command(app, &query);
            None
        }
        Some(Command::MoveToTaskpad(query)) => {
            execute_move_to_taskpad_command(app, &query);
            None
        }
        Some(Command::MoveToBackburner(query)) => {
            execute_move_to_backburner_command(app, &query);
            None
        }
        Some(Command::MoveToShelved(query)) => {
            execute_move_to_shelved_command(app, &query);
            None
        }
        Some(Command::Focus(query)) => {
            execute_focus_command(app, &query);
            None
        }
        Some(Command::Show(container)) => {
            execute_show_command(app, container);
            None
        }
        Some(Command::Edit(task_id, content)) => {
            execute_edit_command(app, task_id, content);
            None
        }
        Some(Command::AddSubtask(query, content)) => {
            execute_add_subtask(app, &query, &content);
            None
        }
        Some(Command::Toggle(query)) => {
            execute_toggle_command(app, &query);
            None
        }
        Some(Command::ToggleAppMode) => {
            app.app_mode = toggle_app_mode(app, app.app_mode);
            None
        }
        None => {
            app.activity_log.add_message("Invalid command".to_string());
            None
        }
    };

    // Update display after any command
    app.display_container_state.update_display_order(&app.tasks);
    app.show_help = false;

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Test utilities and conventions:
    /// - When checking activity log messages, always use `activity_log.latest_message()`
    ///   instead of trying to access the log entries directly. The ActivityLog struct
    ///   provides this method specifically for getting the most recent message.
    /// - Container names in messages are always lowercase (e.g., "taskpad", not "Taskpad")
    fn setup_test_app() -> App {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let tasks_file = temp_dir
            .path()
            .join("tasks.json")
            .to_str()
            .unwrap()
            .to_string();

        let mut app = App {
            tasks_file,
            ..Default::default()
        };
        app.add_task(Task::new(1, "Buy groceries".to_string()));
        app.add_task(Task::new(2, "Call dentist".to_string()));
        app.add_task(Task::new(3, "Write report".to_string()));
        app.next_id = 4;
        app
    }

    #[test]
    fn test_parse_command() {
        // Test create command (default)
        let cmd = parse_command("Buy milk".to_string());
        assert!(matches!(cmd, Command::Create(content) if content == "Buy milk"));

        // Test complete command
        let cmd = parse_command("complete Test task".to_string());
        assert!(matches!(cmd, Command::Complete(content) if content == "Test task"));

        // Test delete command
        let cmd = parse_command("delete Test task".to_string());
        assert!(matches!(cmd, Command::Delete(content) if content == "Test task"));

        // Test move to taskpad command
        let cmd = parse_command("move to taskpad Test task".to_string());
        assert!(matches!(cmd, Command::MoveToTaskpad(content) if content == "Test task"));

        // Test move to backburner command
        let cmd = parse_command("move to backburner Test task".to_string());
        assert!(matches!(cmd, Command::MoveToBackburner(content) if content == "Test task"));

        // Test move to shelved command
        let cmd = parse_command("move to shelved Test task".to_string());
        assert!(matches!(cmd, Command::MoveToShelved(content) if content == "Test task"));

        // Test focus command
        let cmd = parse_command("focus Test task".to_string());
        assert!(matches!(cmd, Command::Focus(content) if content == "Test task"));

        // Test show command
        let cmd = parse_command("show taskpad".to_string());
        assert!(matches!(cmd, Command::Show(container) if container == TaskContainer::Taskpad));

        // Test with trailing spaces in task content
        let cmd = parse_command("complete Test task  ".to_string());
        assert!(matches!(cmd, Command::Complete(content) if content == "Test task  "));

        // Test toggle command
        let cmd = parse_command("toggle Test task".to_string());
        assert!(matches!(cmd, Command::Toggle(content) if content == "Test task"));

        // Test subtask command
        let cmd = parse_command("subtask 1 subtask content".to_string());
        assert!(matches!(cmd, Command::AddSubtask(parent, content) 
            if parent == "1" && content == "subtask content"));
    }

    #[test]
    fn test_find_task_by_partial_content() {
        let app = setup_test_app();

        // Should match exact content
        let index = find_task(&app, "Buy groceries");
        assert!(index.is_some());
        assert_eq!(app.tasks[index.unwrap()].content, "Buy groceries");

        // Should not match partial content
        assert!(find_task(&app, "groceries").is_none());
    }

    #[test]
    fn test_find_task_by_exact_content() {
        let app = setup_test_app();

        // Find by exact content
        let index = find_task(&app, "Buy groceries");
        assert!(index.is_some());
        assert_eq!(app.tasks[index.unwrap()].content, "Buy groceries");
    }

    #[test]
    fn test_find_task_by_display_index() {
        let mut app = setup_test_app();
        app.display_container_state.update_display_order(&app.tasks);

        // Find by display index
        let index = find_task(&app, "1");
        assert!(index.is_some());
    }

    #[test]
    fn test_find_nonexistent_task() {
        let app = setup_test_app();
        assert!(find_task(&app, "nonexistent task").is_none());
    }

    #[test]
    fn test_find_deleted_task() {
        let mut app = setup_test_app();
        let initial_count = app.tasks.len();

        // First find and delete a task
        let index = find_task(&app, "Buy groceries").unwrap();
        app.tasks.remove(index);
        assert_eq!(app.tasks.len(), initial_count - 1);

        // Now try to find it again
        assert!(find_task(&app, "Buy groceries").is_none());
    }

    #[test]
    fn test_complete_task_success() {
        let mut app = setup_test_app();
        let result = complete_task(&mut app, "Buy groceries", None);
        assert!(
            matches!(result, CommandResult::TaskCompleted { content } if content == "Buy groceries")
        );
    }

    #[test]
    fn test_complete_already_archived_task() {
        let mut app = setup_test_app();

        // First complete the task
        let _ = complete_task(&mut app, "Buy groceries", None);

        // Try to complete it again
        let result = complete_task(&mut app, "Buy groceries", None);
        assert!(
            matches!(result, CommandResult::TaskAlreadyArchived(content) if content == "Buy groceries")
        );
    }

    #[test]
    fn test_complete_nonexistent_task() {
        let mut app = setup_test_app();
        let result = complete_task(&mut app, "nonexistent task", None);
        assert!(matches!(result, CommandResult::NoMatchingTask));
    }

    #[test]
    fn test_delete_task_by_content() {
        let mut app = setup_test_app();
        let initial_count = app.tasks.len();

        // Delete by content match
        execute_command(&mut app, Some(Command::Delete("Buy groceries".to_string())));
        assert_eq!(app.tasks.len(), initial_count - 1);
        assert!(app.tasks.iter().all(|t| t.content != "Buy groceries"));
    }

    #[test]
    fn test_delete_task_by_index() {
        let mut app = setup_test_app();
        let initial_count = app.tasks.len();

        // Update display order first
        app.display_container_state.update_display_order(&app.tasks);

        // Delete by index
        execute_command(&mut app, Some(Command::Delete("1".to_string())));
        assert_eq!(app.tasks.len(), initial_count - 1);
    }

    #[test]
    fn test_delete_nonexistent_task() {
        let mut app = setup_test_app();
        let initial_count = app.tasks.len();

        // Try to delete nonexistent task
        execute_command(
            &mut app,
            Some(Command::Delete("nonexistent task".to_string())),
        );
        assert_eq!(app.tasks.len(), initial_count);
    }

    #[test]
    fn test_delete_completed_task() {
        let mut app = setup_test_app();
        let initial_count = app.tasks.len();

        // First complete a task
        let _ = complete_task(&mut app, "Buy groceries", None);

        // Then delete it
        execute_command(&mut app, Some(Command::Delete("Buy groceries".to_string())));
        assert_eq!(app.tasks.len(), initial_count - 1);
        assert!(app.tasks.iter().all(|t| t.content != "Buy groceries"));
    }

    #[test]
    fn test_move_to_taskpad_success() {
        let mut app = setup_test_app();
        let task = &mut app.tasks[0];
        task.container = TaskContainer::Backburner;
        let content = task.content.clone();

        execute_move_to_taskpad_command(&mut app, &content);

        assert_eq!(app.tasks[0].container, TaskContainer::Taskpad);
        assert_eq!(
            app.activity_log.latest_message().unwrap(),
            &format!("Moved task to taskpad: {content}")
        );
    }

    #[test]
    fn test_move_to_taskpad_already_there() {
        let mut app = setup_test_app();
        let task = &app.tasks[0];
        let content = task.content.clone();

        execute_move_to_taskpad_command(&mut app, &content);

        assert_eq!(app.tasks[0].container, TaskContainer::Taskpad);
        assert_eq!(
            app.activity_log.latest_message().unwrap(),
            "Task already in taskpad"
        );
    }

    #[test]
    fn test_move_to_backburner_success() {
        let mut app = setup_test_app();
        let task = &app.tasks[0];
        let content = task.content.clone();

        execute_move_to_backburner_command(&mut app, &content);

        assert_eq!(app.tasks[0].container, TaskContainer::Backburner);
        assert_eq!(
            app.activity_log.latest_message().unwrap(),
            &format!("Moved task to backburner: {content}")
        );
    }

    #[test]
    fn test_move_to_backburner_already_there() {
        let mut app = setup_test_app();
        let task = &mut app.tasks[0];
        task.container = TaskContainer::Backburner;
        let content = task.content.clone();

        execute_move_to_backburner_command(&mut app, &content);

        assert_eq!(app.tasks[0].container, TaskContainer::Backburner);
        assert_eq!(
            app.activity_log.latest_message().unwrap(),
            "Task already in backburner"
        );
    }

    #[test]
    fn test_move_to_shelved_success() {
        let mut app = setup_test_app();
        let task = &app.tasks[0];
        let content = task.content.clone();

        execute_move_to_shelved_command(&mut app, &content);

        assert_eq!(app.tasks[0].container, TaskContainer::Shelved);
        assert_eq!(
            app.activity_log.latest_message().unwrap(),
            &format!("Moved task to shelved: {content}")
        );
    }

    #[test]
    fn test_move_to_shelved_already_there() {
        let mut app = setup_test_app();
        let task = &mut app.tasks[0];
        task.container = TaskContainer::Shelved;
        let content = task.content.clone();

        execute_move_to_shelved_command(&mut app, &content);

        assert_eq!(app.tasks[0].container, TaskContainer::Shelved);
        assert_eq!(
            app.activity_log.latest_message().unwrap(),
            "Task already in shelved"
        );
    }

    #[test]
    fn test_move_nonexistent_task() {
        let mut app = setup_test_app();

        execute_move_to_taskpad_command(&mut app, "Nonexistent task");
        assert_eq!(
            app.activity_log.latest_message().unwrap(),
            format!("No matching task found")
        );

        execute_move_to_backburner_command(&mut app, "Nonexistent task");
        assert_eq!(
            app.activity_log.latest_message().unwrap(),
            format!("No matching task found")
        );

        execute_move_to_shelved_command(&mut app, "Nonexistent task");
        assert_eq!(
            app.activity_log.latest_message().unwrap(),
            format!("No matching task found")
        );
    }

    #[test]
    fn test_focus_task_by_content() {
        let mut app = setup_test_app();
        app.display_container_state.update_display_order(&app.tasks);
        let result = focus_task(&mut app, "Buy groceries");
        assert!(matches!(result, FocusResult::Focused { content } if content == "Buy groceries"));
    }

    #[test]
    fn test_focus_task_by_index_updates_state() {
        let mut app = setup_test_app();
        app.display_container_state.update_display_order(&app.tasks);

        let result = focus_task(&mut app, "1");
        assert!(matches!(result, FocusResult::Focused { .. }));

        // Check that state is updated
        assert_eq!(app.display_container_state.focused_index, Some(1));
        assert_eq!(app.display_container_state.input_value(), "Buy groceries");
    }

    #[test]
    fn test_focus_task_updates_input() {
        let mut app = setup_test_app();
        app.display_container_state.update_display_order(&app.tasks);

        // Set initial input
        app.display_container_state.set_input("previous input");

        // Focus on first task
        let result = focus_task(&mut app, "1");
        assert!(matches!(result, FocusResult::Focused { .. }));

        // Check that input is updated
        assert_eq!(app.display_container_state.input_value(), "Buy groceries");
    }

    #[test]
    fn test_focus_nonexistent_task() {
        let mut app = setup_test_app();
        let result = focus_task(&mut app, "nonexistent task");
        assert!(matches!(result, FocusResult::NoMatchingTask));
    }

    #[test]
    fn test_show_command() {
        let mut app = setup_test_app();
        assert_eq!(
            app.display_container_state.active_container,
            TaskContainer::Taskpad
        );

        execute_show_command(&mut app, TaskContainer::Backburner);
        assert_eq!(
            app.display_container_state.active_container,
            TaskContainer::Backburner
        );
        assert_eq!(
            app.activity_log.latest_message(),
            Some("Showing backburner tasks")
        );
    }

    #[test]
    fn test_add_subtask_success() {
        let mut app = setup_test_app();
        let query = "Buy groceries";
        let content = "Subtask content";

        execute_add_subtask(&mut app, query, content);

        // Verify subtask was created with correct parent reference
        let subtask = app.tasks.last().unwrap();
        assert!(
            app.tasks
                .iter()
                .any(|t| t.content == query && t.child_ids.contains(&subtask.id))
        );
        assert_eq!(subtask.content, content);

        // Verify parent's child_ids was updated
        let parent = app.tasks.iter().find(|t| t.content == query).unwrap();
        assert!(parent.child_ids.contains(&subtask.id));
    }

    #[test]
    fn test_add_subtask_nonexistent_parent() {
        let mut app = setup_test_app();
        let invalid_parent_query = "nonexistent task";
        let content = "Subtask content";
        let initial_task_count = app.tasks.len();

        execute_add_subtask(&mut app, invalid_parent_query, content);

        // Verify no task was created
        assert_eq!(app.tasks.len(), initial_task_count);
        assert_eq!(
            app.activity_log.latest_message(),
            Some(format!("No task found matching '{}'", invalid_parent_query).as_str())
        );
    }

    #[test]
    fn test_delete_subtask_updates_parent() {
        let mut app = setup_test_app();

        // Create a parent task
        execute_create_command(&mut app, "Parent task");
        let parent_id = app.tasks.last().unwrap().id;

        // Create a child task
        execute_add_subtask(&mut app, "Parent task", "Child task");

        // Verify initial state
        let parent_index = app.tasks.iter().position(|t| t.id == parent_id).unwrap();
        assert!(
            app.tasks[parent_index]
                .child_ids
                .contains(&app.tasks.last().unwrap().id),
            "Child task should be in parent's child_ids"
        );

        // Delete the child task
        execute_delete_command(&mut app, "Child task");

        // Verify the child is removed from parent's child_ids
        let parent_index = app.tasks.iter().position(|t| t.id == parent_id).unwrap();
        assert!(
            !app.tasks[parent_index]
                .child_ids
                .contains(&app.tasks.last().unwrap().id),
            "Child task should be removed from parent's child_ids"
        );

        // Verify the child task is actually deleted
        assert!(
            !app.tasks.iter().any(|t| t.content == "Child task"),
            "Child task should be deleted"
        );
    }

    #[test]
    fn test_toggle_task_expansion() {
        let mut app = setup_test_app();
        let query = "Buy groceries";

        // Initially, the task should not be in the folded_tasks set
        let task_index = find_task(&app, query).unwrap();
        let task_id = app.tasks[task_index].id;
        assert!(!app.display_container_state.folded_tasks.contains(&task_id));

        // Toggle the task expansion
        execute_toggle_command(&mut app, query);

        // Now the task should be in the folded_tasks set
        assert!(app.display_container_state.folded_tasks.contains(&task_id));

        // Toggle again
        execute_toggle_command(&mut app, query);

        // Now the task should not be in the folded_tasks set
        assert!(!app.display_container_state.folded_tasks.contains(&task_id));

        // Verify activity log message
        assert_eq!(
            app.activity_log.latest_message(),
            Some(format!("Toggled task: {}", app.tasks[task_index].content).as_str())
        );
    }

    #[test]
    fn test_toggle_nonexistent_task() {
        let mut app = setup_test_app();
        let invalid_query = "nonexistent task";

        execute_toggle_command(&mut app, invalid_query);

        // Verify activity log message
        assert_eq!(
            app.activity_log.latest_message(),
            Some("No matching task found")
        );
    }
}
