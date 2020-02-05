use crate::GUID;
use core::prefab::{Prefab, PrefabValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reference {
    None,
    Named(String),
    Guid(GUID),
}

impl Default for Reference {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Program {
    pub version: (usize, usize, usize),
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub types: Vec<Type>,
    #[serde(default)]
    pub traits: Vec<Trait>,
    #[serde(default)]
    pub functions: Vec<Function>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub variables: Vec<Variable>,
    #[serde(default)]
    pub operations: Vec<Operation>,
}

impl Prefab for Program {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    #[serde(default)]
    pub input_constrains: Vec<TypeConstraint>,
    #[serde(default)]
    pub output_constrains: Vec<TypeConstraint>,
    #[serde(default)]
    pub variables: Vec<Variable>,
    pub entry_node: Reference,
    pub nodes: Vec<Node>,
}

impl Prefab for Event {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub type_name: String,
}

impl Prefab for Variable {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Type {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub traits_implementation: HashMap<String, Vec<Method>>,
    #[serde(default)]
    pub export: bool,
}

impl Prefab for Type {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub type_name: String,
    #[serde(default)]
    pub public: bool,
}

impl Prefab for Field {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Trait {
    pub name: String,
    #[serde(default)]
    pub methods: Vec<Method>,
    #[serde(default)]
    pub export: bool,
}

impl Prefab for Trait {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Method {
    pub name: String,
    pub owner_trait: Reference,
    #[serde(default)]
    pub input_constrains: Vec<TypeConstraint>,
    #[serde(default)]
    pub output_constrains: Vec<TypeConstraint>,
    #[serde(default)]
    pub variables: Vec<Variable>,
    #[serde(default)]
    pub associated: bool,
    pub entry_node: Reference,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub help: String,
}

impl Prefab for Method {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeConstraint {
    Any,
    Type(Reference),
    ImplementTraits(Vec<Reference>),
    Node,
}

impl Prefab for TypeConstraint {}

impl Default for TypeConstraint {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub name: String,
    #[serde(default)]
    pub input_constrains: Vec<TypeConstraint>,
    #[serde(default)]
    pub output_constrains: Vec<TypeConstraint>,
    #[serde(default)]
    pub help: String,
}

impl Prefab for Operation {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    #[serde(default)]
    pub input_constrains: Vec<TypeConstraint>,
    #[serde(default)]
    pub output_constrains: Vec<TypeConstraint>,
    #[serde(default)]
    pub variables: Vec<Variable>,
    pub entry_node: Reference,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub help: String,
}

impl Prefab for Function {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Link {
    None,
    NodeIndexed(Reference, usize),
}

impl Prefab for Link {}

impl Default for Link {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Reference,
    pub node_type: NodeType,
    #[serde(default)]
    pub next_node: Reference,
    #[serde(default)]
    pub input_links: Vec<Link>,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
}

impl Prefab for Node {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IfElse {
    #[serde(default)]
    pub next_node_true: Reference,
    #[serde(default)]
    pub next_node_false: Reference,
}

impl Prefab for IfElse {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    None,
    Halt,
    /// body entry node
    Loop(Reference),
    IfElse(IfElse),
    Break,
    Continue,
    GetInstance,
    GetGlobalVariable(String),
    GetLocalVariable(String),
    GetInput(usize),
    SetOutput(usize),
    GetValue(Value),
    GetListItem(usize),
    GetObjectItem(String),
    MutateValue,
    CallOperation(String),
    CallFunction(String),
    /// (type name, method name)
    CallMethod(String, String),
}

impl NodeType {
    pub fn is_input_output(&self) -> (bool, bool) {
        match self {
            Self::Halt
            | Self::GetListItem(_)
            | Self::GetObjectItem(_)
            | Self::CallOperation(_)
            | Self::CallFunction(_)
            | Self::IfElse(_)
            | Self::Loop(_)
            | Self::CallMethod(_, _) => (true, true),
            Self::GetInstance
            | Self::GetGlobalVariable(_)
            | Self::GetLocalVariable(_)
            | Self::GetInput(_)
            | Self::GetValue(_) => (false, true),
            Self::SetOutput(_) | Self::MutateValue | Self::Break | Self::Continue => (true, false),
            Self::None => (false, false),
        }
    }
}

impl Prefab for NodeType {}

impl Default for NodeType {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Value {
    pub type_name: String,
    pub data: PrefabValue,
}

impl Prefab for Value {}