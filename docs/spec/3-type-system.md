# Part 3: Type System

## 3.1 Overview

Colony has a gradual type system:
- Variables can be untyped (dynamic) or annotated with types
- Pipeline types are always inferred or declared
- Type errors can be warnings (default) or errors (strict mode)

## 3.2 Runtime Values

```rust
// crates/colony-core/src/types.rs

use indexmap::{IndexMap, IndexSet};
use std::sync::Arc;

/// Runtime value representation
#[derive(Clone, Debug)]
pub enum Value {
    // === Primitives ===
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    
    // === Compound Types ===
    List(Vec<Value>),
    Dict(IndexMap<String, Value>),
    Range(Box<RangeValue>),
    Tuple(Vec<Value>),
    Set(IndexSet<ValueKey>),
    
    // === Callable Types ===
    Command(CommandBlock),
    Proc(Arc<ProcDef>),
    Func(Arc<FuncDef>),
    Agent(Arc<AgentDef>),
    Builtin(BuiltinFn),
    
    // === Special Types ===
    Stream(Box<dyn ValueStream>),
    Table(Table),
    Obj(Obj),
    
    // === Pipeline Types ===
    Typed(Box<Value>, PipelineType),
}

/// Range with start, end, inclusive flag
#[derive(Clone, Debug, PartialEq)]
pub struct RangeValue {
    pub start: i64,
    pub end: i64,
    pub inclusive: bool,
    pub step: i64,
}

impl RangeValue {
    pub fn new(start: i64, end: i64, inclusive: bool) -> Self {
        Self { start, end, inclusive, step: 1 }
    }
    
    pub fn with_step(mut self, step: i64) -> Self {
        self.step = step;
        self
    }
    
    pub fn iter(&self) -> impl Iterator<Item = i64> {
        let start = self.start;
        let end = if self.inclusive { self.end + 1 } else { self.end };
        let step = self.step;
        
        (0..).map(move |i| start + i * step)
            .take_while(move |&n| if step > 0 { n < end } else { n > end })
    }
}

/// Wrapper for hashable values (for Set)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueKey {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Bytes(Vec<u8>),
    Tuple(Vec<ValueKey>),
}

impl TryFrom<&Value> for ValueKey {
    type Error = TypeError;
    
    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v {
            Value::Null => Ok(ValueKey::Null),
            Value::Bool(b) => Ok(ValueKey::Bool(*b)),
            Value::Int(n) => Ok(ValueKey::Int(*n)),
            Value::Str(s) => Ok(ValueKey::Str(s.clone())),
            Value::Bytes(b) => Ok(ValueKey::Bytes(b.clone())),
            Value::Tuple(items) => {
                let keys: Result<Vec<_>, _> = items.iter()
                    .map(ValueKey::try_from)
                    .collect();
                Ok(ValueKey::Tuple(keys?))
            }
            _ => Err(TypeError::NotHashable(v.type_name())),
        }
    }
}
```

## 3.3 Value Methods

```rust
impl Value {
    /// Get type name for error messages
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "Null",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Str(_) => "Str",
            Value::Bytes(_) => "Bytes",
            Value::List(_) => "List",
            Value::Dict(_) => "Dict",
            Value::Range(_) => "Range",
            Value::Tuple(_) => "Tuple",
            Value::Set(_) => "Set",
            Value::Command(_) => "Command",
            Value::Proc(_) => "Proc",
            Value::Func(_) => "Func",
            Value::Agent(_) => "Agent",
            Value::Builtin(_) => "Builtin",
            Value::Stream(_) => "Stream",
            Value::Table(_) => "Table",
            Value::Obj(_) => "Obj",
            Value::Typed(_, _) => "Typed",
        }
    }
    
    /// Truthiness for conditionals
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0 && !f.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::Bytes(b) => !b.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Dict(d) => !d.is_empty(),
            Value::Set(s) => !s.is_empty(),
            Value::Range(r) => r.start != r.end || r.inclusive,
            _ => true,
        }
    }
    
    /// Convert to string representation
    pub fn to_string(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => {
                if f.is_nan() { "NaN".to_string() }
                else if f.is_infinite() { 
                    if *f > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
                }
                else { f.to_string() }
            }
            Value::Str(s) => s.clone(),
            Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
            Value::List(l) => {
                let items: Vec<_> = l.iter().map(|v| v.to_repr()).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Dict(d) => {
                let items: Vec<_> = d.iter()
                    .map(|(k, v)| format!("{}: {}", k, v.to_repr()))
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
            Value::Tuple(t) => {
                let items: Vec<_> = t.iter().map(|v| v.to_repr()).collect();
                format!("({})", items.join(", "))
            }
            _ => format!("<{}>", self.type_name()),
        }
    }
    
    /// Convert to repr (quoted strings, etc.)
    pub fn to_repr(&self) -> String {
        match self {
            Value::Str(s) => format!("\"{}\"", s.escape_default()),
            Value::Bytes(b) => format!("b\"{}\"", String::from_utf8_lossy(b).escape_default()),
            _ => self.to_string(),
        }
    }
    
    /// Convert to JSON-compatible value
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Int(n) => serde_json::Value::Number((*n).into()),
            Value::Float(f) => {
                serde_json::Number::from_f64(*f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
            Value::Str(s) => serde_json::Value::String(s.clone()),
            Value::List(l) => {
                serde_json::Value::Array(l.iter().map(|v| v.to_json()).collect())
            }
            Value::Dict(d) => {
                let map: serde_json::Map<_, _> = d.iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect();
                serde_json::Value::Object(map)
            }
            Value::Tuple(t) => {
                serde_json::Value::Array(t.iter().map(|v| v.to_json()).collect())
            }
            _ => serde_json::Value::String(self.to_string()),
        }
    }
}
```

## 3.4 Table Type

```rust
/// Structured table (like Nushell)
#[derive(Clone, Debug)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    pub fn new(columns: Vec<String>) -> Self {
        Self { columns, rows: Vec::new() }
    }
    
    pub fn with_rows(columns: Vec<String>, rows: Vec<Vec<Value>>) -> Self {
        Self { columns, rows }
    }
    
    pub fn add_row(&mut self, row: Vec<Value>) {
        assert_eq!(row.len(), self.columns.len());
        self.rows.push(row);
    }
    
    pub fn get(&self, row: usize, col: &str) -> Option<&Value> {
        let col_idx = self.columns.iter().position(|c| c == col)?;
        self.rows.get(row)?.get(col_idx)
    }
    
    pub fn column(&self, name: &str) -> Option<Vec<&Value>> {
        let idx = self.columns.iter().position(|c| c == name)?;
        Some(self.rows.iter().map(|r| &r[idx]).collect())
    }
    
    pub fn iter_rows(&self) -> impl Iterator<Item = IndexMap<&str, &Value>> + '_ {
        self.rows.iter().map(|row| {
            self.columns.iter()
                .zip(row.iter())
                .map(|(k, v)| (k.as_str(), v))
                .collect()
        })
    }
    
    pub fn to_json(&self) -> serde_json::Value {
        let rows: Vec<_> = self.iter_rows()
            .map(|row| {
                let obj: serde_json::Map<_, _> = row.into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_json()))
                    .collect();
                serde_json::Value::Object(obj)
            })
            .collect();
        serde_json::Value::Array(rows)
    }
}
```

## 3.5 Object Type

```rust
/// Object with prototype chain
#[derive(Clone, Debug)]
pub struct Obj {
    pub data: IndexMap<String, Value>,
    pub prototype: Option<Arc<Obj>>,
}

impl Obj {
    pub fn new() -> Self {
        Self { data: IndexMap::new(), prototype: None }
    }
    
    pub fn with_prototype(prototype: Arc<Obj>) -> Self {
        Self { data: IndexMap::new(), prototype: Some(prototype) }
    }
    
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key).or_else(|| {
            self.prototype.as_ref().and_then(|p| p.get(key))
        })
    }
    
    pub fn set(&mut self, key: String, value: Value) {
        self.data.insert(key, value);
    }
    
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        // Own keys only, not prototype
        self.data.keys().map(|s| s.as_str())
    }
    
    pub fn all_keys(&self) -> Vec<&str> {
        let mut keys: Vec<_> = self.data.keys().map(|s| s.as_str()).collect();
        if let Some(ref proto) = self.prototype {
            for k in proto.all_keys() {
                if !keys.contains(&k) {
                    keys.push(k);
                }
            }
        }
        keys
    }
}
```

## 3.6 Pipeline Types

Pipeline types annotate data flowing through pipelines:

```rust
/// Type annotation for pipeline data
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PipelineType {
    /// Any type (no constraint)
    Any,
    
    // === Text Types ===
    /// Plain text/string
    Str,
    /// JSON data
    Json,
    /// JSON array specifically
    JsonArray,
    /// JSON object specifically
    JsonObject,
    /// YAML data
    Yaml,
    /// TOML data
    Toml,
    /// CSV data
    Csv,
    /// TSV data
    Tsv,
    /// XML data
    Xml,
    /// HTML data
    Html,
    /// Markdown
    Markdown,
    
    // === Primitive Types ===
    Int,
    Float,
    Bool,
    Null,
    
    // === Binary Types ===
    Binary,
    Image,
    
    // === Structured Types ===
    /// File path
    Path,
    /// Table data (columns + rows)
    Table,
    /// List of values
    List,
    /// Key-value pairs
    Dict,
    
    // === Custom Types ===
    Custom(String),
}

impl PipelineType {
    /// Parse from string annotation
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "*" | "any" => Self::Any,
            "str" | "string" | "text" => Self::Str,
            "json" => Self::Json,
            "json:array" | "jsonarray" => Self::JsonArray,
            "json:object" | "jsonobject" => Self::JsonObject,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            "csv" => Self::Csv,
            "tsv" => Self::Tsv,
            "xml" => Self::Xml,
            "html" => Self::Html,
            "markdown" | "md" => Self::Markdown,
            "int" | "integer" => Self::Int,
            "float" | "number" => Self::Float,
            "bool" | "boolean" => Self::Bool,
            "null" | "none" => Self::Null,
            "binary" | "bytes" => Self::Binary,
            "image" => Self::Image,
            "path" => Self::Path,
            "table" => Self::Table,
            "list" | "array" => Self::List,
            "dict" | "object" | "map" => Self::Dict,
            other => Self::Custom(other.to_string()),
        }
    }
    
    /// Infer from HTTP Content-Type header
    pub fn from_content_type(ct: &str) -> Self {
        let ct = ct.split(';').next().unwrap_or(ct).trim();
        match ct {
            "application/json" => Self::Json,
            "application/yaml" | "text/yaml" | "text/x-yaml" => Self::Yaml,
            "application/toml" | "text/toml" => Self::Toml,
            "text/csv" => Self::Csv,
            "text/tab-separated-values" => Self::Tsv,
            "application/xml" | "text/xml" => Self::Xml,
            "text/html" => Self::Html,
            "text/markdown" => Self::Markdown,
            "application/octet-stream" => Self::Binary,
            ct if ct.starts_with("image/") => Self::Image,
            ct if ct.starts_with("text/") => Self::Str,
            _ => Self::Any,
        }
    }
    
    /// Infer from file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "json" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            "csv" => Self::Csv,
            "tsv" | "tab" => Self::Tsv,
            "xml" => Self::Xml,
            "html" | "htm" => Self::Html,
            "md" | "markdown" => Self::Markdown,
            "txt" => Self::Str,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => Self::Image,
            _ => Self::Any,
        }
    }
    
    /// Check if types are compatible for piping
    pub fn is_compatible(&self, other: &Self) -> bool {
        if *self == Self::Any || *other == Self::Any {
            return true;
        }
        
        // Exact match
        if self == other {
            return true;
        }
        
        // JSON subtypes
        match (self, other) {
            (Self::Json, Self::JsonArray) | (Self::Json, Self::JsonObject) => true,
            (Self::JsonArray, Self::Json) | (Self::JsonObject, Self::Json) => true,
            (Self::JsonArray, Self::List) | (Self::List, Self::JsonArray) => true,
            (Self::JsonObject, Self::Dict) | (Self::Dict, Self::JsonObject) => true,
            _ => false,
        }
    }
    
    /// Get MIME type
    pub fn to_content_type(&self) -> &'static str {
        match self {
            Self::Json | Self::JsonArray | Self::JsonObject => "application/json",
            Self::Yaml => "application/yaml",
            Self::Toml => "application/toml",
            Self::Csv => "text/csv",
            Self::Tsv => "text/tab-separated-values",
            Self::Xml => "application/xml",
            Self::Html => "text/html",
            Self::Markdown => "text/markdown",
            Self::Binary => "application/octet-stream",
            Self::Image => "image/png",
            _ => "text/plain",
        }
    }
}
```

## 3.7 Type Annotations

Type annotations for variables, parameters, and return types:

```rust
/// Static type annotation
#[derive(Clone, Debug, PartialEq)]
pub enum TypeAnnotation {
    /// Simple type name: Int, Str, etc.
    Simple(String),
    
    /// Generic type: List[Int], Dict[Str, Int]
    Generic(String, Vec<TypeAnnotation>),
    
    /// Union type: Int | Str
    Union(Vec<TypeAnnotation>),
    
    /// Optional type: Str? (same as Str | Null)
    Optional(Box<TypeAnnotation>),
    
    /// Tuple type: (Int, Str, Bool)
    Tuple(Vec<TypeAnnotation>),
    
    /// Function type: (Int, Int) -> Int
    Function {
        params: Vec<TypeAnnotation>,
        return_type: Box<TypeAnnotation>,
    },
    
    /// Pipeline type: [json -> table]
    Pipeline {
        input: PipelineType,
        output: PipelineType,
    },
}

impl TypeAnnotation {
    /// Check if a value matches this type annotation
    pub fn check(&self, value: &Value) -> bool {
        match self {
            TypeAnnotation::Simple(name) => {
                match name.as_str() {
                    "Any" => true,
                    "Int" => matches!(value, Value::Int(_)),
                    "Float" => matches!(value, Value::Float(_) | Value::Int(_)),
                    "Str" => matches!(value, Value::Str(_)),
                    "Bool" => matches!(value, Value::Bool(_)),
                    "Null" => matches!(value, Value::Null),
                    "Bytes" => matches!(value, Value::Bytes(_)),
                    "List" => matches!(value, Value::List(_)),
                    "Dict" => matches!(value, Value::Dict(_)),
                    "Table" => matches!(value, Value::Table(_)),
                    "Range" => matches!(value, Value::Range(_)),
                    "Tuple" => matches!(value, Value::Tuple(_)),
                    "Set" => matches!(value, Value::Set(_)),
                    "Proc" => matches!(value, Value::Proc(_)),
                    "Func" => matches!(value, Value::Func(_)),
                    "Agent" => matches!(value, Value::Agent(_)),
                    "Command" => matches!(value, Value::Command(_)),
                    _ => false,
                }
            }
            
            TypeAnnotation::Generic(name, params) => {
                match (name.as_str(), value) {
                    ("List", Value::List(items)) => {
                        if let Some(item_type) = params.first() {
                            items.iter().all(|v| item_type.check(v))
                        } else {
                            true
                        }
                    }
                    ("Dict", Value::Dict(map)) => {
                        let value_type = params.get(1).or(params.first());
                        if let Some(vt) = value_type {
                            map.values().all(|v| vt.check(v))
                        } else {
                            true
                        }
                    }
                    ("Set", Value::Set(_)) => true,
                    _ => false,
                }
            }
            
            TypeAnnotation::Union(types) => {
                types.iter().any(|t| t.check(value))
            }
            
            TypeAnnotation::Optional(inner) => {
                matches!(value, Value::Null) || inner.check(value)
            }
            
            TypeAnnotation::Tuple(types) => {
                if let Value::Tuple(items) = value {
                    if items.len() != types.len() {
                        return false;
                    }
                    items.iter().zip(types).all(|(v, t)| t.check(v))
                } else {
                    false
                }
            }
            
            TypeAnnotation::Function { .. } => {
                matches!(value, Value::Proc(_) | Value::Func(_) | Value::Builtin(_))
            }
            
            TypeAnnotation::Pipeline { .. } => {
                matches!(value, Value::Proc(_))
            }
        }
    }
    
    /// Parse from string
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        // Simple implementation - full parser would be more complex
        let s = s.trim();
        
        // Optional: Type?
        if s.ends_with('?') {
            let inner = Self::parse(&s[..s.len()-1])?;
            return Ok(TypeAnnotation::Optional(Box::new(inner)));
        }
        
        // Union: A | B
        if s.contains('|') {
            let types: Result<Vec<_>, _> = s.split('|')
                .map(|p| Self::parse(p.trim()))
                .collect();
            return Ok(TypeAnnotation::Union(types?));
        }
        
        // Generic: Name[Params]
        if let Some(bracket_start) = s.find('[') {
            if s.ends_with(']') {
                let name = &s[..bracket_start];
                let params_str = &s[bracket_start+1..s.len()-1];
                let params: Result<Vec<_>, _> = params_str.split(',')
                    .map(|p| Self::parse(p.trim()))
                    .collect();
                return Ok(TypeAnnotation::Generic(name.to_string(), params?));
            }
        }
        
        // Tuple: (A, B, C)
        if s.starts_with('(') && s.ends_with(')') {
            let inner = &s[1..s.len()-1];
            let types: Result<Vec<_>, _> = inner.split(',')
                .map(|p| Self::parse(p.trim()))
                .collect();
            return Ok(TypeAnnotation::Tuple(types?));
        }
        
        // Simple type
        Ok(TypeAnnotation::Simple(s.to_string()))
    }
}
```

## 3.8 Type Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum TypeError {
    #[error("Expected {expected}, got {actual}")]
    Mismatch {
        expected: String,
        actual: String,
    },
    
    #[error("Cannot index {target} with {index}")]
    InvalidIndex {
        target: String,
        index: String,
    },
    
    #[error("Field '{field}' not found on {target}")]
    FieldNotFound {
        field: String,
        target: String,
    },
    
    #[error("Cannot call non-callable value of type {0}")]
    NotCallable(String),
    
    #[error("Cannot iterate over value of type {0}")]
    NotIterable(String),
    
    #[error("Value of type {0} is not hashable")]
    NotHashable(String),
    
    #[error("Cannot convert {from} to {to}")]
    ConversionError {
        from: String,
        to: String,
    },
    
    #[error("Pipeline type mismatch: expected {expected}, got {actual}")]
    PipelineMismatch {
        expected: PipelineType,
        actual: PipelineType,
    },
}
```

## 3.9 Type Inference

```rust
/// Infer the pipeline type of a value
pub fn infer_pipeline_type(value: &Value) -> PipelineType {
    match value {
        Value::Null => PipelineType::Null,
        Value::Bool(_) => PipelineType::Bool,
        Value::Int(_) => PipelineType::Int,
        Value::Float(_) => PipelineType::Float,
        Value::Str(s) => infer_string_type(s),
        Value::Bytes(_) => PipelineType::Binary,
        Value::List(_) => PipelineType::List,
        Value::Dict(_) => PipelineType::Dict,
        Value::Table(_) => PipelineType::Table,
        Value::Typed(_, t) => t.clone(),
        _ => PipelineType::Any,
    }
}

/// Try to infer type from string content
fn infer_string_type(s: &str) -> PipelineType {
    let trimmed = s.trim();
    
    // Try JSON
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
       || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
            return if trimmed.starts_with('[') {
                PipelineType::JsonArray
            } else {
                PipelineType::JsonObject
            };
        }
    }
    
    // Try YAML (if has indicators)
    if trimmed.contains(':') && !trimmed.contains('{') {
        if serde_yaml::from_str::<serde_yaml::Value>(trimmed).is_ok() {
            return PipelineType::Yaml;
        }
    }
    
    // Check for path-like strings
    if trimmed.starts_with('/') || trimmed.starts_with("./") || trimmed.starts_with("~/") {
        return PipelineType::Path;
    }
    
    PipelineType::Str
}
```

## 3.10 Type Coercion

```rust
/// Coerce value to target type
pub fn coerce(value: Value, target: &PipelineType) -> Result<Value, TypeError> {
    let source = infer_pipeline_type(&value);
    
    if source == *target || *target == PipelineType::Any {
        return Ok(value);
    }
    
    match (value, target) {
        // Int -> Float
        (Value::Int(n), PipelineType::Float) => {
            Ok(Value::Float(n as f64))
        }
        
        // Float -> Int (truncate)
        (Value::Float(f), PipelineType::Int) => {
            Ok(Value::Int(f as i64))
        }
        
        // String -> JSON
        (Value::Str(s), PipelineType::Json | PipelineType::JsonArray | PipelineType::JsonObject) => {
            let json: serde_json::Value = serde_json::from_str(&s)
                .map_err(|_| TypeError::ConversionError {
                    from: "Str".to_string(),
                    to: "Json".to_string(),
                })?;
            Ok(json_to_value(json))
        }
        
        // JSON -> String
        (v, PipelineType::Str) if matches!(infer_pipeline_type(&v), PipelineType::Json | PipelineType::JsonArray | PipelineType::JsonObject) => {
            Ok(Value::Str(serde_json::to_string(&v.to_json()).unwrap()))
        }
        
        // List -> Table (if list of dicts)
        (Value::List(items), PipelineType::Table) => {
            list_to_table(items)
        }
        
        // Table -> List
        (Value::Table(t), PipelineType::List) => {
            let rows: Vec<Value> = t.iter_rows()
                .map(|row| {
                    let dict: IndexMap<String, Value> = row.into_iter()
                        .map(|(k, v)| (k.to_string(), v.clone()))
                        .collect();
                    Value::Dict(dict)
                })
                .collect();
            Ok(Value::List(rows))
        }
        
        // Any -> Str
        (v, PipelineType::Str) => {
            Ok(Value::Str(v.to_string()))
        }
        
        (v, target) => {
            Err(TypeError::ConversionError {
                from: v.type_name().to_string(),
                to: format!("{:?}", target),
            })
        }
    }
}

/// Convert JSON to Colony Value
pub fn json_to_value(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(arr) => {
            Value::List(arr.into_iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let map: IndexMap<String, Value> = obj.into_iter()
                .map(|(k, v)| (k, json_to_value(v)))
                .collect();
            Value::Dict(map)
        }
    }
}

/// Convert list of dicts to table
fn list_to_table(items: Vec<Value>) -> Result<Value, TypeError> {
    if items.is_empty() {
        return Ok(Value::Table(Table::new(Vec::new())));
    }
    
    // Get columns from first dict
    let first = items.first().unwrap();
    let columns: Vec<String> = match first {
        Value::Dict(d) => d.keys().cloned().collect(),
        _ => return Err(TypeError::ConversionError {
            from: "List".to_string(),
            to: "Table".to_string(),
        }),
    };
    
    // Build rows
    let rows: Vec<Vec<Value>> = items.iter()
        .map(|item| {
            match item {
                Value::Dict(d) => {
                    columns.iter()
                        .map(|c| d.get(c).cloned().unwrap_or(Value::Null))
                        .collect()
                }
                _ => vec![Value::Null; columns.len()],
            }
        })
        .collect();
    
    Ok(Value::Table(Table::with_rows(columns, rows)))
}
```

---

*Continue to Part 4: AST Definitions*