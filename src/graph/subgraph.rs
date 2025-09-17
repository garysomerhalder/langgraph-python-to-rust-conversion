//! Subgraph implementation for composable workflows

use std::sync::Arc;
use std::collections::HashMap;
use async_trait::async_trait;

use crate::state::StateData;
use crate::graph::{GraphError, CompiledGraph, StateGraph, Node, NodeType};
use crate::Result;

/// A subgraph that can be embedded within a larger graph
#[derive(Clone)]
pub struct Subgraph {
    /// Unique identifier for the subgraph
    pub id: String,
    
    /// The compiled graph to execute
    pub graph: Arc<CompiledGraph>,
    
    /// Input mapping from parent state to subgraph state
    pub input_mapper: Arc<dyn StateMapper>,
    
    /// Output mapping from subgraph state to parent state
    pub output_mapper: Arc<dyn StateMapper>,
    
    /// Whether the subgraph state is isolated from parent
    pub isolated: bool,
}

/// Trait for mapping state between parent and subgraph
#[async_trait]
pub trait StateMapper: Send + Sync {
    /// Map state from source to target
    async fn map(&self, source: &StateData) -> Result<StateData>;
}

/// Default state mapper that passes through all data
pub struct PassthroughMapper;

#[async_trait]
impl StateMapper for PassthroughMapper {
    async fn map(&self, source: &StateData) -> Result<StateData> {
        Ok(source.clone())
    }
}

/// Selective state mapper that only maps specified keys
pub struct SelectiveMapper {
    /// Keys to map from source to target
    pub mappings: HashMap<String, String>,
}

impl SelectiveMapper {
    /// Create a new selective mapper
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }
    
    /// Add a mapping from source key to target key
    pub fn add_mapping(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.mappings.insert(from.into(), to.into());
        self
    }
}

#[async_trait]
impl StateMapper for SelectiveMapper {
    async fn map(&self, source: &StateData) -> Result<StateData> {
        let mut target = StateData::new();
        
        for (from_key, to_key) in &self.mappings {
            if let Some(value) = source.get(from_key) {
                target.insert(to_key.clone(), value.clone());
            }
        }
        
        Ok(target)
    }
}

impl Subgraph {
    /// Create a new subgraph
    pub fn new(id: impl Into<String>, graph: CompiledGraph) -> Self {
        Self {
            id: id.into(),
            graph: Arc::new(graph),
            input_mapper: Arc::new(PassthroughMapper),
            output_mapper: Arc::new(PassthroughMapper),
            isolated: false,
        }
    }
    
    /// Set the input mapper
    pub fn with_input_mapper(mut self, mapper: Arc<dyn StateMapper>) -> Self {
        self.input_mapper = mapper;
        self
    }
    
    /// Set the output mapper
    pub fn with_output_mapper(mut self, mapper: Arc<dyn StateMapper>) -> Self {
        self.output_mapper = mapper;
        self
    }
    
    /// Set whether the subgraph state is isolated
    pub fn with_isolation(mut self, isolated: bool) -> Self {
        self.isolated = isolated;
        self
    }
    
    /// Execute the subgraph with given input state
    pub async fn execute(&self, input: &StateData) -> Result<StateData> {
        // Map input state
        let subgraph_input = self.input_mapper.map(input).await?;
        
        // Execute subgraph (placeholder - actual execution would use executor)
        // let subgraph_output = self.graph.invoke(subgraph_input).await?;
        let subgraph_output = subgraph_input; // Placeholder
        
        // Map output state
        let output = self.output_mapper.map(&subgraph_output).await?;
        
        // Merge with input if not isolated
        if !self.isolated {
            let mut merged = input.clone();
            for (key, value) in output {
                merged.insert(key, value);
            }
            Ok(merged)
        } else {
            Ok(output)
        }
    }
}

/// Builder for creating graphs with subgraphs
pub struct SubgraphBuilder {
    /// Parent graph being built
    parent: StateGraph,
    
    /// Registered subgraphs
    subgraphs: HashMap<String, Subgraph>,
}

impl SubgraphBuilder {
    /// Create a new subgraph builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            parent: StateGraph::new(name),
            subgraphs: HashMap::new(),
        }
    }
    
    /// Add a subgraph node
    pub fn add_subgraph(mut self, subgraph: Subgraph) -> Self {
        let node_id = subgraph.id.clone();
        
        // Add as a special node type in parent graph
        self.parent.add_node(Node {
            id: node_id.clone(),
            node_type: NodeType::Agent(format!("subgraph:{}", node_id)),
            metadata: None,
        });
        
        // Register the subgraph
        self.subgraphs.insert(node_id, subgraph);
        
        self
    }
    
    /// Build the final graph with subgraphs
    pub fn build(self) -> Result<(StateGraph, HashMap<String, Subgraph>)> {
        Ok((self.parent, self.subgraphs))
    }
}

/// Recursive subgraph that can contain other subgraphs
pub struct RecursiveSubgraph {
    /// The main subgraph
    pub subgraph: Subgraph,
    
    /// Nested subgraphs
    pub children: HashMap<String, RecursiveSubgraph>,
    
    /// Maximum recursion depth
    pub max_depth: usize,
}

impl RecursiveSubgraph {
    /// Create a new recursive subgraph
    pub fn new(subgraph: Subgraph, max_depth: usize) -> Self {
        Self {
            subgraph,
            children: HashMap::new(),
            max_depth,
        }
    }
    
    /// Add a child subgraph
    pub fn add_child(&mut self, id: String, child: RecursiveSubgraph) -> Result<()> {
        if child.max_depth >= self.max_depth {
            return Err(GraphError::InvalidStructure(
                "Child subgraph depth exceeds parent limit".to_string()
            ).into());
        }
        
        self.children.insert(id, child);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::graph::GraphBuilder;
    
    #[tokio::test]
    async fn test_state_mappers() {
        let mut source = StateData::new();
        source.insert("input_key".to_string(), json!("value"));
        source.insert("other_key".to_string(), json!(42));
        
        // Test passthrough mapper
        let passthrough = PassthroughMapper;
        let result = passthrough.map(&source).await.unwrap();
        assert_eq!(result, source);
        
        // Test selective mapper
        let selective = SelectiveMapper::new()
            .add_mapping("input_key", "output_key");
        let result = selective.map(&source).await.unwrap();
        assert_eq!(result.get("output_key"), Some(&json!("value")));
        assert_eq!(result.get("other_key"), None);
    }
    
    #[test]
    fn test_subgraph_builder() {
        // Create a simple subgraph
        let subgraph_builder = GraphBuilder::new("sub")
            .add_node("__start__", NodeType::Start)
            .add_node("__end__", NodeType::End)
            .set_entry_point("__start__")
            .add_edge("__start__", "__end__");
        
        let subgraph_compiled = subgraph_builder.build().unwrap().compile().unwrap();
        let subgraph = Subgraph::new("process_sub", subgraph_compiled);
        
        // Build parent graph with subgraph
        let builder = SubgraphBuilder::new("parent")
            .add_subgraph(subgraph);
        
        let (parent, subgraphs) = builder.build().unwrap();
        assert!(parent.get_node("process_sub").is_some());
        assert!(subgraphs.contains_key("process_sub"));
    }
}