use crate::graph::{Edge, Node, StateGraph};
use crate::state::{Reducer, StateChannels};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait GenericCheckpointer<T>: Send + Sync
where
    T: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de>,
{
    async fn save(&self, checkpoint_id: &str, state: &T) -> Result<(), Box<dyn std::error::Error>>;
    async fn load(&self, checkpoint_id: &str) -> Result<T, Box<dyn std::error::Error>>;
}

pub struct StateGraphManager<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub graph: StateGraph,
    pub state: Arc<RwLock<T>>,
    pub channels: StateChannels<T>,
    pub reducers: HashMap<String, Box<dyn Reducer>>,
    pub conditional_edges: Vec<StateConditionalEdge<T>>,
    pub checkpointer: Option<Arc<dyn GenericCheckpointer<T>>>,
}

impl<T> StateGraphManager<T>
where
    T: Clone + Send + Sync + Default + 'static + Serialize + for<'de> Deserialize<'de>,
{
    pub fn new(name: &str) -> Self {
        Self {
            graph: StateGraph::new(name),
            state: Arc::new(RwLock::new(T::default())),
            channels: StateChannels::new(),
            reducers: HashMap::new(),
            conditional_edges: Vec::new(),
            checkpointer: None,
        }
    }

    pub fn with_state(name: &str, initial_state: T) -> Self {
        Self {
            graph: StateGraph::new(name),
            state: Arc::new(RwLock::new(initial_state)),
            channels: StateChannels::new(),
            reducers: HashMap::new(),
            conditional_edges: Vec::new(),
            checkpointer: None,
        }
    }

    pub async fn get_state(&self) -> T {
        self.state.read().await.clone()
    }

    pub async fn update_state<F>(&self, updater: F)
    where
        F: FnOnce(&mut T),
    {
        let mut state = self.state.write().await;
        updater(&mut *state);
    }

    pub async fn merge_state(&self, partial: serde_json::Value, reducer_name: &str)
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        if let Some(reducer) = self.reducers.get(reducer_name) {
            let mut state = self.state.write().await;
            let state_value = serde_json::to_value(&*state).unwrap();
            let merged_value = reducer.reduce(Some(&state_value), partial);
            if let Ok(new_state) = serde_json::from_value::<T>(merged_value) {
                *state = new_state;
            }
        }
    }

    pub fn add_reducer(&mut self, name: String, reducer: Box<dyn Reducer>) {
        self.reducers.insert(name, reducer);
    }

    pub fn add_conditional_edge(&mut self, edge: StateConditionalEdge<T>) {
        self.conditional_edges.push(edge);
    }

    pub async fn evaluate_conditional_edges(&self) -> Vec<(String, String)> {
        let state = self.get_state().await;
        let mut edges = Vec::new();

        for cond_edge in &self.conditional_edges {
            if (cond_edge.condition)(&state) {
                edges.push((cond_edge.source.clone(), cond_edge.target.clone()));
            }
        }

        edges
    }

    pub async fn checkpoint(&self, checkpoint_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(checkpointer) = &self.checkpointer {
            let state = self.get_state().await;
            checkpointer.save(checkpoint_id, &state).await?;
        }
        Ok(())
    }

    pub async fn restore(&self, checkpoint_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(checkpointer) = &self.checkpointer {
            let restored_state = checkpointer.load(checkpoint_id).await?;
            let mut state = self.state.write().await;
            *state = restored_state;
        }
        Ok(())
    }

    pub fn set_checkpointer(&mut self, checkpointer: Arc<dyn GenericCheckpointer<T>>) {
        self.checkpointer = Some(checkpointer);
    }

    pub fn add_node(&mut self, node: Node) {
        self.graph.add_node(node);
    }

    pub fn add_edge(
        &mut self,
        from: &str,
        to: &str,
        edge: Edge,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.graph.add_edge(from, to, edge)?;
        Ok(())
    }

    pub fn set_entry_point(&mut self, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.graph.set_entry_point(node_id)?;
        Ok(())
    }

    pub fn compile(self) -> Result<crate::graph::CompiledGraph, Box<dyn std::error::Error>> {
        Ok(self.graph.compile()?)
    }
}

impl<T> std::fmt::Debug for StateGraphManager<T>
where
    T: Clone + Send + Sync + 'static + std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateGraphManager")
            .field("graph", &"<StateGraph>")
            .field("state", &self.state)
            .field("channels", &"<StateChannels>")
            .field("reducers", &self.reducers.keys().collect::<Vec<_>>())
            .field("conditional_edges", &self.conditional_edges)
            .field("checkpointer", &self.checkpointer.is_some())
            .finish()
    }
}

impl<T> Clone for StateGraphManager<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            graph: self.graph.clone(),
            state: Arc::clone(&self.state),
            channels: self.channels.clone(),
            reducers: HashMap::new(), // Can't clone Box<dyn Reducer>
            conditional_edges: self.conditional_edges.clone(),
            checkpointer: None, // Can't clone Arc<dyn GenericCheckpointer<T>>
        }
    }
}

pub struct StateConditionalEdge<T> {
    pub source: String,
    pub target: String,
    pub condition: Box<dyn Fn(&T) -> bool + Send + Sync>,
}

impl<T> std::fmt::Debug for StateConditionalEdge<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateConditionalEdge")
            .field("source", &self.source)
            .field("target", &self.target)
            .field("condition", &"<function>")
            .finish()
    }
}

impl<T> Clone for StateConditionalEdge<T> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            target: self.target.clone(),
            condition: Box::new(|_| false), // Can't clone closures
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    struct TestState {
        counter: i32,
        message: String,
    }

    #[tokio::test]
    async fn test_state_graph_creation() {
        let graph: StateGraphManager<TestState> = StateGraphManager::new("test");
        let state = graph.get_state().await;
        assert_eq!(state.counter, 0);
        assert_eq!(state.message, "");
    }

    #[tokio::test]
    async fn test_state_update() {
        let graph: StateGraphManager<TestState> = StateGraphManager::new("test");

        graph
            .update_state(|state| {
                state.counter = 42;
                state.message = "Updated".to_string();
            })
            .await;

        let state = graph.get_state().await;
        assert_eq!(state.counter, 42);
        assert_eq!(state.message, "Updated");
    }

    #[tokio::test]
    async fn test_conditional_edge() {
        let mut graph: StateGraphManager<TestState> = StateGraphManager::new("test");

        graph.add_conditional_edge(StateConditionalEdge {
            source: "start".to_string(),
            target: "end".to_string(),
            condition: Box::new(|state| state.counter > 10),
        });

        let edges = graph.evaluate_conditional_edges().await;
        assert_eq!(edges.len(), 0);

        graph
            .update_state(|state| {
                state.counter = 20;
            })
            .await;

        let edges = graph.evaluate_conditional_edges().await;
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].0, "start");
        assert_eq!(edges[0].1, "end");
    }
}
