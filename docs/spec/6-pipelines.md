# Part 6: Pipelines

## 6.1 Overview

Colony extends POSIX pipelines with typed data flow:

| Operator | Name | Semantics |
|----------|------|-----------|
| `\|` | Pipe | POSIX byte stream (untyped) |
| `->` | Arrow | Typed pipe with annotations |
| `=>` | Fat Arrow | Auto-converting typed pipe |
| `~>` | Alter | Structured modification pipe |

## 6.2 Pipeline Execution

```rust
// crates/colony-runtime/src/pipeline.rs

use std::process::{Command, Stdio, Child};
use std::io::{Read, Write};

pub struct PipelineExecutor {
    options: ShellOptions,
    type_registry: TypeRegistry,
}

impl PipelineExecutor {
    pub fn new(options: ShellOptions) -> Self {
        Self {
            options,
            type_registry: TypeRegistry::default(),
        }
    }
    
    /// Execute a pipeline
    pub fn execute(
        &mut self,
        pipeline: &Pipeline,
        evaluator: &mut Evaluator,
    ) -> Result<i32, PipelineError> {
        let elements = &pipeline.elements;
        
        if elements.is_empty() {
            return Ok(0);
        }
        
        // Single command - no pipe needed
        if elements.len() == 1 {
            return self.execute_single(evaluator, &elements[0]);
        }
        
        // Multi-command pipeline
        self.execute_multi(evaluator, elements)
    }
    
    /// Execute single command
    fn execute_single(
        &mut self,
        evaluator: &mut Evaluator,
        element: &PipelineElement,
    ) -> Result<i32, PipelineError> {
        let mut status = evaluator.eval_statement(&element.command)
            .map(|v| match v {
                Value::Int(n) => n as i32,
                _ => 0,
            })
            .map_err(|e| PipelineError::EvalError(e))?;
        
        if element.negated {
            status = if status == 0 { 1 } else { 0 };
        }
        
        Ok(status)
    }
    
    /// Execute multi-command pipeline
    fn execute_multi(
        &mut self,
        evaluator: &mut Evaluator,
        elements: &[PipelineElement],
    ) -> Result<i32, PipelineError> {
        // Determine pipeline type based on operators
        let is_typed = elements.iter().any(|e| {
            matches!(e.pipe_op, Some(PipeOp::Arrow | PipeOp::FatArrow | PipeOp::Alter))
        });
        
        if is_typed && self.options.parse_arrow {
            self.execute_typed_pipeline(evaluator, elements)
        } else {
            self.execute_posix_pipeline(evaluator, elements)
        }
    }
    
    /// Execute POSIX-style pipeline (byte streams)
    fn execute_posix_pipeline(
        &mut self,
        evaluator: &mut Evaluator,
        elements: &[PipelineElement],
    ) -> Result<i32, PipelineError> {
        let mut children: Vec<Child> = Vec::new();
        let mut prev_stdout: Option<std::process::ChildStdout> = None;
        
        for (i, element) in elements.iter().enumerate() {
            let is_last = i == elements.len() - 1;
            let include_stderr = matches!(element.pipe_op, Some(PipeOp::PipeErr));
            
            // Build command
            let (name, args) = self.expand_command(evaluator, &element.command)?;
            
            let mut cmd = Command::new(&name);
            cmd.args(&args);
            
            // Setup stdin from previous command
            if let Some(stdout) = prev_stdout.take() {
                cmd.stdin(Stdio::from(stdout));
            }
            
            // Setup stdout
            if !is_last {
                cmd.stdout(Stdio::piped());
                if include_stderr {
                    cmd.stderr(Stdio::piped());
                }
            }
            
            // Spawn
            let mut child = cmd.spawn()
                .map_err(|e| PipelineError::SpawnError(name.clone(), e))?;
            
            // Capture stdout for next command
            if !is_last {
                prev_stdout = child.stdout.take();
            }
            
            children.push(child);
        }
        
        // Wait for all children and collect statuses
        let mut statuses = Vec::new();
        for child in &mut children {
            let status = child.wait()
                .map_err(PipelineError::WaitError)?;
            statuses.push(status.code().unwrap_or(128));
        }
        
        // Return status based on pipefail setting
        if evaluator.options.strict_errexit {
            // pipefail: return first non-zero
            for &status in &statuses {
                if status != 0 {
                    return Ok(status);
                }
            }
            Ok(0)
        } else {
            // Default: return last command's status
            Ok(*statuses.last().unwrap_or(&0))
        }
    }
    
    /// Execute typed pipeline
    fn execute_typed_pipeline(
        &mut self,
        evaluator: &mut Evaluator,
        elements: &[PipelineElement],
    ) -> Result<i32, PipelineError> {
        let mut current_data: Option<TypedData> = None;
        let mut last_status = 0;
        
        for (i, element) in elements.iter().enumerate() {
            let is_last = i == elements.len() - 1;
            let pipe_op = if i > 0 {
                elements[i - 1].pipe_op
            } else {
                None
            };
            
            // Get command info
            let (name, args) = self.expand_command(evaluator, &element.command)?;
            
            // Get type signature from registry
            let signature = self.type_registry.get(&name);
            
            // Validate input type
            if let (Some(ref data), Some(ref sig)) = (&current_data, &signature) {
                if !sig.input.is_compatible(&data.pipeline_type) {
                    return Err(PipelineError::TypeMismatch {
                        expected: sig.input.clone(),
                        actual: data.pipeline_type.clone(),
                        command: name.clone(),
                    });
                }
            }
            
            // Execute command
            let result = self.execute_typed_command(
                evaluator,
                &name,
                &args,
                current_data.take(),
                pipe_op,
            )?;
            
            last_status = result.status;
            
            if !is_last {
                current_data = Some(result.data);
            } else {
                // Final output - write to stdout
                if let Some(data) = result.data {
                    self.output_typed_data(&data)?;
                }
            }
        }
        
        Ok(last_status)
    }
    
    /// Execute a single typed command
    fn execute_typed_command(
        &mut self,
        evaluator: &mut Evaluator,
        name: &str,
        args: &[String],
        input: Option<TypedData>,
        pipe_op: Option<PipeOp>,
    ) -> Result<TypedResult, PipelineError> {
        // Convert input based on pipe operator
        let input_bytes = if let Some(data) = input {
            match pipe_op {
                Some(PipeOp::Arrow) => {
                    // Pass through, let command handle type
                    data.to_bytes()
                }
                Some(PipeOp::FatArrow) => {
                    // Auto-convert to command's expected input
                    let sig = self.type_registry.get(name);
                    if let Some(sig) = sig {
                        data.convert_to(&sig.input)?.to_bytes()
                    } else {
                        data.to_bytes()
                    }
                }
                Some(PipeOp::Alter) => {
                    // Structured modification - handled specially
                    return self.execute_alter_command(evaluator, name, args, data);
                }
                _ => data.to_bytes(),
            }
        } else {
            Vec::new()
        };
        
        // Spawn process
        let mut cmd = Command::new(name);
        cmd.args(args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        
        let mut child = cmd.spawn()
            .map_err(|e| PipelineError::SpawnError(name.to_string(), e))?;
        
        // Write input
        if !input_bytes.is_empty() {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&input_bytes)?;
            }
        }
        
        // Read output
        let mut output = Vec::new();
        if let Some(mut stdout) = child.stdout.take() {
            stdout.read_to_end(&mut output)?;
        }
        
        let status = child.wait()?.code().unwrap_or(128);
        
        // Infer output type
        let output_type = self.infer_output_type(name, &output);
        
        Ok(TypedResult {
            status,
            data: TypedData {
                bytes: output,
                pipeline_type: output_type,
            },
        })
    }
    
    /// Execute alter pipe command
    fn execute_alter_command(
        &mut self,
        evaluator: &mut Evaluator,
        name: &str,
        args: &[String],
        input: TypedData,
    ) -> Result<TypedResult, PipelineError> {
        // Parse input as structured data
        let mut value = input.to_value()?;
        
        // Apply transformation
        match name {
            "select" | "pick" => {
                // Select specific fields
                value = self.alter_select(value, args)?;
            }
            "reject" | "drop" => {
                // Remove specific fields
                value = self.alter_reject(value, args)?;
            }
            "rename" => {
                // Rename fields
                value = self.alter_rename(value, args)?;
            }
            "update" => {
                // Update field values
                value = self.alter_update(evaluator, value, args)?;
            }
            "where" | "filter" => {
                // Filter rows
                value = self.alter_filter(evaluator, value, args)?;
            }
            "sort" | "sort-by" => {
                // Sort rows
                value = self.alter_sort(value, args)?;
            }
            "reverse" => {
                value = self.alter_reverse(value)?;
            }
            "first" | "take" => {
                let n = args.first()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1);
                value = self.alter_first(value, n)?;
            }
            "last" => {
                let n = args.first()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1);
                value = self.alter_last(value, n)?;
            }
            "skip" => {
                let n = args.first()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1);
                value = self.alter_skip(value, n)?;
            }
            "group-by" => {
                value = self.alter_group_by(value, args)?;
            }
            "flatten" => {
                value = self.alter_flatten(value)?;
            }
            _ => {
                return Err(PipelineError::UnknownAlterCommand(name.to_string()));
            }
        }
        
        // Convert back to bytes
        let output = TypedData::from_value(value, input.pipeline_type);
        
        Ok(TypedResult {
            status: 0,
            data: output,
        })
    }
    
    /// Expand command to name and args
    fn expand_command(
        &self,
        evaluator: &mut Evaluator,
        stmt: &Statement,
    ) -> Result<(String, Vec<String>), PipelineError> {
        match stmt {
            Statement::SimpleCommand(cmd) => {
                let name = evaluator.expand_word(&cmd.name)
                    .map_err(|e| PipelineError::EvalError(e))?;
                let mut args = Vec::new();
                for arg in &cmd.args {
                    let expanded = evaluator.expand_word_glob(arg)
                        .map_err(|e| PipelineError::EvalError(e))?;
                    args.extend(expanded);
                }
                Ok((name, args))
            }
            _ => Err(PipelineError::NotACommand),
        }
    }
    
    /// Infer output type from command and content
    fn infer_output_type(&self, name: &str, output: &[u8]) -> PipelineType {
        // Check registry first
        if let Some(sig) = self.type_registry.get(name) {
            return sig.output.clone();
        }
        
        // Try to infer from content
        if let Ok(s) = std::str::from_utf8(output) {
            let trimmed = s.trim();
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
                    return if trimmed.starts_with('[') {
                        PipelineType::JsonArray
                    } else {
                        PipelineType::JsonObject
                    };
                }
            }
        }
        
        PipelineType::Str
    }
    
    /// Output typed data to stdout
    fn output_typed_data(&self, data: &TypedData) -> Result<(), PipelineError> {
        use std::io::Write;
        std::io::stdout().write_all(&data.bytes)?;
        Ok(())
    }
}
```

## 6.3 Typed Data

```rust
/// Data flowing through typed pipeline
#[derive(Clone, Debug)]
pub struct TypedData {
    pub bytes: Vec<u8>,
    pub pipeline_type: PipelineType,
}

impl TypedData {
    pub fn from_string(s: String, pipeline_type: PipelineType) -> Self {
        Self {
            bytes: s.into_bytes(),
            pipeline_type,
        }
    }
    
    pub fn from_value(value: Value, pipeline_type: PipelineType) -> Self {
        let bytes = match pipeline_type {
            PipelineType::Json | PipelineType::JsonArray | PipelineType::JsonObject => {
                serde_json::to_string_pretty(&value.to_json())
                    .unwrap_or_default()
                    .into_bytes()
            }
            PipelineType::Yaml => {
                serde_yaml::to_string(&value.to_json())
                    .unwrap_or_default()
                    .into_bytes()
            }
            _ => value.to_string().into_bytes(),
        };
        
        Self { bytes, pipeline_type }
    }
    
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
    
    pub fn to_string(&self) -> Result<String, PipelineError> {
        String::from_utf8(self.bytes.clone())
            .map_err(|_| PipelineError::InvalidUtf8)
    }
    
    pub fn to_value(&self) -> Result<Value, PipelineError> {
        let s = self.to_string()?;
        
        match self.pipeline_type {
            PipelineType::Json | PipelineType::JsonArray | PipelineType::JsonObject => {
                let json: serde_json::Value = serde_json::from_str(&s)?;
                Ok(json_to_value(json))
            }
            PipelineType::Yaml => {
                let yaml: serde_yaml::Value = serde_yaml::from_str(&s)?;
                Ok(yaml_to_value(yaml))
            }
            PipelineType::Int => {
                s.trim().parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| PipelineError::ParseError("Int".to_string()))
            }
            PipelineType::Float => {
                s.trim().parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| PipelineError::ParseError("Float".to_string()))
            }
            PipelineType::Bool => {
                match s.trim() {
                    "true" | "1" | "yes" => Ok(Value::Bool(true)),
                    "false" | "0" | "no" => Ok(Value::Bool(false)),
                    _ => Err(PipelineError::ParseError("Bool".to_string())),
                }
            }
            _ => Ok(Value::Str(s)),
        }
    }
    
    pub fn convert_to(&self, target: &PipelineType) -> Result<Self, PipelineError> {
        if self.pipeline_type == *target {
            return Ok(self.clone());
        }
        
        let value = self.to_value()?;
        let converted = coerce(value, target)
            .map_err(|e| PipelineError::ConversionError(format!("{}", e)))?;
        
        Ok(Self::from_value(converted, target.clone()))
    }
}

pub struct TypedResult {
    pub status: i32,
    pub data: TypedData,
}
```

## 6.4 Type Registry

```rust
/// Registry of command type signatures
#[derive(Default)]
pub struct TypeRegistry {
    signatures: IndexMap<String, CommandSignature>,
}

#[derive(Clone, Debug)]
pub struct CommandSignature {
    pub input: PipelineType,
    pub output: PipelineType,
    pub description: Option<String>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            signatures: IndexMap::new(),
        };
        
        // Register common commands
        registry.register_defaults();
        registry
    }
    
    fn register_defaults(&mut self) {
        // JSON tools
        self.register("jq", PipelineType::Json, PipelineType::Json);
        self.register("yq", PipelineType::Yaml, PipelineType::Any);
        
        // Format converters
        self.register("json2yaml", PipelineType::Json, PipelineType::Yaml);
        self.register("yaml2json", PipelineType::Yaml, PipelineType::Json);
        self.register("csv2json", PipelineType::Csv, PipelineType::JsonArray);
        self.register("json2csv", PipelineType::JsonArray, PipelineType::Csv);
        
        // Text tools
        self.register("cat", PipelineType::Any, PipelineType::Str);
        self.register("grep", PipelineType::Str, PipelineType::Str);
        self.register("sed", PipelineType::Str, PipelineType::Str);
        self.register("awk", PipelineType::Str, PipelineType::Str);
        self.register("sort", PipelineType::Str, PipelineType::Str);
        self.register("uniq", PipelineType::Str, PipelineType::Str);
        self.register("head", PipelineType::Str, PipelineType::Str);
        self.register("tail", PipelineType::Str, PipelineType::Str);
        self.register("wc", PipelineType::Str, PipelineType::Str);
        
        // Data tools
        self.register("curl", PipelineType::Any, PipelineType::Any);
        self.register("httpie", PipelineType::Any, PipelineType::Json);
        
        // File info
        self.register("ls", PipelineType::Any, PipelineType::Str);
        self.register("stat", PipelineType::Path, PipelineType::Str);
        self.register("file", PipelineType::Path, PipelineType::Str);
    }
    
    pub fn register(&mut self, command: &str, input: PipelineType, output: PipelineType) {
        self.signatures.insert(command.to_string(), CommandSignature {
            input,
            output,
            description: None,
        });
    }
    
    pub fn register_with_desc(&mut self, command: &str, input: PipelineType, output: PipelineType, desc: &str) {
        self.signatures.insert(command.to_string(), CommandSignature {
            input,
            output,
            description: Some(desc.to_string()),
        });
    }
    
    pub fn get(&self, command: &str) -> Option<&CommandSignature> {
        self.signatures.get(command)
    }
    
    /// Load custom type definitions from TOOLS.md
    pub fn load_from_tools_file(&mut self, content: &str) -> Result<(), ParseError> {
        // Parse markdown table format
        for line in content.lines() {
            if line.starts_with('|') && !line.contains("---") {
                let parts: Vec<&str> = line.split('|')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect();
                
                if parts.len() >= 3 {
                    let command = parts[0];
                    let input = PipelineType::from_str(parts[1]);
                    let output = PipelineType::from_str(parts[2]);
                    let desc = parts.get(3).map(|s| s.to_string());
                    
                    self.signatures.insert(command.to_string(), CommandSignature {
                        input,
                        output,
                        description: desc,
                    });
                }
            }
        }
        
        Ok(())
    }
}
```

## 6.5 Alter Operations

```rust
impl PipelineExecutor {
    /// Select specific fields
    fn alter_select(&self, value: Value, fields: &[String]) -> Result<Value, PipelineError> {
        match value {
            Value::Dict(d) => {
                let selected: IndexMap<String, Value> = fields.iter()
                    .filter_map(|f| d.get(f).map(|v| (f.clone(), v.clone())))
                    .collect();
                Ok(Value::Dict(selected))
            }
            Value::List(items) => {
                let selected: Vec<Value> = items.into_iter()
                    .map(|item| self.alter_select(item, fields))
                    .collect::<Result<_, _>>()?;
                Ok(Value::List(selected))
            }
            Value::Table(t) => {
                let cols: Vec<String> = fields.iter()
                    .filter(|f| t.columns.contains(f))
                    .cloned()
                    .collect();
                let col_indices: Vec<usize> = cols.iter()
                    .filter_map(|c| t.columns.iter().position(|tc| tc == c))
                    .collect();
                
                let rows: Vec<Vec<Value>> = t.rows.iter()
                    .map(|row| col_indices.iter().map(|&i| row[i].clone()).collect())
                    .collect();
                
                Ok(Value::Table(Table::with_rows(cols, rows)))
            }
            _ => Err(PipelineError::AlterTypeError("select".to_string(), value.type_name().to_string())),
        }
    }
    
    /// Reject specific fields
    fn alter_reject(&self, value: Value, fields: &[String]) -> Result<Value, PipelineError> {
        match value {
            Value::Dict(d) => {
                let rejected: IndexMap<String, Value> = d.into_iter()
                    .filter(|(k, _)| !fields.contains(k))
                    .collect();
                Ok(Value::Dict(rejected))
            }
            Value::List(items) => {
                let rejected: Vec<Value> = items.into_iter()
                    .map(|item| self.alter_reject(item, fields))
                    .collect::<Result<_, _>>()?;
                Ok(Value::List(rejected))
            }
            _ => Err(PipelineError::AlterTypeError("reject".to_string(), value.type_name().to_string())),
        }
    }
    
    /// Rename fields (args: old1 new1 old2 new2 ...)
    fn alter_rename(&self, value: Value, args: &[String]) -> Result<Value, PipelineError> {
        let renames: IndexMap<&str, &str> = args.chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some((chunk[0].as_str(), chunk[1].as_str()))
                } else {
                    None
                }
            })
            .collect();
        
        match value {
            Value::Dict(d) => {
                let renamed: IndexMap<String, Value> = d.into_iter()
                    .map(|(k, v)| {
                        let new_key = renames.get(k.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or(k);
                        (new_key, v)
                    })
                    .collect();
                Ok(Value::Dict(renamed))
            }
            Value::List(items) => {
                let renamed: Vec<Value> = items.into_iter()
                    .map(|item| self.alter_rename(item, args))
                    .collect::<Result<_, _>>()?;
                Ok(Value::List(renamed))
            }
            _ => Err(PipelineError::AlterTypeError("rename".to_string(), value.type_name().to_string())),
        }
    }
    
    /// Filter rows (args: field operator value)
    fn alter_filter(&self, evaluator: &mut Evaluator, value: Value, args: &[String]) -> Result<Value, PipelineError> {
        if args.len() < 3 {
            return Err(PipelineError::InvalidAlterArgs("filter requires: field operator value".to_string()));
        }
        
        let field = &args[0];
        let op = &args[1];
        let test_value = &args[2];
        
        match value {
            Value::List(items) => {
                let filtered: Vec<Value> = items.into_iter()
                    .filter(|item| {
                        if let Value::Dict(d) = item {
                            if let Some(field_value) = d.get(field) {
                                return self.compare_filter(field_value, op, test_value);
                            }
                        }
                        false
                    })
                    .collect();
                Ok(Value::List(filtered))
            }
            _ => Err(PipelineError::AlterTypeError("filter".to_string(), value.type_name().to_string())),
        }
    }
    
    fn compare_filter(&self, field_value: &Value, op: &str, test: &str) -> bool {
        let field_str = field_value.to_string();
        
        match op {
            "==" | "=" | "eq" => field_str == test,
            "!=" | "ne" => field_str != test,
            ">" | "gt" => {
                if let (Ok(a), Ok(b)) = (field_str.parse::<f64>(), test.parse::<f64>()) {
                    a > b
                } else {
                    field_str > test.to_string()
                }
            }
            ">=" | "ge" => {
                if let (Ok(a), Ok(b)) = (field_str.parse::<f64>(), test.parse::<f64>()) {
                    a >= b
                } else {
                    field_str >= test.to_string()
                }
            }
            "<" | "lt" => {
                if let (Ok(a), Ok(b)) = (field_str.parse::<f64>(), test.parse::<f64>()) {
                    a < b
                } else {
                    field_str < test.to_string()
                }
            }
            "<=" | "le" => {
                if let (Ok(a), Ok(b)) = (field_str.parse::<f64>(), test.parse::<f64>()) {
                    a <= b
                } else {
                    field_str <= test.to_string()
                }
            }
            "~" | "=~" | "contains" => field_str.contains(test),
            "!~" | "not-contains" => !field_str.contains(test),
            "starts-with" => field_str.starts_with(test),
            "ends-with" => field_str.ends_with(test),
            _ => false,
        }
    }
    
    /// Sort rows
    fn alter_sort(&self, value: Value, args: &[String]) -> Result<Value, PipelineError> {
        match value {
            Value::List(mut items) => {
                let field = args.first().map(|s| s.as_str());
                let reverse = args.iter().any(|a| a == "--reverse" || a == "-r");
                
                items.sort_by(|a, b| {
                    let (va, vb) = if let Some(f) = field {
                        let va = if let Value::Dict(d) = a { d.get(f) } else { None };
                        let vb = if let Value::Dict(d) = b { d.get(f) } else { None };
                        (va, vb)
                    } else {
                        (Some(a), Some(b))
                    };
                    
                    match (va, vb) {
                        (Some(va), Some(vb)) => {
                            let cmp = self.compare_values_order(va, vb);
                            if reverse { cmp.reverse() } else { cmp }
                        }
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                });
                
                Ok(Value::List(items))
            }
            _ => Err(PipelineError::AlterTypeError("sort".to_string(), value.type_name().to_string())),
        }
    }
    
    fn compare_values_order(&self, a: &Value, b: &Value) -> std::cmp::Ordering {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
            (Value::Str(a), Value::Str(b)) => a.cmp(b),
            _ => a.to_string().cmp(&b.to_string()),
        }
    }
    
    /// Reverse order
    fn alter_reverse(&self, value: Value) -> Result<Value, PipelineError> {
        match value {
            Value::List(mut items) => {
                items.reverse();
                Ok(Value::List(items))
            }
            Value::Str(s) => {
                Ok(Value::Str(s.chars().rev().collect()))
            }
            _ => Err(PipelineError::AlterTypeError("reverse".to_string(), value.type_name().to_string())),
        }
    }
    
    /// Take first N items
    fn alter_first(&self, value: Value, n: usize) -> Result<Value, PipelineError> {
        match value {
            Value::List(items) => {
                Ok(Value::List(items.into_iter().take(n).collect()))
            }
            _ => Err(PipelineError::AlterTypeError("first".to_string(), value.type_name().to_string())),
        }
    }
    
    /// Take last N items
    fn alter_last(&self, value: Value, n: usize) -> Result<Value, PipelineError> {
        match value {
            Value::List(items) => {
                let len = items.len();
                let skip = if len > n { len - n } else { 0 };
                Ok(Value::List(items.into_iter().skip(skip).collect()))
            }
            _ => Err(PipelineError::AlterTypeError("last".to_string(), value.type_name().to_string())),
        }
    }
    
    /// Skip first N items
    fn alter_skip(&self, value: Value, n: usize) -> Result<Value, PipelineError> {
        match value {
            Value::List(items) => {
                Ok(Value::List(items.into_iter().skip(n).collect()))
            }
            _ => Err(PipelineError::AlterTypeError("skip".to_string(), value.type_name().to_string())),
        }
    }
    
    /// Group by field
    fn alter_group_by(&self, value: Value, args: &[String]) -> Result<Value, PipelineError> {
        let field = args.first()
            .ok_or_else(|| PipelineError::InvalidAlterArgs("group-by requires field name".to_string()))?;
        
        match value {
            Value::List(items) => {
                let mut groups: IndexMap<String, Vec<Value>> = IndexMap::new();
                
                for item in items {
                    let key = if let Value::Dict(ref d) = item {
                        d.get(field).map(|v| v.to_string()).unwrap_or_default()
                    } else {
                        String::new()
                    };
                    
                    groups.entry(key).or_default().push(item);
                }
                
                let result: IndexMap<String, Value> = groups.into_iter()
                    .map(|(k, v)| (k, Value::List(v)))
                    .collect();
                
                Ok(Value::Dict(result))
            }
            _ => Err(PipelineError::AlterTypeError("group-by".to_string(), value.type_name().to_string())),
        }
    }
    
    /// Flatten nested lists
    fn alter_flatten(&self, value: Value) -> Result<Value, PipelineError> {
        match value {
            Value::List(items) => {
                let mut flat = Vec::new();
                for item in items {
                    match item {
                        Value::List(inner) => flat.extend(inner),
                        other => flat.push(other),
                    }
                }
                Ok(Value::List(flat))
            }
            _ => Err(PipelineError::AlterTypeError("flatten".to_string(), value.type_name().to_string())),
        }
    }
}
```

## 6.6 Pipeline Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Failed to spawn '{0}': {1}")]
    SpawnError(String, std::io::Error),
    
    #[error("Failed to wait for process: {0}")]
    WaitError(std::io::Error),
    
    #[error("Pipeline type mismatch: command '{command}' expects {expected:?}, got {actual:?}")]
    TypeMismatch {
        expected: PipelineType,
        actual: PipelineType,
        command: String,
    },
    
    #[error("Conversion error: {0}")]
    ConversionError(String),
    
    #[error("Invalid UTF-8 in pipeline data")]
    InvalidUtf8,
    
    #[error("Parse error: expected {0}")]
    ParseError(String),
    
    #[error("Not a command")]
    NotACommand,
    
    #[error("Unknown alter command: {0}")]
    UnknownAlterCommand(String),
    
    #[error("Invalid arguments for alter operation: {0}")]
    InvalidAlterArgs(String),
    
    #[error("Alter operation '{0}' cannot operate on type {1}")]
    AlterTypeError(String, String),
    
    #[error("Evaluation error: {0}")]
    EvalError(#[from] EvalError),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
```

---

*Continue to Part 8: Buffer Management*