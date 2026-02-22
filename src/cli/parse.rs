//! CLI parse: clap types for Merkle. No behavior; definitions only.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Merkle CLI - Deterministic filesystem state management
#[derive(Parser)]
#[command(name = "merkle")]
#[command(about = "Deterministic filesystem state management using Merkle trees")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Workspace root directory
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    /// Configuration file path (overrides default config loading)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Enable verbose logging (default: off)
    #[arg(long, default_value = "false")]
    pub verbose: bool,

    /// Log level (trace, debug, info, warn, error, off)
    #[arg(long)]
    pub log_level: Option<String>,

    /// Log format (json, text)
    #[arg(long)]
    pub log_format: Option<String>,

    /// Log output (stdout, stderr, file, both)
    #[arg(long)]
    pub log_output: Option<String>,

    /// Log file path (if output includes "file")
    #[arg(long)]
    pub log_file: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scan filesystem and rebuild tree
    Scan {
        /// Force rebuild even if tree exists
        #[arg(long)]
        force: bool,
    },
    /// Workspace commands (status, validate)
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
    /// Show unified status (workspace, agents, providers)
    Status {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Show only workspace section
        #[arg(long)]
        workspace_only: bool,
        /// Show only agents section
        #[arg(long)]
        agents_only: bool,
        /// Show only providers section
        #[arg(long)]
        providers_only: bool,
        /// Include top-level path breakdown in workspace section
        #[arg(long)]
        breakdown: bool,
        /// Test provider connectivity
        #[arg(long)]
        test_connectivity: bool,
    },
    /// Validate workspace integrity
    Validate,
    /// Start watch mode daemon
    Watch {
        /// Debounce window in milliseconds
        #[arg(long, default_value = "100")]
        debounce_ms: u64,
        /// Batch window in milliseconds
        #[arg(long, default_value = "50")]
        batch_window_ms: u64,
        /// Run in foreground (default: background daemon)
        #[arg(long)]
        foreground: bool,
    },
    /// Manage agents
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    /// Manage providers
    Provider {
        #[command(subcommand)]
        command: ProviderCommands,
    },
    /// Initialize default agents and prompts
    Init {
        /// Force re-initialization (overwrite existing)
        #[arg(long)]
        force: bool,

        /// List what would be initialized without creating
        #[arg(long)]
        list: bool,
    },
    /// Context operations (generate and retrieve frames)
    Context {
        #[command(subcommand)]
        command: ContextCommands,
    },
}

#[derive(Subcommand)]
pub enum WorkspaceCommands {
    /// Show workspace status (tree, context coverage, top paths)
    Status {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Include top-level path breakdown
        #[arg(long)]
        breakdown: bool,
    },
    /// Validate workspace integrity
    Validate {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// List or add paths to the workspace ignore list
    Ignore {
        /// Path to add (omit to list current ignore list)
        path: Option<PathBuf>,
        /// When adding, report what would be added without writing
        #[arg(long)]
        dry_run: bool,
        /// Output format for list mode (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Tombstone a node and its descendants (logical delete; reversible with restore)
    Delete {
        /// Path to file or directory to delete
        path: Option<PathBuf>,
        /// Node ID (hex) instead of path
        #[arg(long)]
        node: Option<String>,
        /// Report counts without performing the operation
        #[arg(long)]
        dry_run: bool,
        /// Do not add the path to the workspace ignore list
        #[arg(long)]
        no_ignore: bool,
    },
    /// Restore a tombstoned node and its descendants
    Restore {
        /// Path to file or directory to restore
        path: Option<PathBuf>,
        /// Node ID (hex) instead of path
        #[arg(long)]
        node: Option<String>,
        /// Report counts without performing the operation
        #[arg(long)]
        dry_run: bool,
    },
    /// Purge tombstoned records older than TTL
    Compact {
        /// Tombstone age threshold in days (default: 90)
        #[arg(long)]
        ttl: Option<u64>,
        /// Purge all tombstoned records regardless of age
        #[arg(long)]
        all: bool,
        /// Do not purge frame blobs; only purge node and head index records
        #[arg(long)]
        keep_frames: bool,
        /// Report counts without compaction
        #[arg(long)]
        dry_run: bool,
    },
    /// List tombstoned (deleted) nodes
    ListDeleted {
        /// Show only nodes tombstoned longer than this many days
        #[arg(long)]
        older_than: Option<u64>,
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },
}

#[derive(Subcommand)]
pub enum AgentCommands {
    /// Show agent status (validation and prompt path)
    Status {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// List all agents
    List {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Filter by role (Reader or Writer)
        #[arg(long)]
        role: Option<String>,
    },
    /// Show agent details
    Show {
        /// Agent ID
        agent_id: String,
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Include prompt file content in output
        #[arg(long)]
        include_prompt: bool,
    },
    /// Validate agent configuration
    Validate {
        /// Agent ID (required unless --all is used)
        #[arg(required_unless_present = "all")]
        agent_id: Option<String>,
        /// Validate all agents
        #[arg(long, conflicts_with = "agent_id")]
        all: bool,
        /// Show detailed validation results
        #[arg(long)]
        verbose: bool,
    },
    /// Create new agent
    Create {
        /// Agent ID
        agent_id: String,
        /// Agent role (Reader or Writer)
        #[arg(long)]
        role: Option<String>,
        /// Path to prompt file (required for Writer)
        #[arg(long)]
        prompt_path: Option<String>,
        /// Use interactive mode (default)
        #[arg(long)]
        interactive: bool,
        /// Use non-interactive mode (use flags)
        #[arg(long)]
        non_interactive: bool,
    },
    /// Edit agent configuration
    Edit {
        /// Agent ID
        agent_id: String,
        /// Update prompt file path
        #[arg(long)]
        prompt_path: Option<String>,
        /// Update agent role
        #[arg(long)]
        role: Option<String>,
        /// Editor to use (default: $EDITOR)
        #[arg(long)]
        editor: Option<String>,
    },
    /// Remove agent
    Remove {
        /// Agent ID
        agent_id: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ProviderCommands {
    /// Show provider status (optional connectivity)
    Status {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Test connectivity per provider (may be slow)
        #[arg(long)]
        test_connectivity: bool,
    },
    /// List all providers
    List {
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Filter by provider type (openai, anthropic, ollama, local)
        #[arg(long)]
        type_filter: Option<String>,
    },
    /// Show provider details
    Show {
        /// Provider name
        provider_name: String,
        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
        /// Show API key status
        #[arg(long)]
        include_credentials: bool,
    },
    /// Validate provider configuration
    Validate {
        /// Provider name
        provider_name: String,
        /// Test provider API connectivity
        #[arg(long)]
        test_connectivity: bool,
        /// Verify model is available
        #[arg(long)]
        check_model: bool,
        /// Show detailed validation results
        #[arg(long)]
        verbose: bool,
    },
    /// Test provider connectivity
    Test {
        /// Provider name
        provider_name: String,
        /// Test specific model (overrides config)
        #[arg(long)]
        model: Option<String>,
        /// Connection timeout in seconds (default: 10)
        #[arg(long, default_value = "10")]
        timeout: u64,
    },
    /// Create new provider
    Create {
        /// Provider name
        provider_name: String,
        /// Provider type (openai, anthropic, ollama, local)
        #[arg(long, name = "type")]
        type_: Option<String>,
        /// Model name
        #[arg(long)]
        model: Option<String>,
        /// Endpoint URL
        #[arg(long)]
        endpoint: Option<String>,
        /// API key
        #[arg(long)]
        api_key: Option<String>,
        /// Use interactive mode (default)
        #[arg(long)]
        interactive: bool,
        /// Use non-interactive mode (use flags)
        #[arg(long)]
        non_interactive: bool,
    },
    /// Edit provider configuration
    Edit {
        /// Provider name
        provider_name: String,
        /// Update model name
        #[arg(long)]
        model: Option<String>,
        /// Update endpoint URL
        #[arg(long)]
        endpoint: Option<String>,
        /// Update API key
        #[arg(long)]
        api_key: Option<String>,
        /// Editor to use (default: $EDITOR)
        #[arg(long)]
        editor: Option<String>,
    },
    /// Remove provider
    Remove {
        /// Provider name
        provider_name: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ContextCommands {
    /// Generate context frame for a node
    Generate {
        /// Target node by NodeID (hex string)
        #[arg(long, conflicts_with_all = ["path", "path_positional"])]
        node: Option<String>,

        /// Target node by workspace-relative or absolute path
        #[arg(long, value_name = "PATH", conflicts_with = "node")]
        path: Option<PathBuf>,

        /// Target path (positional; same as --path)
        #[arg(value_name = "PATH", index = 1, conflicts_with = "node")]
        path_positional: Option<PathBuf>,

        /// Agent to use for generation
        #[arg(long)]
        agent: Option<String>,

        /// Provider to use for generation (required)
        #[arg(long)]
        provider: Option<String>,

        /// Frame type (defaults to context-<agent_id>)
        #[arg(long)]
        frame_type: Option<String>,

        /// Generate even if head frame exists
        #[arg(long)]
        force: bool,
        /// Disable recursive generation for directory targets
        #[arg(long)]
        no_recursive: bool,
    },
    /// Retrieve context frames for a node
    Get {
        /// Target node by NodeID (hex string)
        #[arg(long, conflicts_with = "path")]
        node: Option<String>,

        /// Target node by workspace-relative or absolute path
        #[arg(long, conflicts_with = "node")]
        path: Option<PathBuf>,

        /// Filter by agent ID
        #[arg(long)]
        agent: Option<String>,

        /// Filter by frame type
        #[arg(long)]
        frame_type: Option<String>,

        /// Maximum frames to return
        #[arg(long, default_value = "10")]
        max_frames: usize,

        /// Ordering policy: recency or deterministic
        #[arg(long, default_value = "recency")]
        ordering: String,

        /// Concatenate frame contents with separator
        #[arg(long)]
        combine: bool,

        /// Separator used with --combine
        #[arg(long, default_value = "\n\n---\n\n")]
        separator: String,

        /// Output format: text or json
        #[arg(long, default_value = "text")]
        format: String,

        /// Include metadata fields in output
        #[arg(long)]
        include_metadata: bool,

        /// Include frames marked deleted (tombstones)
        #[arg(long)]
        include_deleted: bool,
    },
}
