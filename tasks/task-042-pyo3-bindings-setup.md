# Task 042: PyO3 Python Bindings Setup and Architecture

## 📋 Task Details
- **Task ID:** 042
- **Title:** Implement PyO3 Python Bindings for Drop-in Replacement
- **Phase:** 🟢 GREEN (Production Ready)
- **Priority:** P0 (Critical Path)
- **Estimated Hours:** 20 hours
- **Assigned To:** Gary Somerhalder
- **Status:** ⏸️ Pending

## 🎯 Description
Create comprehensive Python bindings using PyO3 that enable the Rust LangGraph implementation to be used as a drop-in replacement for the Python version. This includes async/await interoperability, proper error handling, and maintaining identical API surface.

## ✅ Acceptance Criteria
- [ ] PyO3 module structure matching Python LangGraph API
- [ ] Async/await interoperability between Python and Rust
- [ ] All channel types exposed with identical Python interface
- [ ] StateGraph and CompiledGraph Python wrappers functional
- [ ] Error handling with proper Python exception types
- [ ] Memory management and GIL handling optimized
- [ ] Python type hints and documentation generated
- [ ] Integration tests with actual Python LangGraph test suite
- [ ] Performance benchmarks showing significant improvements
- [ ] Distribution packaging for PyPI deployment

## 📦 Dependencies
- **Prerequisites:** Task 020 (Pregel Core), Task 040 (All Channels)
- **Blocks:** Task 043 (Python API Wrapper), Task 058 (Integration Testing)
- **Related:** Task 044 (Documentation Generation)

## 🔧 Technical Notes

### PyO3 Module Structure

```rust
// src/python_bindings/mod.rs
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3_asyncio::tokio::future_into_py;

/// Main Python module entry point
#[pymodule]
fn langgraph_rust(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyStateGraph>()?;
    m.add_class::<PyCompiledGraph>()?;
    m.add_class::<PyLastValueChannel>()?;
    m.add_class::<PyContextChannel>()?;
    m.add_class::<PyTopicChannel>()?;
    m.add_class::<PyDynamicBarrierChannel>()?;
    m.add_class::<PyNamedBarrierChannel>()?;
    m.add_class::<PyEphemeralValueChannel>()?;
    m.add_class::<PyAnyValueChannel>()?;
    m.add_class::<PyBinaryOperatorChannel>()?;
    
    m.add_function(wrap_pyfunction!(create_state_graph, m)?)?;
    m.add_function(wrap_pyfunction!(create_compiled_graph, m)?)?;
    
    // Add exception types
    m.add("ChannelError", _py.get_type::<PyChannelError>())?;
    m.add("PregelError", _py.get_type::<PyPregelError>())?;
    
    Ok(())
}
```

### Async-Enabled Channel Bindings

```rust
/// Python wrapper for LastValue channel
#[pyclass(name = "LastValue")]
#[derive(Clone)]
pub struct PyLastValueChannel {
    inner: Arc<LastValueChannel<PyObject>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[pymethods]
impl PyLastValueChannel {
    #[new]
    fn new(name: String) -> Self {
        Self {
            inner: Arc::new(LastValueChannel::new(name)),
            runtime: Arc::new(tokio::runtime::Runtime::new().unwrap()),
        }
    }
    
    /// Async get method exposed to Python
    fn get<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let inner = self.inner.clone();
        
        future_into_py(py, async move {
            let result = inner.get().await
                .map_err(|e| PyChannelError::new_err(e.to_string()))?;
            
            Python::with_gil(|py| {
                Ok(result.into_py(py))
            })
        })
    }
    
    /// Async update method with Python list input
    fn update<'py>(&self, py: Python<'py>, values: &PyList) -> PyResult<&'py PyAny> {
        let inner = self.inner.clone();
        
        // Convert Python list to Rust Vec<PyObject>
        let rust_values: Vec<PyObject> = values.iter()
            .map(|item| item.into())
            .collect();
        
        future_into_py(py, async move {
            inner.update(rust_values).await
                .map_err(|e| PyChannelError::new_err(e.to_string()))?;
            
            Python::with_gil(|py| Ok(py.None()))
        })
    }
    
    /// Synchronous interface for backwards compatibility
    fn get_sync(&self, py: Python) -> PyResult<PyObject> {
        py.allow_threads(|| {
            self.runtime.block_on(async {
                let result = self.inner.get().await
                    .map_err(|e| PyChannelError::new_err(e.to_string()))?;
                
                Python::with_gil(|py| Ok(result.into_py(py)))
            })
        })
    }
    
    fn update_sync(&self, py: Python, values: &PyList) -> PyResult<()> {
        let rust_values: Vec<PyObject> = values.iter()
            .map(|item| item.into())
            .collect();
        
        py.allow_threads(|| {
            self.runtime.block_on(async {
                self.inner.update(rust_values).await
                    .map_err(|e| PyChannelError::new_err(e.to_string()))
            })
        })
    }
    
    /// Property access for channel name
    #[getter]
    fn name(&self) -> String {
        self.inner.name().to_string()
    }
    
    /// Channel type introspection
    #[getter]
    fn channel_type(&self) -> String {
        format!("{:?}", self.inner.channel_type())
    }
    
    /// Python context manager support
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    
    fn __exit__(
        &self,
        _exc_type: Option<&PyAny>,
        _exc_value: Option<&PyAny>,
        _traceback: Option<&PyAny>,
    ) -> PyResult<bool> {
        // Cleanup if needed
        Ok(false)
    }
}
```

### StateGraph Python Wrapper

```rust
/// Python wrapper for StateGraph
#[pyclass(name = "StateGraph")]
pub struct PyStateGraph {
    inner: Arc<StateGraph>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[pymethods]
impl PyStateGraph {
    #[new]
    #[pyo3(signature = (schema, **kwargs))]
    fn new(schema: &PyAny, kwargs: Option<&PyDict>) -> PyResult<Self> {
        // Convert Python schema to Rust GraphSchema
        let rust_schema = convert_python_schema(schema)?;
        
        // Handle keyword arguments
        let config = if let Some(kw) = kwargs {
            convert_python_config(kw)?
        } else {
            StateGraphConfig::default()
        };
        
        Ok(Self {
            inner: Arc::new(StateGraph::new(rust_schema, config)),
            runtime: Arc::new(tokio::runtime::Runtime::new().unwrap()),
        })
    }
    
    /// Add node to the graph
    fn add_node(&self, name: String, func: PyObject) -> PyResult<()> {
        let python_func = PyComputeFunction::new(func);
        self.inner.add_node(name, Box::new(python_func))
            .map_err(|e| PyStateGraphError::new_err(e.to_string()))
    }
    
    /// Add edge between nodes
    #[pyo3(signature = (from_node, to_node, condition = None))]
    fn add_edge(
        &self,
        from_node: String,
        to_node: String,
        condition: Option<PyObject>,
    ) -> PyResult<()> {
        let edge = if let Some(cond) = condition {
            Edge::conditional(from_node, to_node, PyCondition::new(cond))
        } else {
            Edge::direct(from_node, to_node)
        };
        
        self.inner.add_edge(edge)
            .map_err(|e| PyStateGraphError::new_err(e.to_string()))
    }
    
    /// Compile graph for execution
    fn compile<'py>(
        &self,
        py: Python<'py>,
        checkpointer: Option<PyObject>,
    ) -> PyResult<PyCompiledGraph> {
        let checkpoint_manager = if let Some(cp) = checkpointer {
            Some(PyCheckpointManager::new(cp))
        } else {
            None
        };
        
        let compiled = self.runtime.block_on(async {
            self.inner.compile(checkpoint_manager).await
                .map_err(|e| PyStateGraphError::new_err(e.to_string()))
        })?;
        
        Ok(PyCompiledGraph::new(compiled, self.runtime.clone()))
    }
    
    /// Set entry point
    fn set_entry_point(&self, node: String) -> PyResult<()> {
        self.inner.set_entry_point(node)
            .map_err(|e| PyStateGraphError::new_err(e.to_string()))
    }
    
    /// Set finish point  
    fn set_finish_point(&self, node: String) -> PyResult<()> {
        self.inner.set_finish_point(node)
            .map_err(|e| PyStateGraphError::new_err(e.to_string()))
    }
    
    /// Python dict-like access to nodes
    fn __getitem__(&self, key: &str) -> PyResult<PyObject> {
        // Return node information
        todo!("Implement node access")
    }
    
    fn __setitem__(&self, key: &str, value: PyObject) -> PyResult<()> {
        // Set node function
        todo!("Implement node setting")
    }
    
    /// String representation
    fn __repr__(&self) -> String {
        format!("StateGraph(nodes={}, edges={})", 
                self.inner.node_count(), 
                self.inner.edge_count())
    }
}
```

### Python Function Wrapper

```rust
/// Wrapper for Python callable functions
pub struct PyComputeFunction {
    func: PyObject,
}

impl PyComputeFunction {
    pub fn new(func: PyObject) -> Self {
        Self { func }
    }
}

#[async_trait]
impl ComputeFn for PyComputeFunction {
    async fn call(&self, context: NodeContext) -> Result<ComputeResult, PregelError> {
        // Convert NodeContext to Python dict
        let py_context = Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("node_id", context.node_id.to_string())?;
            dict.set_item("execution_id", context.execution_id.to_string())?;
            dict.set_item("iteration", context.iteration)?;
            dict.set_item("input_state", context.input_state.into_py(py))?;
            Ok::<_, PyErr>(dict.into())
        })?;
        
        // Call Python function
        let result = Python::with_gil(|py| {
            let result = if self.is_async_function(py)? {
                // Handle async Python function
                self.call_async_function(py, py_context)
            } else {
                // Handle sync Python function
                self.func.call1(py, (py_context,))
            }?;
            
            // Convert Python result to ComputeResult
            self.convert_python_result(py, result)
        })?;
        
        Ok(result)
    }
}

impl PyComputeFunction {
    fn is_async_function(&self, py: Python) -> PyResult<bool> {
        let inspect = py.import("inspect")?;
        let is_coroutine = inspect.call_method1("iscoroutinefunction", (&self.func,))?;
        is_coroutine.extract()
    }
    
    fn call_async_function(&self, py: Python, args: PyObject) -> PyResult<PyObject> {
        // Use asyncio to run the coroutine
        let asyncio = py.import("asyncio")?;
        let loop = asyncio.call_method0("new_event_loop")?;
        let coro = self.func.call1(py, (args,))?;
        loop.call_method1("run_until_complete", (coro,))
    }
    
    fn convert_python_result(&self, py: Python, result: PyObject) -> PyResult<ComputeResult> {
        // Convert Python dict/object back to ComputeResult
        let dict = result.downcast::<PyDict>(py)?;
        
        let outputs = dict.get_item("outputs")
            .unwrap_or(py.None())
            .extract::<HashMap<String, serde_json::Value>>()?;
        
        let messages = dict.get_item("messages")
            .unwrap_or_else(|| PyList::empty(py).into())
            .extract::<Vec<Message>>()?;
        
        let next_nodes = dict.get_item("next_nodes")
            .unwrap_or_else(|| PyList::empty(py).into())
            .extract::<Vec<NodeId>>()?;
        
        Ok(ComputeResult {
            outputs,
            messages,
            next_nodes,
            checkpoint_data: None,
        })
    }
}
```

### Error Handling and Exceptions

```rust
/// Custom Python exception types
create_exception!(langgraph_rust, PyChannelError, pyo3::exceptions::PyException);
create_exception!(langgraph_rust, PyPregelError, pyo3::exceptions::PyException);
create_exception!(langgraph_rust, PyStateGraphError, pyo3::exceptions::PyException);
create_exception!(langgraph_rust, PyCheckpointError, pyo3::exceptions::PyException);

/// Conversion from Rust errors to Python exceptions
impl From<ChannelError> for PyErr {
    fn from(err: ChannelError) -> PyErr {
        PyChannelError::new_err(err.to_string())
    }
}

impl From<PregelError> for PyErr {
    fn from(err: PregelError) -> PyErr {
        PyPregelError::new_err(err.to_string())
    }
}
```

## 🧪 Testing Requirements
- [ ] All Python-exposed APIs work identically to original LangGraph
- [ ] Async/await functionality works in Python asyncio environment
- [ ] Error handling propagates correctly to Python exceptions
- [ ] Memory management handles GIL correctly without leaks
- [ ] Performance benchmarks show significant improvements
- [ ] Type hints generate correctly for Python IDEs
- [ ] Integration with Python test suite passes 100%
- [ ] Cross-platform compatibility (Windows, macOS, Linux)

## 📝 Implementation Steps
1. **Setup PyO3 project structure** with proper Cargo.toml configuration
2. **Create basic module exports** for all core types
3. **Implement async/await interoperability** using pyo3-asyncio
4. **Build comprehensive channel wrappers** with identical APIs
5. **Create StateGraph and CompiledGraph wrappers** with full functionality
6. **Implement Python function calling infrastructure** for compute functions
7. **Add comprehensive error handling** with proper Python exception types
8. **Create extensive integration tests** using Python LangGraph test suite
9. **Optimize memory management** and GIL handling for performance
10. **Generate Python type hints** and documentation

## 🔗 Related Tasks
- **Foundation:** [Task 020: Pregel Core](task-020-pregel-core-architecture.md), [Task 040: All Channels](task-040-complete-all-channels.md)
- **Next:** [Task 043: Python API Wrapper](task-043-python-api-wrapper.md)
- **Testing:** [Task 058: Integration Testing](task-058-integration-testing.md)
- **Docs:** [Task 044: Documentation](task-044-documentation-generation.md)
- **Tracker:** [Master Tracker](tracker/tracker.md)

## 📊 Success Metrics
- 100% Python LangGraph API compatibility verified through test suite
- Async operations show 5-10x performance improvement over Python
- Memory usage stable with no GIL-related deadlocks
- Python type checking works correctly in IDEs
- Binary distribution size <10MB for wheel packages
- Import time <100ms for langgraph_rust module

## 🚨 Risk Factors
- **High Risk:** Complex async/await interoperability between Python and Rust
- **GIL Management:** Avoiding deadlocks and performance bottlenecks
- **Memory Safety:** Proper object lifetime management across language boundary
- **API Compatibility:** Matching Python's dynamic behavior exactly in static Rust

## 💡 Design Decisions
- **pyo3-asyncio:** Use for seamless async/await integration
- **Type Safety:** Maintain Rust type safety while exposing dynamic Python API  
- **Performance:** Minimize GIL acquisition and Python object conversions
- **Compatibility:** Prioritize exact API match over internal optimization

## 📅 Timeline
- **Start:** Week 8, Day 4
- **Target Completion:** Week 9, Day 4
- **Buffer:** 1 day for async interoperability complexity

---
*Created: 2025-09-14*  
*Last Updated: 2025-09-14*  
*Status Changed: 2025-09-14*