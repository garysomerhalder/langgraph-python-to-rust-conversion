//! Example demonstrating advanced state management features

use langgraph::state::{GraphState, StateData};
use langgraph::state::advanced::{VersionedStateManager, SnapshotManager, StateDiff};
use serde_json::json;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 LangGraph Rust - State Management Example\n");
    
    // Basic state operations
    println!("📦 Basic State Operations:");
    let mut state = GraphState::new();
    
    // Set initial values
    state.set("counter", json!(0));
    state.set("user", json!({"name": "Alice", "role": "admin"}));
    state.set("status", json!("initialized"));
    
    println!("Initial state:");
    println!("  counter: {}", state.get("counter").unwrap());
    println!("  user: {}", state.get("user").unwrap());
    println!("  status: {}\n", state.get("status").unwrap());
    
    // Update state
    state.set("counter", json!(1));
    state.set("status", json!("processing"));
    
    // Add transition history
    state.add_transition(
        "init".to_string(),
        "process".to_string(),
        Some({
            let mut changes = StateData::new();
            changes.insert("counter".to_string(), json!(1));
            changes.insert("status".to_string(), json!("processing"));
            changes
        }),
    );
    
    println!("After update:");
    println!("  counter: {}", state.get("counter").unwrap());
    println!("  status: {}", state.get("status").unwrap());
    println!("  transitions: {}\n", state.history.len());
    
    // State versioning
    println!("📚 State Versioning:");
    let version_manager = VersionedStateManager::new(10);
    
    // Create version 1
    let mut v1_state = StateData::new();
    v1_state.insert("version".to_string(), json!("1.0"));
    v1_state.insert("feature".to_string(), json!("basic"));
    
    let v1 = version_manager.create_version(
        v1_state.clone(),
        Some("Initial version".to_string()),
    ).await?;
    
    println!("Created version {}: {}", v1, "Initial version");
    
    // Create version 2
    let mut v2_state = v1_state.clone();
    v2_state.insert("version".to_string(), json!("2.0"));
    v2_state.insert("feature".to_string(), json!("advanced"));
    v2_state.insert("new_feature".to_string(), json!("analytics"));
    
    let v2 = version_manager.create_version(
        v2_state.clone(),
        Some("Added analytics".to_string()),
    ).await?;
    
    println!("Created version {}: {}", v2, "Added analytics");
    
    // Create version 3
    let mut v3_state = v2_state.clone();
    v3_state.insert("version".to_string(), json!("3.0"));
    v3_state.insert("optimization".to_string(), json!(true));
    
    let v3 = version_manager.create_version(
        v3_state,
        Some("Performance optimization".to_string()),
    ).await?;
    
    println!("Created version {}: {}\n", v3, "Performance optimization");
    
    // Demonstrate rollback
    println!("⏪ Rollback to version 2:");
    version_manager.rollback(v2).await?;
    let current = version_manager.get_current_version().await?;
    println!("  Current version: {}", current.version);
    println!("  Feature: {}", current.state.get("feature").unwrap());
    println!("  Has optimization: {}\n", current.state.contains_key("optimization"));
    
    // State snapshots
    println!("📸 State Snapshots:");
    let snapshot_manager = SnapshotManager::new(5);
    
    // Create snapshots
    let mut snapshot_state = StateData::new();
    snapshot_state.insert("checkpoint".to_string(), json!("alpha"));
    snapshot_state.insert("progress".to_string(), json!(25));
    
    snapshot_manager.create_snapshot(
        "checkpoint_alpha".to_string(),
        snapshot_state.clone(),
        1,
        None,
    ).await?;
    
    println!("Created snapshot: checkpoint_alpha (25% progress)");
    
    snapshot_state.insert("checkpoint".to_string(), json!("beta"));
    snapshot_state.insert("progress".to_string(), json!(50));
    
    snapshot_manager.create_snapshot(
        "checkpoint_beta".to_string(),
        snapshot_state.clone(),
        2,
        None,
    ).await?;
    
    println!("Created snapshot: checkpoint_beta (50% progress)");
    
    snapshot_state.insert("checkpoint".to_string(), json!("gamma"));
    snapshot_state.insert("progress".to_string(), json!(75));
    
    snapshot_manager.create_snapshot(
        "checkpoint_gamma".to_string(),
        snapshot_state,
        3,
        None,
    ).await?;
    
    println!("Created snapshot: checkpoint_gamma (75% progress)\n");
    
    // List and load snapshots
    let snapshots = snapshot_manager.list_snapshots().await;
    println!("Available snapshots: {:?}\n", snapshots);
    
    let loaded = snapshot_manager.load_snapshot("checkpoint_beta").await?;
    println!("Loaded snapshot: {}", loaded.id);
    println!("  Progress: {}", loaded.state.get("progress").unwrap());
    println!("  Checkpoint: {}\n", loaded.state.get("checkpoint").unwrap());
    
    // State diff calculation
    println!("🔍 State Diff Calculation:");
    
    let mut old_state = StateData::new();
    old_state.insert("name".to_string(), json!("Alice"));
    old_state.insert("score".to_string(), json!(100));
    old_state.insert("level".to_string(), json!(5));
    
    let mut new_state = StateData::new();
    new_state.insert("name".to_string(), json!("Alice"));
    new_state.insert("score".to_string(), json!(150));
    new_state.insert("level".to_string(), json!(6));
    new_state.insert("achievement".to_string(), json!("Expert"));
    
    let diff = StateDiff::calculate(&old_state, &new_state);
    
    println!("Changes from old to new state:");
    println!("  Added: {:?}", diff.added.keys().collect::<Vec<_>>());
    println!("  Modified: {:?}", diff.modified.keys().collect::<Vec<_>>());
    println!("  Removed: {:?}", diff.removed.keys().collect::<Vec<_>>());
    
    if let Some((old_val, new_val)) = diff.modified.get("score") {
        println!("\n  Score changed: {} → {}", old_val, new_val);
    }
    
    if let Some((old_val, new_val)) = diff.modified.get("level") {
        println!("  Level changed: {} → {}", old_val, new_val);
    }
    
    println!("\n✅ State management demonstration complete!");
    
    Ok(())
}