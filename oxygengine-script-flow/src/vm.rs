use crate::{ast, ast::*, GUID};
use core::prefab::{PrefabNumber, PrefabValue};
use petgraph::{
    algo::{has_path_connecting, toposort},
    Direction, Graph,
};
use std::{
    collections::{BTreeMap, HashMap},
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

const VERSION: usize = 1;
const VERSION_MIN: usize = 1;

#[derive(Debug, Clone)]
pub struct Reference(pub Arc<RwLock<Value>>);

impl Reference {
    pub fn new(inner: Arc<RwLock<Value>>) -> Self {
        Self(inner)
    }

    pub fn value(value: Value) -> Self {
        Self(Arc::new(RwLock::new(value)))
    }

    pub fn into_inner(self) -> Value {
        (*self.read()).clone()
    }

    pub fn read(&self) -> RwLockReadGuard<Value> {
        self.0.read().unwrap()
    }

    pub fn write(&mut self) -> RwLockWriteGuard<Value> {
        self.0.write().unwrap()
    }
}

impl PartialEq for Reference {
    fn eq(&self, other: &Self) -> bool {
        if let Ok(a) = self.0.try_read() {
            if let Ok(b) = other.0.try_read() {
                return *a == *b;
            }
        }
        false
    }
}

impl Deref for Reference {
    type Target = Arc<RwLock<Value>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Reference {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    None,
    Bool(bool),
    Number(PrefabNumber),
    String(String),
    List(Vec<Reference>),
    Object(BTreeMap<String, Reference>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::List(a), Self::List(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => a == b,
            _ => false,
        }
    }
}

impl From<PrefabValue> for Value {
    fn from(value: PrefabValue) -> Self {
        match value {
            PrefabValue::Null => Value::None,
            PrefabValue::Bool(v) => Value::Bool(v),
            PrefabValue::Number(v) => Value::Number(v),
            PrefabValue::String(v) => Value::String(v),
            PrefabValue::Sequence(v) => {
                Value::List(v.into_iter().map(|v| Reference::value(v.into())).collect())
            }
            PrefabValue::Mapping(v) => Value::Object(
                v.into_iter()
                    .map(|(k, v)| {
                        // TODO: return error instead of panicking.
                        let k = if let PrefabValue::String(k) = k {
                            k
                        } else {
                            panic!("Mapping key is not a string: {:?}", k);
                        };
                        let v = Reference::value(v.into());
                        (k, v)
                    })
                    .collect(),
            ),
        }
    }
}

impl Into<PrefabValue> for Value {
    fn into(self) -> PrefabValue {
        match self {
            Value::None => PrefabValue::Null,
            Value::Bool(v) => PrefabValue::Bool(v),
            Value::Number(v) => PrefabValue::Number(v),
            Value::String(v) => PrefabValue::String(v),
            Value::List(v) => {
                PrefabValue::Sequence(v.into_iter().map(|v| v.read().clone().into()).collect())
            }
            Value::Object(v) => PrefabValue::Mapping(
                v.into_iter()
                    .map(|(k, v)| {
                        let k = PrefabValue::String(k);
                        let v = v.read().clone().into();
                        (k, v)
                    })
                    .collect(),
            ),
        }
    }
}

impl Into<Reference> for Value {
    fn into(self) -> Reference {
        Reference::value(self)
    }
}

#[derive(Debug)]
pub enum VmError {
    /// (program version, virtual machine version)
    ProgramCompiledForDifferentVersion(usize, usize),
    Message(String),
    FoundCycleInFlowGraph,
    /// (expected, provided)
    WrongNumberOfInputs(usize, usize),
    /// (expected, provided)
    WrongNumberOfOutputs(usize, usize),
    CouldNotRunEvent(String),
    CouldNotCallFunction(ast::Reference),
    CouldNotCallMethod(ast::Reference, ast::Reference),
    EventDoesNotExists(ast::Reference),
    NodeDoesNotExists(ast::Reference),
    TypeDoesNotExists(ast::Reference),
    TraitDoesNotExists(ast::Reference),
    MethodDoesNotExists(ast::Reference),
    FunctionDoesNotExists(ast::Reference),
    /// (type guid, method guid)
    TypeDoesNotImplementMethod(ast::Reference, ast::Reference),
    InstanceDoesNotExists,
    GlobalVariableDoesNotExists(ast::Reference),
    LocalVariableDoesNotExists(ast::Reference),
    InputDoesNotExists(usize),
    OutputDoesNotExists(usize),
    StackUnderflow,
    OperationDoesNotExists(ast::Reference),
    OperationIsNotRegistered(String),
    /// (expected, provided, list)
    IndexOutOfBounds(usize, usize, Reference),
    ObjectKeyDoesNotExists(String, Reference),
    ValueIsNotAList(Reference),
    ValueIsNotAnObject(Reference),
    ValueIsNotABool(Reference),
    TryingToPerformInvalidNodeType(NodeType),
    /// (source value, destination value)
    TryingToWriteBorrowedReference(Reference, Reference),
    TryingToReadBorrowedReference(Reference),
    NodeNotFoundInExecutionPipeline(ast::Reference),
    NodeIsNotALoop(ast::Reference),
    NodeIsNotAnIfElse(ast::Reference),
    TryingToBreakIfElse,
    TryingToContinueIfElse,
    ThereAreNoCachedNodeOutputs(ast::Reference),
    ThereIsNoCachedNodeIndexedOutput(Link),
    FoundMultipleEntryNodes(Vec<ast::Reference>),
    EntryNodeNotFound,
    FoundNodeWithInvalidIdentifier,
    NodeCannotFlowIn(ast::Reference),
    NodeCannotFlowOut(ast::Reference),
    NodeCannotTakeInput(ast::Reference),
    NodeCannotGiveOutput(ast::Reference),
}

#[derive(Debug)]
pub enum VmOperationError {
    CouldNotPerformOperation {
        message: String,
        name: String,
        inputs: Vec<Value>,
    },
}

pub struct Vm {
    ast: Program,
    operations: HashMap<String, Box<dyn VmOperation>>,
    variables: HashMap<String, Reference>,
    running_events: HashMap<GUID, VmEvent>,
    completed_events: HashMap<GUID, Vec<Reference>>,
    /// {event name: [nodes reference]}
    event_execution_order: HashMap<String, Vec<ast::Reference>>,
    /// {(type name, method name): [node reference]}
    method_execution_order: HashMap<(String, String), Vec<ast::Reference>>,
    /// {event name: [nodes reference]}
    function_execution_order: HashMap<String, Vec<ast::Reference>>,
    /// {type name: {method name: (trait name, is implemented by type)}}
    type_methods: HashMap<String, HashMap<String, (String, bool)>>,
    end_nodes: Vec<ast::Reference>,
}

impl Vm {
    pub fn new(ast: Program) -> Result<Self, VmError> {
        if ast.version < VERSION_MIN || ast.version > VERSION {
            return Err(VmError::ProgramCompiledForDifferentVersion(
                ast.version,
                VERSION,
            ));
        }
        let type_methods = ast
            .types
            .iter()
            .map(|type_| {
                let mut map = HashMap::new();
                for (trait_name, methods) in &type_.traits_implementation {
                    if let Some(trait_) = ast.traits.iter().find(|t| &t.name == trait_name) {
                        for trait_method in &trait_.methods {
                            let b = methods.iter().any(|m| m.name == trait_method.name);
                            map.insert(trait_method.name.clone(), (trait_name.clone(), b));
                        }
                    } else {
                        return Err(VmError::TraitDoesNotExists(ast::Reference::Named(
                            trait_name.clone(),
                        )));
                    }
                }
                Ok((type_.name.clone(), map))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let mut end_nodes = vec![];
        let event_execution_order = ast
            .events
            .iter()
            .map(|event| {
                let mut graph = Graph::<ast::Reference, ()>::new();
                let nodes_map = event
                    .nodes
                    .iter()
                    .map(|node| (node.id.clone(), graph.add_node(node.id.clone())))
                    .collect::<HashMap<_, _>>();
                for node in &event.nodes {
                    if node.id == ast::Reference::None {
                        return Err(VmError::FoundNodeWithInvalidIdentifier);
                    }
                    let (can_input, _, _, can_out) = node.node_type.is_input_output_flow_in_out();
                    if node.next_node != ast::Reference::None {
                        if !can_out {
                            return Err(VmError::NodeCannotFlowOut(node.id.clone()));
                        }
                        if let Some(n) = event.nodes.iter().find(|n| n.id == node.next_node) {
                            let (_, _, can_in2, _) = n.node_type.is_input_output_flow_in_out();
                            if !can_in2 {
                                return Err(VmError::NodeCannotFlowIn(n.id.clone()));
                            }
                        }
                    }
                    if !node.input_links.is_empty() {
                        if !can_input {
                            return Err(VmError::NodeCannotTakeInput(node.id.clone()));
                        }
                        for link in &node.input_links {
                            match link {
                                Link::NodeIndexed(id, _) => {
                                    if let Some(n) = event.nodes.iter().find(|n| &n.id == id) {
                                        let (_, can_output2, _, _) =
                                            n.node_type.is_input_output_flow_in_out();
                                        if !can_output2 {
                                            return Err(VmError::NodeCannotGiveOutput(
                                                n.id.clone(),
                                            ));
                                        }
                                    }
                                }
                                Link::None => {}
                            }
                        }
                    }
                    let to = if let Some(node) = nodes_map.get(&node.next_node) {
                        *node
                    } else {
                        continue;
                    };
                    let from = if let Some(node) = nodes_map.get(&node.id) {
                        *node
                    } else {
                        return Err(VmError::NodeDoesNotExists(node.id.clone()));
                    };
                    graph.add_edge(from, to, ());
                    for link in &node.input_links {
                        match link {
                            Link::NodeIndexed(id, _) => {
                                let from = *nodes_map.get(&id).unwrap();
                                let to = *nodes_map.get(&node.id).unwrap();
                                graph.add_edge(from, to, ());
                            }
                            Link::None => {}
                        }
                    }
                }
                let list = {
                    let indices = match toposort(&graph, None) {
                        Ok(list) => Ok(list),
                        Err(_) => Err(VmError::FoundCycleInFlowGraph),
                    }?;
                    let list = indices
                        .iter()
                        .map(|index| {
                            nodes_map
                                .iter()
                                .find(|(_, v)| *v == index)
                                .unwrap()
                                .0
                                .clone()
                        })
                        .collect::<Vec<_>>();
                    let entry_pos = {
                        let entries = event
                            .nodes
                            .iter()
                            .filter(|n| n.node_type.is_entry())
                            .map(|n| n.id.clone())
                            .collect::<Vec<_>>();
                        if entries.is_empty() {
                            return Err(VmError::EntryNodeNotFound);
                        } else if entries.len() > 1 {
                            return Err(VmError::FoundMultipleEntryNodes(entries));
                        }
                        list.iter().position(|n| n == &entries[0]).unwrap()
                    };
                    let mut from_pos = entry_pos;
                    let mut to_pos = entry_pos;
                    for i in (0..entry_pos).rev() {
                        let idx = indices[i];
                        if graph.externals(Direction::Incoming).any(|n| n == idx)
                            && !has_path_connecting(&graph, idx, indices[entry_pos], None)
                        {
                            break;
                        }
                        from_pos = i;
                    }
                    for i in (entry_pos + 1)..list.len() {
                        let idx = indices[i];
                        if graph.externals(Direction::Outgoing).any(|n| n == idx)
                            && !has_path_connecting(&graph, indices[entry_pos], indices[i], None)
                        {
                            break;
                        }
                        to_pos = i;
                    }
                    list[from_pos..=to_pos].to_vec()
                };
                end_nodes.extend(graph.externals(Direction::Outgoing).map(|index| {
                    nodes_map
                        .iter()
                        .find(|(_, v)| **v == index)
                        .unwrap()
                        .0
                        .clone()
                }));
                Ok((event.name.clone(), list))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let method_execution_order = {
            let mut result = HashMap::new();
            for type_ in &ast.types {
                for (trait_name, methods) in &type_.traits_implementation {
                    if let Some(trait_) = ast.traits.iter().find(|t| &t.name == trait_name) {
                        for method in &trait_.methods {
                            let method = if let Some(method) =
                                methods.iter().find(|m| m.name == method.name)
                            {
                                method
                            } else {
                                method
                            };
                            let mut graph = Graph::<ast::Reference, ()>::new();
                            let nodes_map = method
                                .nodes
                                .iter()
                                .map(|node| (node.id.clone(), graph.add_node(node.id.clone())))
                                .collect::<HashMap<_, _>>();
                            for node in &method.nodes {
                                let to = *nodes_map.get(&node.next_node).unwrap();
                                let from = *nodes_map.get(&node.id).unwrap();
                                graph.add_edge(from, to, ());
                                for link in &node.input_links {
                                    match link {
                                        Link::NodeIndexed(id, _) => {
                                            let to = *nodes_map.get(&node.id).unwrap();
                                            let from = *nodes_map.get(&id).unwrap();
                                            graph.add_edge(from, to, ());
                                        }
                                        Link::None => {}
                                    }
                                }
                            }
                            let list = {
                                let indices = match toposort(&graph, None) {
                                    Ok(list) => Ok(list),
                                    Err(_) => Err(VmError::FoundCycleInFlowGraph),
                                }?;
                                let list = indices
                                    .iter()
                                    .map(|index| {
                                        nodes_map
                                            .iter()
                                            .find(|(_, v)| *v == index)
                                            .unwrap()
                                            .0
                                            .clone()
                                    })
                                    .collect::<Vec<_>>();
                                let entry_pos = {
                                    let entries = method
                                        .nodes
                                        .iter()
                                        .filter(|n| n.node_type.is_entry())
                                        .map(|n| n.id.clone())
                                        .collect::<Vec<_>>();
                                    if entries.is_empty() {
                                        return Err(VmError::EntryNodeNotFound);
                                    } else if entries.len() > 1 {
                                        return Err(VmError::FoundMultipleEntryNodes(entries));
                                    }
                                    list.iter().position(|n| n == &entries[0]).unwrap()
                                };
                                let mut from_pos = entry_pos;
                                let mut to_pos = entry_pos;
                                for i in (0..entry_pos).rev() {
                                    let idx = indices[i];
                                    if graph.externals(Direction::Incoming).any(|n| n == idx)
                                        && !has_path_connecting(
                                            &graph,
                                            idx,
                                            indices[entry_pos],
                                            None,
                                        )
                                    {
                                        break;
                                    }
                                    from_pos = i;
                                }
                                for i in (entry_pos + 1)..list.len() {
                                    let idx = indices[i];
                                    if graph.externals(Direction::Outgoing).any(|n| n == idx)
                                        && !has_path_connecting(
                                            &graph,
                                            indices[entry_pos],
                                            indices[i],
                                            None,
                                        )
                                    {
                                        break;
                                    }
                                    to_pos = i;
                                }
                                list[from_pos..=to_pos].to_vec()
                            };
                            end_nodes.extend(graph.externals(Direction::Outgoing).map(|index| {
                                nodes_map
                                    .iter()
                                    .find(|(_, v)| **v == index)
                                    .unwrap()
                                    .0
                                    .clone()
                            }));
                            result.insert((type_.name.clone(), method.name.clone()), list);
                        }
                    }
                }
            }
            result
        };
        let function_execution_order = ast
            .functions
            .iter()
            .map(|function| {
                let mut graph = Graph::<ast::Reference, ()>::new();
                let nodes_map = function
                    .nodes
                    .iter()
                    .map(|node| (node.id.clone(), graph.add_node(node.id.clone())))
                    .collect::<HashMap<_, _>>();
                for node in &function.nodes {
                    let to = *nodes_map.get(&node.next_node).unwrap();
                    let from = *nodes_map.get(&node.id).unwrap();
                    graph.add_edge(from, to, ());
                    for link in &node.input_links {
                        match link {
                            Link::NodeIndexed(id, _) => {
                                let to = *nodes_map.get(&node.id).unwrap();
                                let from = *nodes_map.get(&id).unwrap();
                                graph.add_edge(from, to, ());
                            }
                            Link::None => {}
                        }
                    }
                }
                let list = {
                    let indices = match toposort(&graph, None) {
                        Ok(list) => Ok(list),
                        Err(_) => Err(VmError::FoundCycleInFlowGraph),
                    }?;
                    let list = indices
                        .iter()
                        .map(|index| {
                            nodes_map
                                .iter()
                                .find(|(_, v)| *v == index)
                                .unwrap()
                                .0
                                .clone()
                        })
                        .collect::<Vec<_>>();
                    let entry_pos = {
                        let entries = function
                            .nodes
                            .iter()
                            .filter(|n| n.node_type.is_entry())
                            .map(|n| n.id.clone())
                            .collect::<Vec<_>>();
                        if entries.is_empty() {
                            return Err(VmError::EntryNodeNotFound);
                        } else if entries.len() > 1 {
                            return Err(VmError::FoundMultipleEntryNodes(entries));
                        }
                        list.iter().position(|n| n == &entries[0]).unwrap()
                    };
                    let mut from_pos = entry_pos;
                    let mut to_pos = entry_pos;
                    for i in (0..entry_pos).rev() {
                        let idx = indices[i];
                        if graph.externals(Direction::Incoming).any(|n| n == idx)
                            && !has_path_connecting(&graph, idx, indices[entry_pos], None)
                        {
                            break;
                        }
                        from_pos = i;
                    }
                    for i in (entry_pos + 1)..list.len() {
                        let idx = indices[i];
                        if graph.externals(Direction::Outgoing).any(|n| n == idx)
                            && !has_path_connecting(&graph, indices[entry_pos], indices[i], None)
                        {
                            break;
                        }
                        to_pos = i;
                    }
                    list[from_pos..=to_pos].to_vec()
                };
                end_nodes.extend(graph.externals(Direction::Outgoing).map(|index| {
                    nodes_map
                        .iter()
                        .find(|(_, v)| **v == index)
                        .unwrap()
                        .0
                        .clone()
                }));
                Ok((function.name.clone(), list))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let variables = ast
            .variables
            .iter()
            .map(|v| (v.name.clone(), Value::None.into()))
            .collect();
        let result = Self {
            ast,
            operations: Default::default(),
            variables,
            running_events: Default::default(),
            completed_events: Default::default(),
            event_execution_order,
            method_execution_order,
            function_execution_order,
            type_methods,
            end_nodes,
        };
        Ok(result)
    }

    pub fn version_range() -> (usize, usize) {
        (VERSION_MIN, VERSION)
    }

    pub fn register_operation<T>(&mut self, name: &str, operator: T)
    where
        T: VmOperation + 'static,
    {
        self.operations.insert(name.to_owned(), Box::new(operator));
    }

    pub fn unregister_operator(&mut self, name: &str) -> bool {
        self.operations.remove(name).is_some()
    }

    pub fn global_variable_value(&self, name: &str) -> Result<Reference, VmError> {
        if let Some(value) = self.variables.get(name) {
            Ok(value.clone())
        } else {
            Err(VmError::GlobalVariableDoesNotExists(ast::Reference::Named(
                name.to_owned(),
            )))
        }
    }

    pub fn set_global_variable_value(
        &mut self,
        name: &str,
        value: Reference,
    ) -> Result<Reference, VmError> {
        if let Some(v) = self.variables.get_mut(name) {
            Ok(std::mem::replace(v, value))
        } else {
            Err(VmError::GlobalVariableDoesNotExists(ast::Reference::Named(
                name.to_owned(),
            )))
        }
    }

    pub fn run_event(&mut self, name: &str, inputs: Vec<Reference>) -> Result<GUID, VmError> {
        if let Some(e) = self.ast.events.iter().find(|e| e.name == name) {
            if e.input_constrains.len() != inputs.len() {
                return Err(VmError::WrongNumberOfInputs(
                    e.input_constrains.len(),
                    inputs.len(),
                ));
            }
            let guid = GUID::default();
            if let Some((_, execution)) = self
                .event_execution_order
                .iter()
                .find(|(k, _)| e.name == **k)
            {
                let vars = e
                    .variables
                    .iter()
                    .map(|v| v.name.clone())
                    .collect::<Vec<_>>();
                self.running_events.insert(
                    guid,
                    VmEvent::new(
                        e.name.clone(),
                        execution.clone(),
                        vars,
                        inputs,
                        e.output_constrains.len(),
                    ),
                );
            } else {
                return Err(VmError::CouldNotRunEvent(name.to_owned()));
            }
            Ok(guid)
        } else {
            Err(VmError::CouldNotRunEvent(name.to_owned()))
        }
    }

    pub fn destroy_event(&mut self, guid: GUID) -> Result<(), VmError> {
        if self.running_events.remove(&guid).is_some() {
            self.completed_events.insert(guid, vec![]);
            Ok(())
        } else {
            Err(VmError::EventDoesNotExists(ast::Reference::Guid(guid)))
        }
    }

    pub fn get_completed_event(&mut self, guid: GUID) -> Option<Vec<Reference>> {
        self.completed_events.remove(&guid)
    }

    pub fn get_completed_events(&mut self) -> impl Iterator<Item = (GUID, Vec<Reference>)> {
        std::mem::take(&mut self.completed_events)
            .into_iter()
            .map(|item| item)
    }

    pub fn process_events(&mut self) -> Result<(), VmError> {
        let count = self.running_events.len();
        let events = std::mem::replace(&mut self.running_events, HashMap::with_capacity(count));
        let mut error = None;
        for (key, mut event) in events {
            if error.is_some() {
                self.running_events.insert(key, event);
            } else {
                match self.process_event(&mut event) {
                    Ok(status) => {
                        if status {
                            self.running_events.insert(key, event);
                        } else {
                            self.completed_events.insert(key, event.outputs);
                        }
                    }
                    Err(err) => error = Some(err),
                }
            }
        }
        if let Some(error) = error {
            Err(error)
        } else {
            Ok(())
        }
    }

    pub fn single_step_event(&mut self, guid: GUID) -> Result<(), VmError> {
        if let Some(mut event) = self.running_events.remove(&guid) {
            self.step_event(&mut event)?;
            self.running_events.insert(guid, event);
            Ok(())
        } else {
            Err(VmError::EventDoesNotExists(ast::Reference::Guid(guid)))
        }
    }

    fn step_event(&mut self, event: &mut VmEvent) -> Result<VmStepStatus, VmError> {
        // TODO: try avoid cloning.
        if let Some(node) = event.get_current_node(self).cloned() {
            match &node.node_type {
                NodeType::Halt => {
                    event.go_to_next_node();
                    return Ok(VmStepStatus::Halt);
                }
                NodeType::Loop(loop_) => {
                    event.push_jump_on_stack(VmJump::Loop(node.id.clone()));
                    event.go_to_node(loop_, self)?;
                }
                NodeType::IfElse(if_else) => {
                    let value = event.get_node_output(&node.input_links[0])?.clone();
                    let value2 = value.clone();
                    if let Ok(v) = value.try_read() {
                        if let Value::Bool(v) = &*v {
                            event.push_jump_on_stack(VmJump::IfElse(node.id.clone()));
                            if *v {
                                event.go_to_node(&if_else.next_node_true, self)?;
                            } else {
                                event.go_to_node(&if_else.next_node_false, self)?;
                            }
                        } else {
                            return Err(VmError::ValueIsNotABool(value2));
                        }
                    } else {
                        return Err(VmError::TryingToReadBorrowedReference(value2));
                    }
                    drop(value);
                }
                NodeType::Break => match event.pop_jump_from_stack()? {
                    VmJump::Loop(id) => {
                        let node = event.get_node(&id, self)?;
                        if let NodeType::Loop(_) = &node.node_type {
                            event.go_to_node(&node.next_node, self)?;
                        } else {
                            return Err(VmError::NodeIsNotALoop(id));
                        }
                    }
                    VmJump::IfElse(_) => {
                        return Err(VmError::TryingToBreakIfElse);
                    }
                    _ => {}
                },
                NodeType::Continue => match event.pop_jump_from_stack()? {
                    VmJump::Loop(id) => {
                        let node = event.get_node(&id, self)?;
                        if let NodeType::Loop(loop_) = &node.node_type {
                            event.go_to_node(&loop_, self)?;
                        } else {
                            return Err(VmError::NodeIsNotALoop(id));
                        }
                    }
                    VmJump::IfElse(_) => {
                        return Err(VmError::TryingToContinueIfElse);
                    }
                    _ => {}
                },
                NodeType::GetInstance => {
                    let value = event.instance_value()?;
                    event.set_node_output(node.id.clone(), value);
                }
                NodeType::GetGlobalVariable(name) => {
                    let value = self.global_variable_value(name)?;
                    event.set_node_output(node.id.clone(), value);
                }
                NodeType::GetLocalVariable(name) => {
                    let value = event.local_variable_value(name)?;
                    event.set_node_output(node.id.clone(), value);
                }
                NodeType::GetInput(index) => {
                    let value = event.input_value(*index)?;
                    event.set_node_output(node.id.clone(), value);
                }
                NodeType::SetOutput(index) => {
                    let value = event.get_node_output(&node.input_links[0])?.clone();
                    event.set_output_value(*index, value)?;
                }
                NodeType::GetValue(value) => {
                    let value: Value = value.data.clone().into();
                    event.set_node_output(node.id.clone(), value.into());
                }
                NodeType::GetListItem(index) => {
                    let value = event.get_node_output(&node.input_links[0])?.clone();
                    let value2 = value.clone();
                    if let Ok(v) = value.try_read() {
                        if let Value::List(list) = &*v {
                            if let Some(value) = list.get(*index) {
                                event.set_node_output(node.id.clone(), value.clone());
                            } else {
                                return Err(VmError::IndexOutOfBounds(list.len(), *index, value2));
                            }
                        } else {
                            return Err(VmError::ValueIsNotAList(value2));
                        }
                    } else {
                        return Err(VmError::TryingToReadBorrowedReference(value2));
                    }
                    drop(value);
                }
                NodeType::GetObjectItem(key) => {
                    let value = event.get_node_output(&node.input_links[0])?.clone();
                    let value2 = value.clone();
                    if let Ok(v) = value.try_read() {
                        if let Value::Object(object) = &*v {
                            if let Some(value) = object.get(key) {
                                event.set_node_output(node.id.clone(), value.clone());
                            } else {
                                return Err(VmError::ObjectKeyDoesNotExists(
                                    key.to_owned(),
                                    value2,
                                ));
                            }
                        } else {
                            return Err(VmError::ValueIsNotAnObject(value2));
                        }
                    } else {
                        return Err(VmError::TryingToReadBorrowedReference(value2));
                    }
                    drop(value);
                }
                NodeType::MutateValue => {
                    let value_dst = event.get_node_output(&node.input_links[0])?;
                    let value_dst2 = value_dst.clone();
                    let value_src = event.get_node_output(&node.input_links[0])?;
                    let value_src2 = value_src.clone();
                    if let Ok(mut dst) = value_dst.try_write() {
                        if let Ok(src) = value_src.try_read() {
                            *dst = src.clone();
                        } else {
                            return Err(VmError::TryingToReadBorrowedReference(value_src2));
                        }
                    } else {
                        return Err(VmError::TryingToWriteBorrowedReference(
                            value_src2, value_dst2,
                        ));
                    }
                }
                NodeType::CallOperation(name) => {
                    if let Some(op) = self.ast.operations.iter().find(|op| &op.name == name) {
                        if let Some(op_impl) = self.operations.get_mut(&op.name) {
                            let inputs = node
                                .input_links
                                .iter()
                                .map(|link| match event.get_node_output(link) {
                                    Ok(v) => Ok(v.clone()),
                                    Err(e) => Err(e),
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            if op.input_constrains.len() != inputs.len() {
                                return Err(VmError::WrongNumberOfInputs(
                                    op.input_constrains.len(),
                                    inputs.len(),
                                ));
                            }
                            let outputs = match op_impl.execute(inputs.as_slice()) {
                                Ok(outputs) => outputs,
                                Err(error) => {
                                    return Err(VmError::Message(format!(
                                        "Error during call to {:?} operation: {:?}",
                                        op.name, error
                                    )))
                                }
                            };
                            if op.output_constrains.len() != outputs.len() {
                                return Err(VmError::WrongNumberOfOutputs(
                                    op.output_constrains.len(),
                                    outputs.len(),
                                ));
                            }
                            event.set_node_outputs(node.id.clone(), outputs);
                        } else {
                            return Err(VmError::OperationIsNotRegistered(op.name.clone()));
                        }
                    } else {
                        return Err(VmError::OperationDoesNotExists(ast::Reference::Named(
                            name.clone(),
                        )));
                    }
                }
                NodeType::CallFunction(name) => {
                    if let Some(function) = self.ast.functions.iter().find(|f| &f.name == name) {
                        if let Some((_, execution)) = self
                            .function_execution_order
                            .iter()
                            .find(|(k, _)| function.name == **k)
                        {
                            let inputs = node
                                .input_links
                                .iter()
                                .map(|link| match event.get_node_output(link) {
                                    Ok(v) => Ok(v.clone()),
                                    Err(e) => Err(e),
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            if function.input_constrains.len() != inputs.len() {
                                return Err(VmError::WrongNumberOfInputs(
                                    function.input_constrains.len(),
                                    inputs.len(),
                                ));
                            }
                            event.contexts.push(VmContext {
                                owner: VmContextOwner::Function(function.name.clone()),
                                caller_node: Some(node.id.clone()),
                                execution: execution.to_vec(),
                                current: Some(0),
                                instance: None,
                                inputs,
                                outputs: vec![Value::None.into(); function.output_constrains.len()],
                                variables: function
                                    .variables
                                    .iter()
                                    .map(|v| (v.name.clone(), Value::None.into()))
                                    .collect::<HashMap<_, _>>(),
                                jump_stack: vec![VmJump::None(None)],
                                node_outputs: Default::default(),
                            });
                        } else {
                            return Err(VmError::CouldNotCallFunction(ast::Reference::Named(
                                name.clone(),
                            )));
                        }
                    } else {
                        return Err(VmError::FunctionDoesNotExists(ast::Reference::Named(
                            name.clone(),
                        )));
                    }
                }
                NodeType::CallMethod(type_name, method_name) => {
                    if let Some(type_) = self.ast.types.iter().find(|t| &t.name == type_name) {
                        let method =
                            type_
                                .traits_implementation
                                .iter()
                                .find_map(|(trait_name, methods)| {
                                    if let Some(method) =
                                        methods.iter().find(|m| &m.name == method_name)
                                    {
                                        Some(method)
                                    } else if let Some(trait_) =
                                        self.ast.traits.iter().find(|t| &t.name == trait_name)
                                    {
                                        trait_.methods.iter().find(|m| &m.name == method_name)
                                    } else {
                                        None
                                    }
                                });
                        let method = if let Some(method) = method {
                            method
                        } else {
                            return Err(VmError::MethodDoesNotExists(ast::Reference::Named(
                                method_name.clone(),
                            )));
                        };
                        if let Some((_, execution)) = self
                            .method_execution_order
                            .iter()
                            .find(|((_, k), _)| method.name == *k)
                        {
                            let inputs = node
                                .input_links
                                .iter()
                                .map(|link| match event.get_node_output(link) {
                                    Ok(v) => Ok(v.clone()),
                                    Err(e) => Err(e),
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            if method.input_constrains.len() != inputs.len() {
                                return Err(VmError::WrongNumberOfInputs(
                                    method.input_constrains.len(),
                                    inputs.len(),
                                ));
                            }
                            let instance =
                                Some(event.get_node_output(&node.input_links[0])?.clone());
                            event.contexts.push(VmContext {
                                owner: VmContextOwner::Method(
                                    type_.name.clone(),
                                    method.name.clone(),
                                ),
                                caller_node: Some(node.id.clone()),
                                execution: execution.to_vec(),
                                current: Some(0),
                                instance,
                                inputs,
                                outputs: vec![Value::None.into(); method.output_constrains.len()],
                                variables: method
                                    .variables
                                    .iter()
                                    .map(|v| (v.name.clone(), Value::None.into()))
                                    .collect::<HashMap<_, _>>(),
                                jump_stack: vec![VmJump::None(None)],
                                node_outputs: Default::default(),
                            });
                        } else {
                            return Err(VmError::CouldNotCallMethod(
                                ast::Reference::Named(type_name.clone()),
                                ast::Reference::Named(method_name.clone()),
                            ));
                        }
                    } else {
                        return Err(VmError::TypeDoesNotExists(ast::Reference::Named(
                            type_name.clone(),
                        )));
                    }
                }
                NodeType::Entry => {}
                _ => {
                    return Err(VmError::TryingToPerformInvalidNodeType(
                        node.node_type.clone(),
                    ))
                }
            }
            if self.end_nodes.contains(&node.id) {
                match event.pop_jump_from_stack()? {
                    VmJump::Loop(id) => {
                        let node = event.get_node(&id, self)?;
                        if let NodeType::Loop(_) = &node.node_type {
                            event.go_to_node(&id, self)?;
                        } else {
                            return Err(VmError::NodeIsNotALoop(id));
                        }
                    }
                    VmJump::IfElse(id) => {
                        let node = event.get_node(&id, self)?;
                        if let NodeType::IfElse(_) = &node.node_type {
                            event.go_to_node(&node.next_node, self)?;
                        } else {
                            return Err(VmError::NodeIsNotAnIfElse(id));
                        }
                    }
                    _ => {}
                }
                if event.is_jump_stack_empty() {
                    event.go_to_next_node();
                    return Ok(VmStepStatus::Stop);
                }
            }
            event.go_to_next_node();
            Ok(VmStepStatus::Continue)
        } else {
            Ok(VmStepStatus::Stop)
        }
    }

    fn process_event(&mut self, event: &mut VmEvent) -> Result<bool, VmError> {
        loop {
            match self.step_event(event)? {
                VmStepStatus::Continue => continue,
                VmStepStatus::Halt => return Ok(true),
                VmStepStatus::Stop => break,
            }
        }
        Ok(false)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum VmStepStatus {
    Continue,
    Halt,
    Stop,
}

pub trait VmOperation: Send + Sync {
    fn execute(&mut self, inputs: &[Reference]) -> Result<Vec<Reference>, VmOperationError>;
}

#[derive(Debug, Clone)]
enum VmContextOwner {
    Event(String),
    // (type name, method name)
    Method(String, String),
    Function(String),
}

#[derive(Debug, Clone)]
enum VmJump {
    /// calling node reference?
    None(Option<ast::Reference>),
    /// loop node reference
    Loop(ast::Reference),
    /// if-else node reference
    IfElse(ast::Reference),
}

#[derive(Debug, Clone)]
struct VmContext {
    pub owner: VmContextOwner,
    pub caller_node: Option<ast::Reference>,
    pub execution: Vec<ast::Reference>,
    pub current: Option<usize>,
    pub instance: Option<Reference>,
    pub inputs: Vec<Reference>,
    pub outputs: Vec<Reference>,
    pub variables: HashMap<String, Reference>,
    pub jump_stack: Vec<VmJump>,
    pub node_outputs: HashMap<ast::Reference, Vec<Reference>>,
}

#[derive(Debug, Clone)]
struct VmEvent {
    pub contexts: Vec<VmContext>,
    pub outputs: Vec<Reference>,
}

impl VmEvent {
    pub fn new(
        owner_event: String,
        execution: Vec<ast::Reference>,
        variables: Vec<String>,
        inputs: Vec<Reference>,
        outputs: usize,
    ) -> Self {
        Self {
            contexts: vec![VmContext {
                owner: VmContextOwner::Event(owner_event),
                caller_node: None,
                execution,
                current: Some(0),
                instance: None,
                inputs,
                outputs: vec![Value::None.into(); outputs],
                variables: variables
                    .into_iter()
                    .map(|g| (g, Value::None.into()))
                    .collect::<HashMap<_, _>>(),
                jump_stack: vec![VmJump::None(None)],
                node_outputs: Default::default(),
            }],
            outputs: vec![],
        }
    }

    fn get_node_outputs(&self, id: &ast::Reference) -> Result<&[Reference], VmError> {
        if let Some(context) = self.contexts.last() {
            if let Some(outputs) = context.node_outputs.get(id) {
                return Ok(outputs);
            }
        }
        Err(VmError::ThereAreNoCachedNodeOutputs(id.clone()))
    }

    fn get_node_output(&self, link: &Link) -> Result<&Reference, VmError> {
        if let Link::NodeIndexed(id, index) = link {
            if let Some(output) = self.get_node_outputs(id)?.get(*index) {
                return Ok(output);
            }
        }
        Err(VmError::ThereIsNoCachedNodeIndexedOutput(link.clone()))
    }

    fn set_node_outputs(&mut self, id: ast::Reference, values: Vec<Reference>) {
        if let Some(context) = self.contexts.last_mut() {
            context.node_outputs.insert(id, values);
        }
    }

    fn set_node_output(&mut self, id: ast::Reference, value: Reference) {
        self.set_node_outputs(id, vec![value]);
    }

    fn get_current_node<'a>(&self, vm: &'a Vm) -> Option<&'a Node> {
        if let Some(context) = self.contexts.last() {
            if let Some(current) = context.current {
                if let Some(node_ref) = context.execution.get(current) {
                    if let Ok(node) = self.get_node(&node_ref, vm) {
                        return Some(node);
                    }
                }
            }
        }
        None
    }

    fn get_node<'a>(&self, reference: &ast::Reference, vm: &'a Vm) -> Result<&'a Node, VmError> {
        if let Some(context) = self.contexts.last() {
            match &context.owner {
                VmContextOwner::Event(event_name) => {
                    if let Some(event) = vm.ast.events.iter().find(|e| &e.name == event_name) {
                        if let Some(node) = event.nodes.iter().find(|n| &n.id == reference) {
                            return Ok(node);
                        }
                    } else {
                        return Err(VmError::EventDoesNotExists(ast::Reference::Named(
                            event_name.clone(),
                        )));
                    }
                }
                VmContextOwner::Method(type_name, method_name) => {
                    if let Some(methods) = vm.type_methods.get(type_name) {
                        if let Some((trait_name, is_impl)) = methods.get(method_name) {
                            let type_ = if let Some(type_) =
                                vm.ast.types.iter().find(|t| &t.name == type_name)
                            {
                                type_
                            } else {
                                return Err(VmError::TypeDoesNotExists(ast::Reference::Named(
                                    type_name.clone(),
                                )));
                            };
                            if *is_impl {
                                if let Some(method) =
                                    type_.traits_implementation.iter().find_map(|(_, methods)| {
                                        methods.iter().find(|m| &m.name == method_name)
                                    })
                                {
                                    if let Some(node) =
                                        method.nodes.iter().find(|n| &n.id == reference)
                                    {
                                        return Ok(node);
                                    }
                                } else {
                                    return Err(VmError::MethodDoesNotExists(
                                        ast::Reference::Named(method_name.clone()),
                                    ));
                                }
                            } else if let Some(trait_) =
                                vm.ast.traits.iter().find(|t| &t.name == trait_name)
                            {
                                if let Some(method) =
                                    trait_.methods.iter().find(|m| &m.name == method_name)
                                {
                                    if let Some(node) =
                                        method.nodes.iter().find(|n| &n.id == reference)
                                    {
                                        return Ok(node);
                                    }
                                } else {
                                    return Err(VmError::MethodDoesNotExists(
                                        ast::Reference::Named(method_name.clone()),
                                    ));
                                }
                            } else {
                                return Err(VmError::TraitDoesNotExists(ast::Reference::Named(
                                    trait_name.clone(),
                                )));
                            }
                        } else {
                            return Err(VmError::TypeDoesNotImplementMethod(
                                ast::Reference::Named(type_name.clone()),
                                ast::Reference::Named(method_name.clone()),
                            ));
                        }
                    } else {
                        return Err(VmError::NodeDoesNotExists(ast::Reference::Named(
                            type_name.clone(),
                        )));
                    }
                }
                VmContextOwner::Function(function_name) => {
                    if let Some(function) =
                        vm.ast.functions.iter().find(|f| &f.name == function_name)
                    {
                        if let Some(node) = function.nodes.iter().find(|n| &n.id == reference) {
                            return Ok(node);
                        }
                    } else {
                        return Err(VmError::FunctionDoesNotExists(ast::Reference::Named(
                            function_name.clone(),
                        )));
                    }
                }
            }
        }
        Err(VmError::NodeDoesNotExists(reference.clone()))
    }

    fn go_to_node(&mut self, reference: &ast::Reference, vm: &Vm) -> Result<(), VmError> {
        let id = self.get_node(reference, vm)?.id.clone();
        if let Some(context) = self.contexts.last() {
            if let Some(index) = context.execution.iter().position(|n| n == &id) {
                self.contexts.last_mut().unwrap().current = Some(index);
                return Ok(());
            }
        }
        Err(VmError::NodeNotFoundInExecutionPipeline(reference.clone()))
    }

    fn go_to_next_node(&mut self) {
        if let Some(context) = self.contexts.last() {
            if let Some(mut current) = context.current {
                current += 1;
                if current < context.execution.len() {
                    self.contexts.last_mut().unwrap().current = Some(current);
                } else {
                    let context = self.contexts.pop();
                    self.go_to_next_node();
                    if let Some(context) = context {
                        if let Some(caller) = context.caller_node {
                            self.set_node_outputs(caller, context.outputs);
                        } else {
                            self.outputs = context.outputs;
                        }
                    }
                }
            }
        }
    }

    fn push_jump_on_stack(&mut self, jump: VmJump) {
        if let Some(context) = self.contexts.last_mut() {
            context.jump_stack.push(jump);
        }
    }

    fn pop_jump_from_stack(&mut self) -> Result<VmJump, VmError> {
        if let Some(context) = self.contexts.last_mut() {
            if let Some(jump) = context.jump_stack.pop() {
                return Ok(jump);
            }
        }
        Err(VmError::StackUnderflow)
    }

    fn is_jump_stack_empty(&self) -> bool {
        if let Some(context) = self.contexts.last() {
            context.jump_stack.is_empty()
        } else {
            true
        }
    }

    fn instance_value(&self) -> Result<Reference, VmError> {
        if let Some(context) = self.contexts.last() {
            if let Some(instance) = &context.instance {
                return Ok(instance.clone());
            }
        }
        Err(VmError::InstanceDoesNotExists)
    }

    fn local_variable_value(&self, name: &str) -> Result<Reference, VmError> {
        if let Some(context) = self.contexts.last() {
            if let Some(value) = context.variables.get(name) {
                return Ok(value.clone());
            }
        }
        Err(VmError::LocalVariableDoesNotExists(ast::Reference::Named(
            name.to_owned(),
        )))
    }

    fn input_value(&self, index: usize) -> Result<Reference, VmError> {
        if let Some(context) = self.contexts.last() {
            if let Some(input) = context.inputs.get(index) {
                return Ok(input.clone());
            }
        }
        Err(VmError::InputDoesNotExists(index))
    }

    fn set_output_value(&mut self, index: usize, value: Reference) -> Result<Reference, VmError> {
        if let Some(context) = self.contexts.last_mut() {
            if let Some(output) = context.outputs.get_mut(index) {
                return Ok(std::mem::replace(output, value));
            }
        }
        Err(VmError::OutputDoesNotExists(index))
    }
}
