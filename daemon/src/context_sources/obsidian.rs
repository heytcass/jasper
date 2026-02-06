use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::fs as async_fs;
use tracing::{debug, info, warn};

/// Pre-compiled regexes used across obsidian parsing (compiled once, reused)
struct ObsidianRegexes {
    frontmatter: Regex,
    task: Regex,
    item: Regex,
    focus_patterns: Vec<Regex>,
}

fn obsidian_regexes() -> &'static ObsidianRegexes {
    static REGEXES: OnceLock<ObsidianRegexes> = OnceLock::new();
    REGEXES.get_or_init(|| ObsidianRegexes {
        frontmatter: Regex::new(r"^---\n(.*?)\n---\n(.*)$").unwrap(),
        task: Regex::new(r"^\s*- \[([ x])\] (.+)$").unwrap(),
        item: Regex::new(r"^\s*[-*]\s+(.+)$").unwrap(),
        focus_patterns: vec![
            Regex::new(r"## Focus(?:\s+Areas?)?\s*\n(.*?)(?:\n##|$)").unwrap(),
            Regex::new(r"## Today's Focus\s*\n(.*?)(?:\n##|$)").unwrap(),
            Regex::new(r"## Priorities\s*\n(.*?)(?:\n##|$)").unwrap(),
        ],
    })
}

use super::{
    ContextSource, ContextData, ContextDataType, ContextContent, NotesContext,
    Task, TaskStatus, Project, ProjectStatus, DailyNote
};

/// Obsidian vault context source
pub struct ObsidianVaultSource {
    vault_path: PathBuf,
    config: ObsidianConfig,
    enabled: bool,
}

/// Configuration for Obsidian integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsidianConfig {
    pub vault_path: String,
    pub daily_notes_folder: String,
    pub daily_notes_format: String,
    pub templates_folder: String,
    pub people_folder: String,
    pub projects_folder: String,
    pub parse_dataview: bool,
    pub parse_tasks: bool,
    pub parse_frontmatter: bool,
    pub relationship_alert_days: i64,
    pub ignored_folders: Vec<String>,
    pub ignored_files: Vec<String>,
}

impl Default for ObsidianConfig {
    fn default() -> Self {
        Self {
            vault_path: "~/Documents/Obsidian Vault".to_string(),
            daily_notes_folder: "Work/Daily".to_string(),
            daily_notes_format: "YYYY-MM-DD".to_string(),
            templates_folder: "Templates".to_string(),
            people_folder: "Work/People".to_string(),
            projects_folder: "Work/Projects".to_string(),
            parse_dataview: true,
            parse_tasks: true,
            parse_frontmatter: true,
            relationship_alert_days: 21,
            ignored_folders: vec![".obsidian".to_string(), ".trash".to_string()],
            ignored_files: vec![".DS_Store".to_string()],
        }
    }
}

/// Frontmatter data from Obsidian notes (project-relevant fields only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontMatter {
    pub name: Option<String>,
    pub status: Option<String>,
    pub due_date: Option<NaiveDate>,
    pub priority: Option<i32>,
    pub client: Option<String>,
    pub progress: Option<f32>,
    pub other: HashMap<String, serde_yaml::Value>,
}

impl ObsidianVaultSource {
    /// Create a new Obsidian vault source
    pub fn new(config: ObsidianConfig) -> Result<Self> {
        let vault_path = PathBuf::from(&config.vault_path);
        
        if !vault_path.exists() {
            return Err(anyhow!("Obsidian vault path does not exist: {}", config.vault_path));
        }
        
        // Check if it's a valid Obsidian vault
        let obsidian_config_path = vault_path.join(".obsidian");
        if !obsidian_config_path.exists() {
            return Err(anyhow!("Not a valid Obsidian vault: missing .obsidian folder"));
        }
        
        Ok(Self {
            vault_path,
            config,
            enabled: true,
        })
    }
    
    /// Parse frontmatter from a markdown file
    fn parse_frontmatter(content: &str) -> Result<(Option<FrontMatter>, String)> {
        let re = &obsidian_regexes().frontmatter;

        if let Some(captures) = re.captures(content) {
            let yaml_content = captures.get(1)
                .ok_or_else(|| anyhow!("Failed to extract YAML content from frontmatter"))?
                .as_str();
            let markdown_content = captures.get(2)
                .ok_or_else(|| anyhow!("Failed to extract markdown content after frontmatter"))?
                .as_str();
            
            match serde_yaml::from_str::<HashMap<String, serde_yaml::Value>>(yaml_content) {
                Ok(yaml_map) => {
                    let frontmatter = FrontMatter {
                        name: yaml_map.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        status: yaml_map.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        due_date: yaml_map.get("due_date")
                            .and_then(|v| v.as_str())
                            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
                        priority: yaml_map.get("priority").and_then(|v| v.as_i64()).map(|i| i as i32),
                        client: yaml_map.get("client").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        progress: yaml_map.get("progress").and_then(|v| v.as_f64()).map(|f| f as f32),
                        other: yaml_map,
                    };
                    
                    Ok((Some(frontmatter), markdown_content.to_string()))
                }
                Err(e) => {
                    warn!("Failed to parse frontmatter: {}", e);
                    Ok((None, content.to_string()))
                }
            }
        } else {
            Ok((None, content.to_string()))
        }
    }
    
    /// Extract tasks from markdown content
    fn extract_tasks(content: &str, file_path: &Path) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        let task_regex = &obsidian_regexes().task;

        for (line_num, line) in content.lines().enumerate() {
            if let Some(captures) = task_regex.captures(line) {
                let is_completed = captures.get(1)
                    .ok_or_else(|| anyhow!("Failed to extract task status from: {}", line))?
                    .as_str() == "x";
                let task_text = captures.get(2)
                    .ok_or_else(|| anyhow!("Failed to extract task text from: {}", line))?
                    .as_str();
                
                let task_id = format!("{}:{}:{}", 
                    file_path.file_name().unwrap_or_default().to_string_lossy(), 
                    line_num, 
                    task_text.chars().take(20).collect::<String>()
                );
                
                tasks.push(Task {
                    id: task_id,
                    title: task_text.to_string(),
                    description: None,
                    due_date: None, // Could be enhanced to parse dates from task text
                    priority: 5, // Default priority
                    status: if is_completed { TaskStatus::Completed } else { TaskStatus::Pending },
                    tags: vec![],
                    source: "obsidian".to_string(),
                });
            }
        }
        
        Ok(tasks)
    }
    
    /// Get daily notes for a date range
    async fn get_daily_notes(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<DailyNote>> {
        let mut daily_notes = Vec::new();
        let daily_notes_path = self.vault_path.join(&self.config.daily_notes_folder);
        
        if !daily_notes_path.exists() {
            debug!("Daily notes folder does not exist: {:?}", daily_notes_path);
            return Ok(daily_notes);
        }
        
        let mut current_date = start.date_naive();
        let end_date = end.date_naive();
        
        while current_date <= end_date {
            let filename = format!("{}.md", current_date.format("%Y-%m-%d"));
            let file_path = daily_notes_path.join(&filename);
            
            if file_path.exists() {
                match async_fs::read_to_string(&file_path).await {
                    Ok(content) => {
                        let (frontmatter, markdown_content) = Self::parse_frontmatter(&content)?;
                        let tasks = Self::extract_tasks(&markdown_content, &file_path)?;
                        
                        // Extract mood and energy level from frontmatter or content
                        let mood = frontmatter.as_ref().and_then(|fm| fm.other.get("mood"))
                            .and_then(|v| v.as_str()).map(|s| s.to_string());
                        let energy_level = frontmatter.as_ref().and_then(|fm| fm.other.get("energy"))
                            .and_then(|v| v.as_i64()).map(|i| i as i32);
                        
                        // Extract focus areas from content
                        let focus_areas = self.extract_focus_areas(&markdown_content)?;
                        
                        daily_notes.push(DailyNote {
                            date: current_date.and_hms_opt(9, 0, 0)
                                .ok_or_else(|| anyhow!("Failed to create datetime for date: {}", current_date))?
                                .and_utc(),
                            title: filename,
                            content: markdown_content,
                            tasks,
                            mood,
                            energy_level,
                            focus_areas,
                        });
                    }
                    Err(e) => {
                        warn!("Failed to read daily note {}: {}", filename, e);
                    }
                }
            }
            
            current_date = current_date.succ_opt()
                .ok_or_else(|| anyhow!("Date overflow: cannot get next day after {}", current_date))?;
        }
        
        Ok(daily_notes)
    }
    
    /// Extract focus areas from content
    fn extract_focus_areas(&self, content: &str) -> Result<Vec<String>> {
        let mut focus_areas = Vec::new();
        let regexes = obsidian_regexes();

        for regex in &regexes.focus_patterns {
            if let Some(captures) = regex.captures(content) {
                let focus_text = captures.get(1)
                    .ok_or_else(|| anyhow!("Failed to extract focus text from line"))?
                    .as_str();
                for line in focus_text.lines() {
                    if let Some(item_captures) = regexes.item.captures(line) {
                        let item = item_captures.get(1)
                            .ok_or_else(|| anyhow!("Failed to extract item text from line"))?
                            .as_str();
                        focus_areas.push(item.to_string());
                    }
                }
            }
        }

        Ok(focus_areas)
    }
    
    /// Get active projects
    async fn get_active_projects(&self) -> Result<Vec<Project>> {
        let mut projects = Vec::new();
        let projects_path = self.vault_path.join(&self.config.projects_folder);
        
        if !projects_path.exists() {
            debug!("Projects folder does not exist: {:?}", projects_path);
            return Ok(projects);
        }
        
        let mut entries = async_fs::read_dir(&projects_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
                match async_fs::read_to_string(&path).await {
                    Ok(content) => {
                        let (frontmatter, markdown_content) = Self::parse_frontmatter(&content)?;
                        
                        if let Some(fm) = frontmatter {
                            let project_name = fm.name.clone().unwrap_or_else(|| {
                                path.file_stem().unwrap_or_default().to_string_lossy().to_string()
                            });
                            
                            let status = match fm.status.as_deref() {
                                Some("Active") => ProjectStatus::Active,
                                Some("Pending") => ProjectStatus::Pending,
                                Some("Completed") => ProjectStatus::Completed,
                                Some("OnHold") => ProjectStatus::OnHold,
                                Some("Cancelled") => ProjectStatus::Cancelled,
                                _ => ProjectStatus::Active, // Default
                            };
                            
                            let tasks = Self::extract_tasks(&markdown_content, &path)?;
                            
                            projects.push(Project {
                                id: path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
                                name: project_name,
                                description: None, // Could extract from content
                                status,
                                due_date: fm.due_date.and_then(|d| d.and_hms_opt(23, 59, 59).map(|dt| dt.and_utc())),
                                client: fm.client,
                                priority: fm.priority.unwrap_or(5),
                                progress: fm.progress.unwrap_or(0.0),
                                tasks,
                            });
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read project file {:?}: {}", path, e);
                    }
                }
            }
        }
        
        Ok(projects)
    }
    
}

#[async_trait]
impl ContextSource for ObsidianVaultSource {
    fn source_id(&self) -> &str {
        "obsidian"
    }
    
    fn display_name(&self) -> &str {
        "Obsidian Vault"
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled && self.vault_path.exists()
    }
    
    async fn fetch_context(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<ContextData> {
        info!("Fetching context from Obsidian vault: {:?}", self.vault_path);

        let daily_notes = self.get_daily_notes(start, end).await?;
        let active_projects = self.get_active_projects().await?;

        // Extract all tasks from daily notes and projects
        let mut all_tasks = Vec::new();
        for note in &daily_notes {
            all_tasks.extend(note.tasks.clone());
        }
        for project in &active_projects {
            all_tasks.extend(project.tasks.clone());
        }

        let notes_context = NotesContext {
            daily_notes,
            active_projects,
            pending_tasks: all_tasks.into_iter().filter(|t| matches!(t.status, TaskStatus::Pending)).collect(),
        };
        
        Ok(ContextData {
            source_id: self.source_id().to_string(),
            timestamp: Utc::now(),
            data_type: ContextDataType::Notes,
            priority: 200, // High priority for personal knowledge
            content: ContextContent::Notes(notes_context),
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("vault_path".to_string(), self.vault_path.to_string_lossy().to_string());
                metadata.insert("source_type".to_string(), "obsidian".to_string());
                metadata
            },
        })
    }
    
    fn priority(&self) -> i32 {
        200 // High priority for personal knowledge
    }
    
    fn required_config(&self) -> Vec<String> {
        vec!["vault_path".to_string()]
    }
}