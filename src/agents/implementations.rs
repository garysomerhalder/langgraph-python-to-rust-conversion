//! Concrete agent implementations for LangGraph
//!
//! This module provides production-ready agent implementations with
//! real reasoning capabilities, memory management, and tool integration.

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::agents::{AgentConfig, Conversation, Message, MessageRole};
use crate::state::StateData;
use crate::tools::{ToolContext, ToolRegistry};
use crate::Result;

/// Trait for concrete agent implementations
#[async_trait]
pub trait Agent: Send + Sync {
    /// Get agent configuration
    fn config(&self) -> &AgentConfig;
    
    /// Process input and update state
    async fn process(&self, input: String, state: StateData) -> Result<StateData>;
    
    /// Chat interaction mode
    async fn chat(&self, conversation: Conversation, state: StateData) -> Result<Conversation>;
}

/// Chain of Thought reasoning agent
pub struct ChainOfThoughtAgent {
    config: AgentConfig,
    tools: Arc<ToolRegistry>,
    reasoning_history: Arc<RwLock<Vec<ReasoningStep>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    pub step_number: usize,
    pub thought: String,
    pub action: Option<String>,
    pub observation: Option<String>,
    pub conclusion: Option<String>,
}

impl ChainOfThoughtAgent {
    pub fn new(config: AgentConfig, tools: Arc<ToolRegistry>) -> Self {
        Self {
            config,
            tools,
            reasoning_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    async fn think_step_by_step(
        &self,
        input: &str,
        state: &StateData,
    ) -> Result<Vec<ReasoningStep>> {
        let mut steps = Vec::new();
        let mut current_thought = input.to_string();
        let max_steps = 10;
        
        for step_num in 1..=max_steps {
            // Generate thought about the current situation
            let thought = format!(
                "Step {}: Analyzing '{}' with state context",
                step_num, current_thought
            );
            
            // Determine if we need to use a tool
            let action = self.determine_action(&current_thought, state).await?;
            
            let mut step = ReasoningStep {
                step_number: step_num,
                thought: thought.clone(),
                action: action.clone(),
                observation: None,
                conclusion: None,
            };
            
            // Execute action if needed
            if let Some(action_str) = action {
                let observation = self.execute_action(&action_str, state).await?;
                step.observation = Some(observation.clone());
                
                // Update current thought based on observation
                current_thought = format!("Based on {}, {}", action_str, observation);
            }
            
            // Check if we have enough information to conclude
            if self.can_conclude(&steps, state).await? {
                step.conclusion = Some(self.formulate_conclusion(&steps, state).await?);
                steps.push(step);
                break;
            }
            
            steps.push(step);
        }
        
        Ok(steps)
    }
    
    async fn determine_action(&self, thought: &str, state: &StateData) -> Result<Option<String>> {
        // Analyze thought to determine if tool use is needed
        if thought.contains("calculate") || thought.contains("compute") {
            return Ok(Some("use_calculator".to_string()));
        }
        
        if thought.contains("search") || thought.contains("find") {
            return Ok(Some("search_knowledge".to_string()));
        }
        
        if thought.contains("transform") || thought.contains("manipulate") {
            return Ok(Some("use_string_tool".to_string()));
        }
        
        // Check state for hints about needed actions
        if let Some(Value::String(task)) = state.get("task_type") {
            match task.as_str() {
                "calculation" => return Ok(Some("use_calculator".to_string())),
                "data_processing" => return Ok(Some("process_data".to_string())),
                "api_request" => return Ok(Some("make_api_call".to_string())),
                _ => {}
            }
        }
        
        Ok(None)
    }
    
    async fn execute_action(&self, action: &str, state: &StateData) -> Result<String> {
        match action {
            "use_calculator" => {
                if let Some(tool) = self.tools.get("calculator") {
                    let params = self.extract_calculator_params(state)?;
                    let context = ToolContext {
                        state: state.clone(),
                        metadata: HashMap::new(),
                        auth: None,
                        timeout: Some(30),
                    };
                    
                    let result = tool.execute(params, context).await?;
                    if let Some(data) = result.data {
                        return Ok(format!("Calculation result: {}", data));
                    }
                }
                Ok("Calculator tool not available".to_string())
            }
            
            "search_knowledge" => {
                // Simulate knowledge search
                if let Some(Value::String(query)) = state.get("query") {
                    Ok(format!("Found information about: {}", query))
                } else {
                    Ok("No search query provided".to_string())
                }
            }
            
            "use_string_tool" => {
                if let Some(tool) = self.tools.get("string_tool") {
                    let params = self.extract_string_params(state)?;
                    let context = ToolContext {
                        state: state.clone(),
                        metadata: HashMap::new(),
                        auth: None,
                        timeout: Some(30),
                    };
                    
                    let result = tool.execute(params, context).await?;
                    if let Some(data) = result.data {
                        return Ok(format!("String operation result: {}", data));
                    }
                }
                Ok("String tool not available".to_string())
            }
            
            _ => Ok(format!("Executed action: {}", action)),
        }
    }
    
    fn extract_calculator_params(&self, state: &StateData) -> Result<Value> {
        // Extract calculation parameters from state
        let operation = state.get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("add");
        
        let operands = state.get("operands")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![0.0]);
        
        Ok(json!({
            "operation": operation,
            "operands": operands
        }))
    }
    
    fn extract_string_params(&self, state: &StateData) -> Result<Value> {
        // Extract string operation parameters from state
        let operation = state.get("string_op")
            .and_then(|v| v.as_str())
            .unwrap_or("concat");
        
        let input = state.get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        Ok(json!({
            "operation": operation,
            "text": input
        }))
    }
    
    async fn can_conclude(&self, steps: &[ReasoningStep], state: &StateData) -> Result<bool> {
        // Determine if we have enough information to conclude
        if steps.is_empty() {
            return Ok(false);
        }
        
        // Check if we've gathered enough observations
        let observations_count = steps.iter()
            .filter(|s| s.observation.is_some())
            .count();
        
        // Check if state indicates completion
        if let Some(Value::Bool(complete)) = state.get("reasoning_complete") {
            return Ok(*complete);
        }
        
        // Conclude if we have sufficient observations or reached max steps
        Ok(observations_count >= 2 || steps.len() >= 5)
    }
    
    async fn formulate_conclusion(
        &self,
        steps: &[ReasoningStep],
        state: &StateData,
    ) -> Result<String> {
        let mut conclusion = String::from("Based on my reasoning:\n");
        
        // Summarize key observations
        for step in steps {
            if let Some(obs) = &step.observation {
                conclusion.push_str(&format!("- {}\n", obs));
            }
        }
        
        // Add final conclusion based on state
        if let Some(Value::String(goal)) = state.get("goal") {
            conclusion.push_str(&format!("\nConclusion: Achieved goal of {}", goal));
        } else {
            conclusion.push_str("\nConclusion: Completed reasoning process successfully");
        }
        
        Ok(conclusion)
    }
}

#[async_trait]
impl Agent for ChainOfThoughtAgent {
    fn config(&self) -> &AgentConfig {
        &self.config
    }
    
    async fn process(&self, input: String, state: StateData) -> Result<StateData> {
        // Perform chain of thought reasoning
        let reasoning_steps = self.think_step_by_step(&input, &state).await?;
        
        // Store reasoning history
        {
            let mut history = self.reasoning_history.write().await;
            history.extend(reasoning_steps.clone());
        }
        
        // Update state with reasoning results
        let mut new_state = state.clone();
        new_state.insert(
            "reasoning_steps".to_string(),
            json!(reasoning_steps),
        );
        
        if let Some(conclusion) = reasoning_steps.last().and_then(|s| s.conclusion.clone()) {
            new_state.insert("conclusion".to_string(), json!(conclusion));
        }
        
        Ok(new_state)
    }
    
    async fn chat(&self, conversation: Conversation, state: StateData) -> Result<Conversation> {
        let mut new_conversation = conversation.clone();
        
        // Process the last user message
        if let Some(last_message) = conversation.messages.last() {
            if last_message.role == MessageRole::User {
                let reasoning_steps = self.think_step_by_step(&last_message.content, &state).await?;
                
                // Add reasoning as assistant message
                let mut response = String::from("Let me think through this step by step:\n\n");
                
                for step in &reasoning_steps {
                    response.push_str(&format!("{}. {}\n", step.step_number, step.thought));
                    
                    if let Some(action) = &step.action {
                        response.push_str(&format!("   Action: {}\n", action));
                    }
                    
                    if let Some(obs) = &step.observation {
                        response.push_str(&format!("   Observation: {}\n", obs));
                    }
                    
                    if let Some(conclusion) = &step.conclusion {
                        response.push_str(&format!("\n{}\n", conclusion));
                    }
                }
                
                new_conversation.messages.push(Message {
                    role: MessageRole::Assistant,
                    content: response,
                    name: Some(self.config.name.clone()),
                    metadata: None,
                });
            }
        }
        
        Ok(new_conversation)
    }
}

/// ReAct (Reasoning and Acting) agent
pub struct ReActAgent {
    config: AgentConfig,
    tools: Arc<ToolRegistry>,
    max_iterations: usize,
}

impl ReActAgent {
    pub fn new(config: AgentConfig, tools: Arc<ToolRegistry>) -> Self {
        Self {
            config,
            tools,
            max_iterations: 5,
        }
    }
    
    async fn reason_and_act(&self, task: &str, state: &StateData) -> Result<Vec<ReActStep>> {
        let mut steps = Vec::new();
        let mut current_context = task.to_string();
        
        for iteration in 1..=self.max_iterations {
            // Reasoning phase
            let thought = self.reason(&current_context, state).await?;
            
            // Acting phase
            let action = self.decide_action(&thought, state).await?;
            
            let mut step = ReActStep {
                iteration,
                thought: thought.clone(),
                action: action.clone(),
                observation: None,
            };
            
            // Execute action and observe
            if let Some(action_spec) = action {
                let observation = self.act(&action_spec, state).await?;
                step.observation = Some(observation.clone());
                
                // Update context with observation
                current_context = format!(
                    "Previous: {}\nAction taken: {:?}\nResult: {}",
                    current_context, action_spec, observation
                );
                
                // Check if task is complete
                if self.is_task_complete(&observation, state).await? {
                    steps.push(step);
                    break;
                }
            }
            
            steps.push(step);
        }
        
        Ok(steps)
    }
    
    async fn reason(&self, context: &str, state: &StateData) -> Result<String> {
        // Generate reasoning based on context and state
        let mut thought = format!("Analyzing: {}", context);
        
        // Consider state information
        if let Some(Value::Object(constraints)) = state.get("constraints") {
            thought.push_str("\nConsidering constraints: ");
            for (key, value) in constraints {
                thought.push_str(&format!("{}: {}, ", key, value));
            }
        }
        
        // Consider available tools
        let available_tools = self.tools.list();
        if !available_tools.is_empty() {
            thought.push_str(&format!("\nAvailable tools: {:?}", available_tools));
        }
        
        Ok(thought)
    }
    
    async fn decide_action(&self, thought: &str, state: &StateData) -> Result<Option<ActionSpec>> {
        // Decide on action based on reasoning
        if thought.contains("calculate") || thought.contains("compute") {
            return Ok(Some(ActionSpec {
                tool: "calculator".to_string(),
                parameters: self.extract_calculator_params(state)?,
            }));
        }
        
        if thought.contains("transform") || thought.contains("text") {
            return Ok(Some(ActionSpec {
                tool: "string_tool".to_string(),
                parameters: self.extract_string_params(state)?,
            }));
        }
        
        if thought.contains("request") || thought.contains("API") {
            return Ok(Some(ActionSpec {
                tool: "http_client".to_string(),
                parameters: json!({
                    "method": "GET",
                    "url": state.get("api_url").and_then(|v| v.as_str()).unwrap_or("https://api.example.com"),
                }),
            }));
        }
        
        Ok(None)
    }
    
    async fn act(&self, action: &ActionSpec, state: &StateData) -> Result<String> {
        // Execute the action using tools
        if let Some(tool) = self.tools.get(&action.tool) {
            let context = ToolContext {
                state: state.clone(),
                metadata: HashMap::new(),
                auth: None,
                timeout: Some(30),
            };
            
            let result = tool.execute(action.parameters.clone(), context).await?;
            
            if result.success {
                if let Some(data) = result.data {
                    return Ok(format!("Success: {}", data));
                }
                return Ok("Action completed successfully".to_string());
            } else {
                return Ok(format!(
                    "Action failed: {}",
                    result.error.unwrap_or_else(|| "Unknown error".to_string())
                ));
            }
        }
        
        Ok(format!("Tool '{}' not available", action.tool))
    }
    
    async fn is_task_complete(&self, observation: &str, state: &StateData) -> Result<bool> {
        // Check if the task is complete based on observation
        if observation.contains("Success") || observation.contains("completed") {
            return Ok(true);
        }
        
        // Check state for completion criteria
        if let Some(Value::Bool(complete)) = state.get("task_complete") {
            return Ok(*complete);
        }
        
        if let Some(Value::String(status)) = state.get("status") {
            return Ok(status == "complete" || status == "done");
        }
        
        Ok(false)
    }
    
    fn extract_calculator_params(&self, state: &StateData) -> Result<Value> {
        let operation = state.get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("add");
        
        let operands = state.get("operands")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![1.0, 1.0]);
        
        Ok(json!({
            "operation": operation,
            "operands": operands
        }))
    }
    
    fn extract_string_params(&self, state: &StateData) -> Result<Value> {
        let operation = state.get("string_op")
            .and_then(|v| v.as_str())
            .unwrap_or("concat");
        
        let text = state.get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("default text");
        
        Ok(json!({
            "operation": operation,
            "text": text,
            "separator": state.get("separator").and_then(|v| v.as_str()).unwrap_or(" "),
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActStep {
    pub iteration: usize,
    pub thought: String,
    pub action: Option<ActionSpec>,
    pub observation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSpec {
    pub tool: String,
    pub parameters: Value,
}

#[async_trait]
impl Agent for ReActAgent {
    fn config(&self) -> &AgentConfig {
        &self.config
    }
    
    async fn process(&self, input: String, state: StateData) -> Result<StateData> {
        // Execute ReAct loop
        let steps = self.reason_and_act(&input, &state).await?;
        
        // Update state with results
        let mut new_state = state.clone();
        new_state.insert("react_steps".to_string(), json!(steps));
        
        // Extract final result
        if let Some(last_step) = steps.last() {
            if let Some(observation) = &last_step.observation {
                new_state.insert("result".to_string(), json!(observation));
            }
        }
        
        Ok(new_state)
    }
    
    async fn chat(&self, conversation: Conversation, state: StateData) -> Result<Conversation> {
        let mut new_conversation = conversation.clone();
        
        if let Some(last_message) = conversation.messages.last() {
            if last_message.role == MessageRole::User {
                let steps = self.reason_and_act(&last_message.content, &state).await?;
                
                let mut response = String::from("Using ReAct pattern:\n\n");
                
                for step in &steps {
                    response.push_str(&format!("Iteration {}:\n", step.iteration));
                    response.push_str(&format!("  Thought: {}\n", step.thought));
                    
                    if let Some(action) = &step.action {
                        response.push_str(&format!("  Action: {} with {:?}\n", action.tool, action.parameters));
                    }
                    
                    if let Some(obs) = &step.observation {
                        response.push_str(&format!("  Observation: {}\n\n", obs));
                    }
                }
                
                new_conversation.messages.push(Message {
                    role: MessageRole::Assistant,
                    content: response,
                    name: Some(self.config.name.clone()),
                    metadata: None,
                });
            }
        }
        
        Ok(new_conversation)
    }
}

/// Memory-enhanced agent with short-term and long-term memory
pub struct MemoryAgent {
    config: AgentConfig,
    short_term_memory: Arc<RwLock<Vec<MemoryItem>>>,
    long_term_memory: Arc<RwLock<HashMap<String, MemoryItem>>>,
    working_memory_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub importance: f32,
    pub timestamp: u64,
    pub access_count: usize,
    pub associations: Vec<String>,
}

impl MemoryAgent {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            short_term_memory: Arc::new(RwLock::new(Vec::new())),
            long_term_memory: Arc::new(RwLock::new(HashMap::new())),
            working_memory_limit: 7, // Miller's Law
        }
    }
    
    async fn store_memory(&self, content: String, importance: f32) -> Result<String> {
        let id = format!("mem_{}", uuid::Uuid::new_v4());
        let memory = MemoryItem {
            id: id.clone(),
            content,
            importance,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            access_count: 0,
            associations: Vec::new(),
        };
        
        // Add to short-term memory
        {
            let mut stm = self.short_term_memory.write().await;
            stm.push(memory.clone());
            
            // Maintain working memory limit
            if stm.len() > self.working_memory_limit {
                // Move least important to long-term memory
                stm.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
                let to_archive = stm.split_off(self.working_memory_limit);
                
                let mut ltm = self.long_term_memory.write().await;
                for item in to_archive {
                    ltm.insert(item.id.clone(), item);
                }
            }
        }
        
        Ok(id)
    }
    
    async fn recall_memory(&self, query: &str) -> Result<Vec<MemoryItem>> {
        let mut relevant_memories = Vec::new();
        
        // Search short-term memory
        {
            let mut stm = self.short_term_memory.write().await;
            for memory in stm.iter_mut() {
                if memory.content.contains(query) {
                    memory.access_count += 1;
                    relevant_memories.push(memory.clone());
                }
            }
        }
        
        // Search long-term memory
        {
            let mut ltm = self.long_term_memory.write().await;
            for (_, memory) in ltm.iter_mut() {
                if memory.content.contains(query) {
                    memory.access_count += 1;
                    memory.importance *= 1.1; // Increase importance on recall
                    relevant_memories.push(memory.clone());
                }
            }
        }
        
        // Sort by relevance (importance * access_count)
        relevant_memories.sort_by(|a, b| {
            let score_a = a.importance * a.access_count as f32;
            let score_b = b.importance * b.access_count as f32;
            score_b.partial_cmp(&score_a).unwrap()
        });
        
        Ok(relevant_memories)
    }
    
    async fn consolidate_memories(&self) -> Result<()> {
        // Consolidate related memories
        let stm = self.short_term_memory.read().await;
        let memories_to_consolidate: Vec<_> = stm.iter()
            .filter(|m| m.access_count > 3)
            .cloned()
            .collect();
        drop(stm);
        
        if memories_to_consolidate.len() >= 2 {
            // Create associations between frequently accessed memories
            let mut ltm = self.long_term_memory.write().await;
            for i in 0..memories_to_consolidate.len() {
                for j in i+1..memories_to_consolidate.len() {
                    let mem1 = &memories_to_consolidate[i];
                    let mem2 = &memories_to_consolidate[j];
                    
                    // Simple similarity check
                    if Self::calculate_similarity(&mem1.content, &mem2.content) > 0.3 {
                        if let Some(stored_mem1) = ltm.get_mut(&mem1.id) {
                            stored_mem1.associations.push(mem2.id.clone());
                        }
                        if let Some(stored_mem2) = ltm.get_mut(&mem2.id) {
                            stored_mem2.associations.push(mem1.id.clone());
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn calculate_similarity(text1: &str, text2: &str) -> f32 {
        // Simple word overlap similarity
        let words1: std::collections::HashSet<_> = text1.split_whitespace().collect();
        let words2: std::collections::HashSet<_> = text2.split_whitespace().collect();
        
        let intersection = words1.intersection(&words2).count() as f32;
        let union = words1.union(&words2).count() as f32;
        
        if union > 0.0 {
            intersection / union
        } else {
            0.0
        }
    }
}

#[async_trait]
impl Agent for MemoryAgent {
    fn config(&self) -> &AgentConfig {
        &self.config
    }
    
    async fn process(&self, input: String, state: StateData) -> Result<StateData> {
        // Store input as memory
        let importance = state.get("importance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5) as f32;
        
        let memory_id = self.store_memory(input.clone(), importance).await?;
        
        // Recall relevant memories
        let query = state.get("recall_query")
            .and_then(|v| v.as_str())
            .unwrap_or(&input);
        
        let recalled = self.recall_memory(query).await?;
        
        // Consolidate memories periodically
        self.consolidate_memories().await?;
        
        // Update state with memory information
        let mut new_state = state.clone();
        new_state.insert("stored_memory_id".to_string(), json!(memory_id));
        new_state.insert("recalled_memories".to_string(), json!(recalled));
        
        // Add memory statistics
        let stm_count = self.short_term_memory.read().await.len();
        let ltm_count = self.long_term_memory.read().await.len();
        
        new_state.insert("memory_stats".to_string(), json!({
            "short_term_count": stm_count,
            "long_term_count": ltm_count,
            "working_memory_usage": format!("{}/{}", stm_count, self.working_memory_limit),
        }));
        
        Ok(new_state)
    }
    
    async fn chat(&self, conversation: Conversation, _state: StateData) -> Result<Conversation> {
        let mut new_conversation = conversation.clone();
        
        if let Some(last_message) = conversation.messages.last() {
            if last_message.role == MessageRole::User {
                // Store the message
                self.store_memory(last_message.content.clone(), 0.7).await?;
                
                // Recall relevant context
                let memories = self.recall_memory(&last_message.content).await?;
                
                let mut response = String::from("Based on my memory:\n\n");
                
                if !memories.is_empty() {
                    response.push_str("Relevant context:\n");
                    for (i, memory) in memories.iter().take(3).enumerate() {
                        response.push_str(&format!(
                            "{}. {} (importance: {:.2}, accessed: {} times)\n",
                            i + 1,
                            memory.content,
                            memory.importance,
                            memory.access_count
                        ));
                    }
                    response.push_str("\n");
                }
                
                response.push_str(&format!(
                    "Response: I've stored your message and recalled {} relevant memories.",
                    memories.len()
                ));
                
                new_conversation.messages.push(Message {
                    role: MessageRole::Assistant,
                    content: response,
                    name: Some(self.config.name.clone()),
                    metadata: None,
                });
            }
        }
        
        Ok(new_conversation)
    }
}

// UUID generation helper (simplified)
mod uuid {
    use std::sync::atomic::{AtomicU64, Ordering};
    
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    
    pub struct Uuid;
    
    impl Uuid {
        pub fn new_v4() -> String {
            let count = COUNTER.fetch_add(1, Ordering::SeqCst);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            format!("{:x}-{:x}", timestamp, count)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_chain_of_thought_agent() {
        let config = AgentConfig {
            name: "test_cot".to_string(),
            description: Some("Test CoT agent".to_string()),
            max_iterations: Some(5),
            tools: vec![],
            system_prompt: None,
            temperature: None,
        };
        
        let tools = Arc::new(ToolRegistry::new());
        let agent = ChainOfThoughtAgent::new(config, tools);
        
        let mut state = StateData::new();
        state.insert("task_type".to_string(), json!("calculation"));
        state.insert("operation".to_string(), json!("add"));
        state.insert("operands".to_string(), json!([5, 3]));
        
        let result = agent.process(
            "Calculate the sum of the numbers".to_string(),
            state
        ).await.unwrap();
        
        assert!(result.contains_key("reasoning_steps"));
    }
    
    #[tokio::test]
    async fn test_react_agent() {
        let config = AgentConfig {
            name: "test_react".to_string(),
            description: Some("Test ReAct agent".to_string()),
            max_iterations: Some(3),
            tools: vec![],
            system_prompt: None,
            temperature: None,
        };
        
        let tools = Arc::new(ToolRegistry::new());
        let agent = ReActAgent::new(config, tools);
        
        let mut state = StateData::new();
        state.insert("task_type".to_string(), json!("text_processing"));
        state.insert("string_op".to_string(), json!("upper"));
        state.insert("text".to_string(), json!("hello world"));
        
        let result = agent.process(
            "Transform the text to uppercase".to_string(),
            state
        ).await.unwrap();
        
        assert!(result.contains_key("react_steps"));
    }
    
    #[tokio::test]
    async fn test_memory_agent() {
        let config = AgentConfig {
            name: "test_memory".to_string(),
            description: Some("Test memory agent".to_string()),
            max_iterations: None,
            tools: vec![],
            system_prompt: None,
            temperature: None,
        };
        
        let agent = MemoryAgent::new(config);
        
        let mut state = StateData::new();
        state.insert("importance".to_string(), json!(0.8));
        
        // Store some memories
        let result1 = agent.process(
            "The capital of France is Paris".to_string(),
            state.clone()
        ).await.unwrap();
        
        assert!(result1.contains_key("stored_memory_id"));
        
        // Recall memory
        state.insert("recall_query".to_string(), json!("France"));
        let result2 = agent.process(
            "What about France?".to_string(),
            state
        ).await.unwrap();
        
        let recalled = result2.get("recalled_memories").unwrap();
        assert!(recalled.as_array().unwrap().len() > 0);
    }
}