use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tracing::{info, warn};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use reqwest;

use super::{
    ContextSource, ContextData, ContextDataType, ContextContent, TaskContext, Task, TaskStatus
};

/// Tasks context source (placeholder implementation)
pub struct TasksContextSource {
    source_type: TaskSourceType,
    config: TasksConfig,
    enabled: bool,
}

/// Types of task sources
#[derive(Debug, Clone)]
pub enum TaskSourceType {
    Todoist,
    LocalFile,
    Obsidian, // This would be handled by the Obsidian source
}

/// Configuration for tasks
#[derive(Debug, Clone)]
pub struct TasksConfig {
    pub api_key: Option<String>,
    pub file_path: Option<String>,
    pub sync_completed: bool,
    pub max_tasks: usize,
}

/// Todoist API response structures
#[derive(Debug, Deserialize)]
struct TodoistTask {
    id: String,
    content: String,
    description: String,
    due: Option<TodoistDue>,
    priority: i32,
    #[serde(rename = "is_completed")]
    completed: bool,
    labels: Vec<String>,
    project_id: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct TodoistDue {
    date: String,
    #[serde(rename = "is_recurring")]
    recurring: bool,
    datetime: Option<String>,
    string: String,
    timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TodoistProject {
    id: String,
    name: String,
    color: String,
    #[serde(rename = "is_shared")]
    shared: bool,
    #[serde(rename = "is_favorite")]
    favorite: bool,
    #[serde(rename = "is_inbox_project")]
    inbox: bool,
    #[serde(rename = "is_team_inbox")]
    team_inbox: bool,
    url: String,
}

/// Local task file format (markdown-based)
#[derive(Debug, Serialize, Deserialize)]
struct LocalTaskFile {
    tasks: Vec<LocalTask>,
    last_updated: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LocalTask {
    id: String,
    title: String,
    description: Option<String>,
    due_date: Option<DateTime<Utc>>,
    priority: i32,
    status: String,
    tags: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TasksContextSource {
    /// Create a new tasks context source
    pub fn new(source_type: TaskSourceType, config: TasksConfig) -> Self {
        let enabled = match source_type {
            TaskSourceType::Todoist => config.api_key.is_some(),
            TaskSourceType::LocalFile => config.file_path.is_some(),
            TaskSourceType::Obsidian => false, // Handled by Obsidian source
        };
        
        Self {
            source_type,
            config,
            enabled,
        }
    }
    
    /// Extract tags from title and return cleaned title and tags
    fn extract_tags_and_clean_title(title: &str) -> (String, Vec<String>) {
        let mut tags = Vec::new();
        let mut clean_title = title.to_string();
        
        // Simple regex-like parsing for tags
        let words: Vec<&str> = title.split_whitespace().collect();
        for word in words {
            if word.starts_with('#') && word.len() > 1 {
                tags.push(word[1..].to_string());
                clean_title = clean_title.replace(word, "").trim().to_string();
            }
        }
        
        (clean_title, tags)
    }
    
    /// Fetch tasks from the configured source
    async fn fetch_tasks(&self, _start: DateTime<Utc>, _end: DateTime<Utc>) -> Result<Vec<Task>> {
        match self.source_type {
            TaskSourceType::Todoist => self.fetch_todoist_tasks().await,
            TaskSourceType::LocalFile => self.fetch_local_tasks().await,
            TaskSourceType::Obsidian => self.fetch_obsidian_tasks().await,
        }
    }
    
    /// Fetch tasks from Todoist API
    async fn fetch_todoist_tasks(&self) -> Result<Vec<Task>> {
        let api_key = self.config.api_key.as_ref()
            .ok_or_else(|| anyhow!("Todoist API key not configured"))?;
        
        info!("Fetching tasks from Todoist API");
        
        let client = reqwest::Client::new();
        
        // First, get projects for context
        let projects_response = client
            .get("https://api.todoist.com/rest/v2/projects")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await?;
        
        if !projects_response.status().is_success() {
            return Err(anyhow!("Failed to fetch Todoist projects: {}", projects_response.status()));
        }
        
        let projects: Vec<TodoistProject> = projects_response.json().await?;
        let project_map: HashMap<String, String> = projects.into_iter()
            .map(|p| (p.id, p.name))
            .collect();
        
        // Then get tasks
        let tasks_response = client
            .get("https://api.todoist.com/rest/v2/tasks")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await?;
        
        if !tasks_response.status().is_success() {
            return Err(anyhow!("Failed to fetch Todoist tasks: {}", tasks_response.status()));
        }
        
        let todoist_tasks: Vec<TodoistTask> = tasks_response.json().await?;
        
        let mut tasks = Vec::new();
        for todoist_task in todoist_tasks {
            // Skip completed tasks if not configured to sync them
            if todoist_task.completed && !self.config.sync_completed {
                continue;
            }
            
            let due_date = if let Some(due) = &todoist_task.due {
                self.parse_todoist_due_date(due)?
            } else {
                None
            };
            
            let status = if todoist_task.completed {
                TaskStatus::Completed
            } else {
                TaskStatus::Pending
            };
            
            let mut tags = todoist_task.labels.clone();
            if let Some(project_name) = project_map.get(&todoist_task.project_id) {
                tags.push(format!("project:{}", project_name));
            }
            
            tasks.push(Task {
                id: todoist_task.id,
                title: todoist_task.content,
                description: if todoist_task.description.is_empty() {
                    None
                } else {
                    Some(todoist_task.description)
                },
                due_date,
                priority: self.convert_todoist_priority(todoist_task.priority),
                status,
                tags,
                source: "todoist".to_string(),
            });
            
            if tasks.len() >= self.config.max_tasks {
                break;
            }
        }
        
        info!("Fetched {} tasks from Todoist", tasks.len());
        Ok(tasks)
    }
    
    /// Fetch tasks from local file
    async fn fetch_local_tasks(&self) -> Result<Vec<Task>> {
        let file_path = self.config.file_path.as_ref()
            .ok_or_else(|| anyhow!("Local task file path not configured"))?;
        
        info!("Fetching tasks from local file: {}", file_path);
        
        if !Path::new(file_path).exists() {
            warn!("Local task file does not exist: {}", file_path);
            return Ok(vec![]);
        }
        
        let content = fs::read_to_string(file_path).await?;
        
        // Try to parse as JSON first
        if let Ok(task_file) = serde_json::from_str::<LocalTaskFile>(&content) {
            return self.convert_local_tasks(task_file.tasks);
        }
        
        // Fall back to markdown parsing
        self.parse_markdown_tasks(&content).await
    }
    
    /// Parse markdown-based task file
    async fn parse_markdown_tasks(&self, content: &str) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        let mut task_id = 1;
        
        for line in content.lines() {
            let line = line.trim();
            
            // Look for task patterns like:
            // - [ ] Task title
            // - [x] Completed task
            // - [!] High priority task
            if line.starts_with("- [") && line.len() > 4 {
                let status_char = line.chars().nth(3).unwrap_or(' ');
                let title = line[5..].trim();
                
                if title.is_empty() {
                    continue;
                }
                
                let status = match status_char {
                    'x' | 'X' => TaskStatus::Completed,
                    '!' => TaskStatus::InProgress,
                    ' ' => TaskStatus::Pending,
                    _ => TaskStatus::Pending,
                };
                
                let priority = if status_char == '!' { 8 } else { 5 };
                
                // Extract tags from title and clean it
                let (clean_title, tags) = Self::extract_tags_and_clean_title(title);
                
                tasks.push(Task {
                    id: format!("local_{}", task_id),
                    title: clean_title,
                    description: None,
                    due_date: None,
                    priority,
                    status,
                    tags,
                    source: "local".to_string(),
                });
                
                task_id += 1;
                
                if tasks.len() >= self.config.max_tasks {
                    break;
                }
            }
        }
        
        info!("Parsed {} tasks from markdown file", tasks.len());
        Ok(tasks)
    }
    
    /// Convert local task format to our Task format
    fn convert_local_tasks(&self, local_tasks: Vec<LocalTask>) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        
        for local_task in local_tasks {
            let status = match local_task.status.as_str() {
                "completed" => TaskStatus::Completed,
                "in_progress" => TaskStatus::InProgress,
                "blocked" => TaskStatus::Blocked,
                "cancelled" => TaskStatus::Cancelled,
                _ => TaskStatus::Pending,
            };
            
            tasks.push(Task {
                id: local_task.id,
                title: local_task.title,
                description: local_task.description,
                due_date: local_task.due_date,
                priority: local_task.priority,
                status,
                tags: local_task.tags,
                source: "local".to_string(),
            });
            
            if tasks.len() >= self.config.max_tasks {
                break;
            }
        }
        
        Ok(tasks)
    }
    
    /// Parse Todoist due date format
    fn parse_todoist_due_date(&self, due: &TodoistDue) -> Result<Option<DateTime<Utc>>> {
        if let Some(datetime) = &due.datetime {
            // Parse full datetime
            match DateTime::parse_from_rfc3339(datetime) {
                Ok(dt) => Ok(Some(dt.with_timezone(&Utc))),
                Err(_) => {
                    // Try parsing as date only
                    match chrono::NaiveDate::parse_from_str(&due.date, "%Y-%m-%d") {
                        Ok(date) => Ok(Some(date.and_hms_opt(23, 59, 59).unwrap().and_utc())),
                        Err(_) => Ok(None),
                    }
                }
            }
        } else {
            // Parse date only
            match chrono::NaiveDate::parse_from_str(&due.date, "%Y-%m-%d") {
                Ok(date) => Ok(Some(date.and_hms_opt(23, 59, 59).unwrap().and_utc())),
                Err(_) => Ok(None),
            }
        }
    }
    
    /// Convert Todoist priority (1-4) to our priority system (1-10)
    fn convert_todoist_priority(&self, priority: i32) -> i32 {
        match priority {
            4 => 10, // Urgent
            3 => 8,  // High
            2 => 5,  // Medium
            1 => 3,  // Low
            _ => 5,  // Default
        }
    }
    
    /// Fetch tasks from Obsidian vault
    async fn fetch_obsidian_tasks(&self) -> Result<Vec<Task>> {
        let vault_path = self.config.file_path.as_ref()
            .ok_or_else(|| anyhow!("Obsidian vault path not configured"))?;
        
        info!("Fetching tasks from Obsidian vault: {}", vault_path);
        
        let mut tasks = Vec::new();
        self.scan_obsidian_vault_for_tasks(vault_path, &mut tasks).await?;
        
        info!("Found {} tasks in Obsidian vault", tasks.len());
        Ok(tasks)
    }
    
    /// Recursively scan Obsidian vault for tasks
    async fn scan_obsidian_vault_for_tasks(&self, vault_path: &str, tasks: &mut Vec<Task>) -> Result<()> {
        use std::path::Path;
        use std::collections::VecDeque;
        
        let vault_dir = Path::new(vault_path);
        if !vault_dir.exists() {
            return Err(anyhow!("Obsidian vault directory does not exist: {}", vault_path));
        }
        
        // Use iterative approach instead of recursion to avoid Send issues
        let mut dirs_to_process = VecDeque::new();
        dirs_to_process.push_back(vault_dir.to_path_buf());
        
        while let Some(dir) = dirs_to_process.pop_front() {
            let mut entries = fs::read_dir(&dir).await?;
            
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                
                if path.is_dir() {
                    // Skip hidden directories and .obsidian
                    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                        if dir_name.starts_with('.') || dir_name == ".obsidian" || dir_name == ".trash" {
                            continue;
                        }
                    }
                    
                    // Add subdirectory to processing queue
                    dirs_to_process.push_back(path);
                } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    // Process markdown files
                    if let Err(e) = self.extract_tasks_from_markdown_file(&path, tasks).await {
                        warn!("Failed to extract tasks from file {:?}: {}", path, e);
                    }
                }
                
                // Limit total tasks
                if tasks.len() >= self.config.max_tasks {
                    break;
                }
            }
            
            // Limit total tasks
            if tasks.len() >= self.config.max_tasks {
                break;
            }
        }
        
        Ok(())
    }
    
    /// Extract tasks from a single markdown file
    async fn extract_tasks_from_markdown_file(&self, file_path: &Path, tasks: &mut Vec<Task>) -> Result<()> {
        let content = fs::read_to_string(file_path).await?;
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        
        let mut task_id = 1;
        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            
            // Look for task patterns:
            // - [ ] Task title
            // - [x] Completed task
            // - [!] High priority task
            // - [>] Scheduled task
            // - [?] Question/unclear task
            if line.starts_with("- [") && line.len() > 4 {
                let status_char = line.chars().nth(3).unwrap_or(' ');
                let title = line[5..].trim();
                
                if title.is_empty() {
                    continue;
                }
                
                let status = match status_char {
                    'x' | 'X' => TaskStatus::Completed,
                    '!' => TaskStatus::InProgress,
                    '>' => TaskStatus::Pending, // Scheduled
                    '?' => TaskStatus::Blocked,  // Question/unclear
                    ' ' => TaskStatus::Pending,
                    _ => TaskStatus::Pending,
                };
                
                let priority = match status_char {
                    '!' => 8, // High priority
                    '>' => 6, // Scheduled
                    '?' => 4, // Question/unclear
                    _ => 5,   // Default
                };
                
                // Extract due dates from common patterns
                let due_date = self.extract_due_date_from_title(title);
                
                // Extract tags from title and clean it
                let (clean_title, mut tags) = Self::extract_tags_and_clean_title(title);
                
                // Add file context as tag
                tags.push(format!("file:{}", file_name.replace(".md", "")));
                
                tasks.push(Task {
                    id: format!("obsidian_{}_{}", file_name, task_id),
                    title: clean_title,
                    description: Some(format!("From {}, line {}", file_name, line_num + 1)),
                    due_date,
                    priority,
                    status,
                    tags,
                    source: "obsidian".to_string(),
                });
                
                task_id += 1;
                
                if tasks.len() >= self.config.max_tasks {
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    /// Extract due date from task title using common patterns
    fn extract_due_date_from_title(&self, title: &str) -> Option<DateTime<Utc>> {
        // Look for patterns like:
        // - "due 2024-07-17"
        // - "by July 17"
        // - "deadline: 2024-07-17"
        // - "📅 2024-07-17"
        
        // Simple date pattern matching (YYYY-MM-DD)
        if let Some(date_match) = title.find("2024-") {
            let date_str = &title[date_match..date_match + 10];
            if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                return Some(date.and_hms_opt(23, 59, 59).unwrap().and_utc());
            }
        }
        
        // Look for "due:" or "deadline:" followed by date
        for prefix in &["due:", "deadline:", "by ", "📅 "] {
            if let Some(pos) = title.to_lowercase().find(prefix) {
                let after_prefix = &title[pos + prefix.len()..];
                if let Some(date_str) = after_prefix.split_whitespace().next() {
                    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                        return Some(date.and_hms_opt(23, 59, 59).unwrap().and_utc());
                    }
                }
            }
        }
        
        None
    }
}

#[async_trait]
impl ContextSource for TasksContextSource {
    fn source_id(&self) -> &str {
        match self.source_type {
            TaskSourceType::Todoist => "tasks_todoist",
            TaskSourceType::LocalFile => "tasks_local",
            TaskSourceType::Obsidian => "tasks_obsidian",
        }
    }
    
    fn display_name(&self) -> &str {
        match self.source_type {
            TaskSourceType::Todoist => "Todoist Tasks",
            TaskSourceType::LocalFile => "Local Task File",
            TaskSourceType::Obsidian => "Obsidian Tasks",
        }
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    async fn fetch_context(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<ContextData> {
        info!("Fetching tasks context from {:?}", self.source_type);
        
        let tasks = self.fetch_tasks(start, end).await?;
        
        let overdue_count = tasks.iter()
            .filter(|t| {
                if let Some(due_date) = t.due_date {
                    due_date < Utc::now() && !matches!(t.status, TaskStatus::Completed)
                } else {
                    false
                }
            })
            .count();
        
        let upcoming_count = tasks.iter()
            .filter(|t| {
                if let Some(due_date) = t.due_date {
                    due_date > Utc::now() && due_date <= Utc::now() + chrono::Duration::days(7)
                } else {
                    false
                }
            })
            .count();
        
        let task_context = TaskContext {
            tasks,
            overdue_count,
            upcoming_count,
        };
        
        Ok(ContextData {
            source_id: self.source_id().to_string(),
            timestamp: Utc::now(),
            data_type: ContextDataType::Tasks,
            priority: 120, // Medium priority
            content: ContextContent::Tasks(task_context),
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("source_type".to_string(), format!("{:?}", self.source_type));
                metadata
            },
        })
    }
    
    fn priority(&self) -> i32 {
        120 // Medium priority
    }
    
    fn required_config(&self) -> Vec<String> {
        match self.source_type {
            TaskSourceType::Todoist => vec!["api_key".to_string()],
            TaskSourceType::LocalFile => vec!["file_path".to_string()],
            TaskSourceType::Obsidian => vec![],
        }
    }
}