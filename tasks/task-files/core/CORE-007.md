# CORE-007: Implement MessageGraph and MessagesState

## 📋 Task Details
- **ID**: CORE-007
- **Category**: Core
- **Priority**: P0 (Critical)
- **Effort**: 2 days
- **Status**: 🔴 TODO

## 📝 Description
Implement specialized message-based graph types for conversation flows, matching Python LangGraph's `MessageGraph` and `MessagesState` functionality. This is essential for LLM-based conversational applications.

## ✅ Acceptance Criteria
- [ ] Create `MessageGraph` type specialized for conversations
- [ ] Implement `MessagesState` with built-in message management
- [ ] Add message reducers (append, replace, filter)
- [ ] Support message history with configurable limits
- [ ] Implement conversation turn handling
- [ ] Add message metadata support (timestamps, roles, etc.)
- [ ] Enable conversation branching and merging
- [ ] Full serialization/deserialization support
- [ ] Integration tests for multi-turn conversations
- [ ] Documentation and examples

## 🔧 Technical Approach
```rust
pub struct Message {
    pub id: Uuid,
    pub content: String,
    pub role: MessageRole,
    pub metadata: HashMap<String, Value>,
    pub timestamp: DateTime<Utc>,
}

pub struct MessagesState {
    pub messages: Vec<Message>,
    pub max_messages: Option<usize>,
    pub metadata: HashMap<String, Value>,
}

pub struct MessageGraph {
    inner: StateGraph,
    message_reducer: Box<dyn MessageReducer>,
}

impl MessageGraph {
    pub fn new() -> Self;
    pub fn add_message_node<F>(&mut self, name: &str, handler: F);
    pub fn compile(self) -> CompiledMessageGraph;
}
```

## 📚 Resources
- Python LangGraph MessageGraph documentation
- Conversation management patterns
- Message state handling examples

## 🧪 Test Requirements
- Multi-turn conversation tests
- Message history management tests
- Reducer functionality tests
- Conversation branching tests
- Memory limit tests
- Serialization round-trip tests

## Dependencies
- Core graph system (already implemented)
- State management (already implemented)
- Serialization with serde