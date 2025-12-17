# Part 5: Execution Model

## 5.1 Overview

The Colony execution model handles:
- Statement and expression evaluation
- Variable scoping (dynamic vs lexical)
- Command execution and process management
- Error handling and propagation

## 5.2 Evaluator Structure

```rust
// crates/colony-runtime/src/evaluator.rs

use colony_core::types::Value;
use colony_parser::ast::*;
use crate::scope::ScopeStack;
use crate::process::ProcessManager;

pub struct Evaluator {
    /// Variable scope stack
    scopes: ScopeStack,
    /// Process manager for spawning commands
    processes: ProcessManager,
    /// Channel manager
    channels: ChannelManager,
    /// Shell options
    options: ShellOptions,
    /// Last command exit status
    last_status: i32,
    /// Current working directory
    cwd: PathBuf,
    /// Environment variables
    env: IndexMap<String, String>,
    /// Builtin commands
    builtins: IndexMap<String, BuiltinFn>,
    /// Defined functions/procs
    functions: IndexMap<String, Callable>,
    /// Defined agents
    agents: IndexMap<String, AgentDef>,
    /// Loop control state
    loop_control: Option<LoopControl>,
    /// Return value from function
    return_value: Option<Value>,
}

#[derive(Clone, Debug)]
pub enum Callable {
    /// POSIX function
    Function(FunctionDef),
    /// Colony proc
    Proc(ProcDef),
    /// Colony func
    Func(FuncDef),
    /// Builtin
    Builtin(BuiltinFn),
}

#[derive(Clone, Copy, Debug)]
enum LoopControl {
    Break(Option<u32>),    // Break with optional level
    Continue(Option<u32>), // Continue with optional level
}

pub type BuiltinFn = fn(&mut Evaluator, &[String]) -> Result<i32, EvalError>;

impl Evaluator {
    pub fn new(options: ShellOptions) -> Self {
        let mut eval = Self {
            scopes: ScopeStack::new(options.lexical_scope),
            processes: ProcessManager::new(),
            channels: ChannelManager::new(),
            options,
            last_status: 0,
            cwd: std::env::current_dir().unwrap_or_default(),
            env: std::env::vars().collect(),
            builtins: IndexMap::new(),
            functions: IndexMap::new(),
            agents: IndexMap::new(),
            loop_control: None,
            return_value: None,
        };
        
        eval.register_builtins();
        eval
    }
    
    /// Execute a program (list of statements)
    pub fn eval_program(&mut self, program: &Program) -> Result<i32, EvalError> {
        for stmt in &program.statements {
            self.eval_statement(stmt)?;
            
            // Check for return/break/continue
            if self.return_value.is_some() || self.loop_control.is_some() {
                break;
            }
        }
        
        Ok(self.last_status)
    }
    
    /// Execute a single statement
    pub fn eval_statement(&mut self, stmt: &Statement) -> Result<Value, EvalError> {
        match stmt {
            Statement::SimpleCommand(cmd) => {
                self.last_status = self.eval_simple_command(cmd)?;
                Ok(Value::Int(self.last_status as i64))
            }
            Statement::Pipeline(pipeline) => {
                self.last_status = self.eval_pipeline(pipeline)?;
                Ok(Value::Int(self.last_status as i64))
            }
            Statement::List(list) => {
                self.last_status = self.eval_command_list(list)?;
                Ok(Value::Int(self.last_status as i64))
            }
            Statement::BraceGroup(group) => {
                self.eval_brace_group(group)
            }
            Statement::Subshell(subshell) => {
                self.eval_subshell(subshell)
            }
            Statement::If(if_stmt) => {
                self.eval_if(if_stmt)
            }
            Statement::Case(case_stmt) => {
                self.eval_case(case_stmt)
            }
            Statement::For(for_stmt) => {
                self.eval_for(for_stmt)
            }
            Statement::While(while_stmt) => {
                self.eval_while(while_stmt)
            }
            Statement::Until(until_stmt) => {
                self.eval_until(until_stmt)
            }
            Statement::Match(match_stmt) => {
                self.eval_match(match_stmt)
            }
            Statement::VarDecl(decl) => {
                self.eval_var_decl(decl)
            }
            Statement::FunctionDef(func) => {
                self.define_function(func.clone());
                Ok(Value::Null)
            }
            Statement::ProcDef(proc) => {
                self.define_proc(proc.clone());
                Ok(Value::Null)
            }
            Statement::FuncDef(func) => {
                self.define_func(func.clone());
                Ok(Value::Null)
            }
            Statement::AgentDef(agent) => {
                self.define_agent(agent.clone());
                Ok(Value::Null)
            }
            Statement::Try(try_stmt) => {
                self.eval_try(try_stmt)
            }
            Statement::Return(ret) => {
                self.eval_return(ret)
            }
            Statement::Break(brk) => {
                self.loop_control = Some(LoopControl::Break(None));
                Ok(Value::Null)
            }
            Statement::Continue(cont) => {
                self.loop_control = Some(LoopControl::Continue(None));
                Ok(Value::Null)
            }
            Statement::Expression(expr) => {
                self.eval_expression(expr)
            }
            Statement::Assignment(assign) => {
                self.eval_assignment(assign)
            }
            Statement::Empty => Ok(Value::Null),
        }
    }
}
```

## 5.3 Variable Scoping

```rust
// crates/colony-runtime/src/scope.rs

use colony_core::types::Value;
use indexmap::IndexMap;

/// Variable scope
#[derive(Clone, Debug)]
pub struct Scope {
    /// Variables in this scope
    variables: IndexMap<String, Variable>,
    /// Scope kind
    kind: ScopeKind,
}

#[derive(Clone, Debug)]
pub struct Variable {
    pub value: Value,
    pub readonly: bool,
    pub exported: bool,
    pub type_annotation: Option<TypeAnnotation>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScopeKind {
    /// Global scope
    Global,
    /// Function/proc scope
    Function,
    /// Block scope (if lexical_scope enabled)
    Block,
    /// Loop scope
    Loop,
}

/// Stack of scopes for variable lookup
pub struct ScopeStack {
    scopes: Vec<Scope>,
    lexical: bool,
}

impl ScopeStack {
    pub fn new(lexical: bool) -> Self {
        Self {
            scopes: vec![Scope {
                variables: IndexMap::new(),
                kind: ScopeKind::Global,
            }],
            lexical,
        }
    }
    
    /// Push a new scope
    pub fn push(&mut self, kind: ScopeKind) {
        self.scopes.push(Scope {
            variables: IndexMap::new(),
            kind,
        });
    }
    
    /// Pop current scope
    pub fn pop(&mut self) -> Option<Scope> {
        if self.scopes.len() > 1 {
            self.scopes.pop()
        } else {
            None
        }
    }
    
    /// Get variable value
    pub fn get(&self, name: &str) -> Option<&Value> {
        // Search from innermost to outermost scope
        for scope in self.scopes.iter().rev() {
            if let Some(var) = scope.variables.get(name) {
                return Some(&var.value);
            }
        }
        None
    }
    
    /// Set variable value (creates in current scope or updates existing)
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), ScopeError> {
        // In lexical mode, search existing first
        if self.lexical {
            for scope in self.scopes.iter_mut().rev() {
                if let Some(var) = scope.variables.get_mut(name) {
                    if var.readonly {
                        return Err(ScopeError::ReadOnly(name.to_string()));
                    }
                    var.value = value;
                    return Ok(());
                }
            }
        }
        
        // Create in current scope
        let scope = self.scopes.last_mut().unwrap();
        scope.variables.insert(name.to_string(), Variable {
            value,
            readonly: false,
            exported: false,
            type_annotation: None,
        });
        Ok(())
    }
    
    /// Declare variable in current scope (Colony var/const)
    pub fn declare(&mut self, name: &str, value: Value, readonly: bool, type_ann: Option<TypeAnnotation>) -> Result<(), ScopeError> {
        let scope = self.scopes.last_mut().unwrap();
        
        // Check for redeclaration in same scope
        if scope.variables.contains_key(name) && self.lexical {
            return Err(ScopeError::AlreadyDeclared(name.to_string()));
        }
        
        scope.variables.insert(name.to_string(), Variable {
            value,
            readonly,
            exported: false,
            type_annotation: type_ann,
        });
        Ok(())
    }
    
    /// Update existing variable (setvar)
    pub fn update(&mut self, name: &str, value: Value) -> Result<(), ScopeError> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(var) = scope.variables.get_mut(name) {
                if var.readonly {
                    return Err(ScopeError::ReadOnly(name.to_string()));
                }
                var.value = value;
                return Ok(());
            }
        }
        Err(ScopeError::Undefined(name.to_string()))
    }
    
    /// Check if variable exists
    pub fn contains(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|s| s.variables.contains_key(name))
    }
    
    /// Get all exported variables
    pub fn exported(&self) -> IndexMap<String, String> {
        let mut result = IndexMap::new();
        for scope in &self.scopes {
            for (name, var) in &scope.variables {
                if var.exported {
                    result.insert(name.clone(), var.value.to_string());
                }
            }
        }
        result
    }
    
    /// Mark variable as exported
    pub fn export(&mut self, name: &str) -> Result<(), ScopeError> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(var) = scope.variables.get_mut(name) {
                var.exported = true;
                return Ok(());
            }
        }
        Err(ScopeError::Undefined(name.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScopeError {
    #[error("Variable '{0}' is readonly")]
    ReadOnly(String),
    #[error("Variable '{0}' is already declared in this scope")]
    AlreadyDeclared(String),
    #[error("Variable '{0}' is not defined")]
    Undefined(String),
}
```

## 5.4 Simple Command Execution

```rust
impl Evaluator {
    /// Execute a simple command
    pub fn eval_simple_command(&mut self, cmd: &SimpleCommand) -> Result<i32, EvalError> {
        // Handle empty command with just assignments
        if cmd.name.parts.is_empty() && cmd.args.is_empty() {
            for assign in &cmd.env_assignments {
                let value = self.expand_word(&assign.value)?;
                self.scopes.set(&assign.name, Value::Str(value))?;
            }
            return Ok(0);
        }
        
        // Expand command name
        let name = self.expand_word(&cmd.name)?;
        
        // Expand arguments
        let mut args: Vec<String> = Vec::new();
        for arg in &cmd.args {
            let expanded = self.expand_word_glob(arg)?;
            args.extend(expanded);
        }
        
        // Check for builtin
        if let Some(builtin) = self.builtins.get(&name).cloned() {
            let all_args: Vec<String> = std::iter::once(name.clone())
                .chain(args)
                .collect();
            return builtin(self, &all_args);
        }
        
        // Check for function/proc
        if let Some(callable) = self.functions.get(&name).cloned() {
            return self.call_function(&callable, &args);
        }
        
        // Check for agent invocation (name starts with @)
        if name.starts_with('@') {
            let agent_name = &name[1..];
            return self.invoke_agent(agent_name, &args);
        }
        
        // External command
        self.exec_external(&name, &args, &cmd.redirects, &cmd.env_assignments)
    }
    
    /// Execute external command
    fn exec_external(
        &mut self,
        name: &str,
        args: &[String],
        redirects: &[Redirect],
        env_assigns: &[Assignment],
    ) -> Result<i32, EvalError> {
        use std::process::{Command, Stdio};
        
        let mut cmd = Command::new(name);
        cmd.args(args);
        cmd.current_dir(&self.cwd);
        
        // Set environment
        cmd.env_clear();
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        for (k, v) in self.scopes.exported() {
            cmd.env(k, v);
        }
        for assign in env_assigns {
            let value = self.expand_word(&assign.value)?;
            cmd.env(&assign.name, &value);
        }
        
        // Apply redirections
        let (stdin, stdout, stderr) = self.setup_redirects(redirects)?;
        
        if let Some(stdin) = stdin {
            cmd.stdin(stdin);
        }
        if let Some(stdout) = stdout {
            cmd.stdout(stdout);
        }
        if let Some(stderr) = stderr {
            cmd.stderr(stderr);
        }
        
        // Spawn and wait
        let status = cmd.status()
            .map_err(|e| EvalError::CommandNotFound(name.to_string(), e))?;
        
        Ok(status.code().unwrap_or(128))
    }
    
    /// Setup I/O redirections
    fn setup_redirects(&self, redirects: &[Redirect]) -> Result<(Option<Stdio>, Option<Stdio>, Option<Stdio>), EvalError> {
        let mut stdin = None;
        let mut stdout = None;
        let mut stderr = None;
        
        for redir in redirects {
            let fd = redir.fd.unwrap_or(match redir.kind {
                RedirectKind::Input | RedirectKind::HereDoc(_) => 0,
                _ => 1,
            });
            
            match &redir.target {
                RedirectTarget::File(word) => {
                    let path = self.expand_word(word)?;
                    let file = match redir.kind {
                        RedirectKind::Input => {
                            std::fs::File::open(&path)
                                .map_err(|e| EvalError::RedirectError(path, e))?
                        }
                        RedirectKind::Output => {
                            std::fs::File::create(&path)
                                .map_err(|e| EvalError::RedirectError(path, e))?
                        }
                        RedirectKind::Append => {
                            std::fs::OpenOptions::new()
                                .append(true)
                                .create(true)
                                .open(&path)
                                .map_err(|e| EvalError::RedirectError(path, e))?
                        }
                        _ => {
                            std::fs::File::create(&path)
                                .map_err(|e| EvalError::RedirectError(path, e))?
                        }
                    };
                    
                    let stdio = Stdio::from(file);
                    match fd {
                        0 => stdin = Some(stdio),
                        1 => stdout = Some(stdio),
                        2 => stderr = Some(stdio),
                        _ => {} // Extended fds handled separately
                    }
                }
                RedirectTarget::Fd(target_fd) => {
                    // Duplicate fd
                    match (fd, target_fd) {
                        (2, 1) => stderr = Some(Stdio::inherit()), // 2>&1
                        (1, 2) => stdout = Some(Stdio::inherit()), // 1>&2
                        _ => {}
                    }
                }
                RedirectTarget::Close => {
                    match fd {
                        0 => stdin = Some(Stdio::null()),
                        1 => stdout = Some(Stdio::null()),
                        2 => stderr = Some(Stdio::null()),
                        _ => {}
                    }
                }
                RedirectTarget::HereDoc { content, expand, .. } => {
                    // Create pipe with here-doc content
                    let content = if *expand {
                        self.expand_string(content)?
                    } else {
                        content.clone()
                    };
                    stdin = Some(Stdio::piped());
                    // Note: actual here-doc content written in spawn phase
                }
            }
        }
        
        Ok((stdin, stdout, stderr))
    }
}
```

## 5.5 Function Calls

```rust
impl Evaluator {
    /// Call a function/proc
    fn call_function(&mut self, callable: &Callable, args: &[String]) -> Result<i32, EvalError> {
        match callable {
            Callable::Function(func) => self.call_posix_function(func, args),
            Callable::Proc(proc) => self.call_proc(proc, args),
            Callable::Func(func) => {
                let result = self.call_func(func, args)?;
                // For funcs called as commands, print result
                println!("{}", result.to_string());
                Ok(0)
            }
            Callable::Builtin(builtin) => {
                let all_args: Vec<String> = args.iter().cloned().collect();
                builtin(self, &all_args)
            }
        }
    }
    
    /// Call POSIX function (dynamic scoping)
    fn call_posix_function(&mut self, func: &FunctionDef, args: &[String]) -> Result<i32, EvalError> {
        // In POSIX functions, set positional parameters
        // But keep dynamic scoping (no new scope unless lexical mode)
        
        let old_args = self.get_positional_params();
        self.set_positional_params(args);
        
        // Execute body
        let mut result = 0;
        for stmt in &func.body {
            self.eval_statement(stmt)?;
            
            if let Some(ret) = self.return_value.take() {
                result = match ret {
                    Value::Int(n) => n as i32,
                    _ => 0,
                };
                break;
            }
            
            if self.loop_control.is_some() {
                break;
            }
        }
        
        self.set_positional_params(&old_args);
        self.last_status = result;
        Ok(result)
    }
    
    /// Call Colony proc (lexical scoping)
    fn call_proc(&mut self, proc: &ProcDef, args: &[String]) -> Result<i32, EvalError> {
        // Create new scope
        self.scopes.push(ScopeKind::Function);
        
        // Bind parameters
        self.bind_proc_params(&proc.params, args)?;
        
        // Execute body
        let mut result = 0;
        for stmt in &proc.body {
            self.eval_statement(stmt)?;
            
            if let Some(ret) = self.return_value.take() {
                result = match ret {
                    Value::Int(n) => n as i32,
                    _ => 0,
                };
                break;
            }
            
            if self.loop_control.is_some() {
                break;
            }
        }
        
        self.scopes.pop();
        self.last_status = result;
        Ok(result)
    }
    
    /// Call Colony func (returns value)
    fn call_func(&mut self, func: &FuncDef, args: &[String]) -> Result<Value, EvalError> {
        // Create new scope
        self.scopes.push(ScopeKind::Function);
        
        // Bind parameters
        self.bind_func_params(&func.params, args)?;
        
        // Execute body
        let result = match &func.body {
            FuncBody::Expression(expr) => {
                self.eval_expression(expr)?
            }
            FuncBody::Block(stmts) => {
                for stmt in stmts {
                    self.eval_statement(stmt)?;
                    
                    if let Some(ret) = self.return_value.take() {
                        self.scopes.pop();
                        return Ok(ret);
                    }
                }
                Value::Null
            }
        };
        
        self.scopes.pop();
        Ok(result)
    }
    
    /// Bind proc parameters
    fn bind_proc_params(&mut self, params: &ProcParams, args: &[String]) -> Result<(), EvalError> {
        // TODO: Parse args for named params (--name=value)
        // For now, just bind positional
        
        for (i, param) in params.positional.iter().enumerate() {
            let value = if i < args.len() {
                Value::Str(args[i].clone())
            } else if let Some(ref default) = param.default {
                self.eval_expression(default)?
            } else {
                return Err(EvalError::MissingArgument(param.name.clone()));
            };
            
            self.scopes.declare(&param.name, value, false, param.type_annotation.clone())?;
        }
        
        // Rest positional
        if let Some(ref rest_name) = params.rest_positional {
            let rest: Vec<Value> = args.iter()
                .skip(params.positional.len())
                .map(|s| Value::Str(s.clone()))
                .collect();
            self.scopes.declare(rest_name, Value::List(rest), false, None)?;
        }
        
        Ok(())
    }
    
    /// Bind func parameters (with type coercion)
    fn bind_func_params(&mut self, params: &[FuncParam], args: &[String]) -> Result<(), EvalError> {
        for (i, param) in params.iter().enumerate() {
            let value = if i < args.len() {
                let arg = &args[i];
                // Try to parse based on type annotation
                if let Some(ref ann) = param.type_annotation {
                    self.coerce_arg(arg, ann)?
                } else {
                    Value::Str(arg.clone())
                }
            } else if let Some(ref default) = param.default {
                self.eval_expression(default)?
            } else {
                return Err(EvalError::MissingArgument(param.name.clone()));
            };
            
            self.scopes.declare(&param.name, value, false, param.type_annotation.clone())?;
        }
        
        Ok(())
    }
    
    /// Coerce string argument to typed value
    fn coerce_arg(&self, arg: &str, type_ann: &TypeAnnotation) -> Result<Value, EvalError> {
        match type_ann {
            TypeAnnotation::Simple(name) => match name.as_str() {
                "Int" => arg.parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| EvalError::TypeCoercion(arg.to_string(), "Int".to_string())),
                "Float" => arg.parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| EvalError::TypeCoercion(arg.to_string(), "Float".to_string())),
                "Bool" => match arg {
                    "true" | "1" | "yes" => Ok(Value::Bool(true)),
                    "false" | "0" | "no" => Ok(Value::Bool(false)),
                    _ => Err(EvalError::TypeCoercion(arg.to_string(), "Bool".to_string())),
                },
                _ => Ok(Value::Str(arg.to_string())),
            },
            _ => Ok(Value::Str(arg.to_string())),
        }
    }
}
```

## 5.6 Control Flow

```rust
impl Evaluator {
    /// Evaluate if statement
    fn eval_if(&mut self, if_stmt: &IfStatement) -> Result<Value, EvalError> {
        // Evaluate condition
        if self.eval_condition(&if_stmt.condition)? {
            return self.eval_block(&if_stmt.then_body);
        }
        
        // Check elif branches
        for elif in &if_stmt.elif_branches {
            if self.eval_condition(&elif.condition)? {
                return self.eval_block(&elif.body);
            }
        }
        
        // Else branch
        if let Some(ref else_body) = if_stmt.else_body {
            return self.eval_block(else_body);
        }
        
        Ok(Value::Null)
    }
    
    /// Evaluate condition
    fn eval_condition(&mut self, cond: &Condition) -> Result<bool, EvalError> {
        match cond {
            Condition::Command(cmd) => {
                let status = self.eval_statement(cmd)?;
                match status {
                    Value::Int(n) => Ok(n == 0),
                    _ => Ok(false),
                }
            }
            Condition::Expression(expr) => {
                let value = self.eval_expression(expr)?;
                Ok(value.is_truthy())
            }
        }
    }
    
    /// Evaluate block of statements
    fn eval_block(&mut self, stmts: &[Statement]) -> Result<Value, EvalError> {
        let mut result = Value::Null;
        
        for stmt in stmts {
            result = self.eval_statement(stmt)?;
            
            if self.return_value.is_some() || self.loop_control.is_some() {
                break;
            }
        }
        
        Ok(result)
    }
    
    /// Evaluate for loop
    fn eval_for(&mut self, for_stmt: &ForStatement) -> Result<Value, EvalError> {
        match &for_stmt.kind {
            ForKind::Words { var, words } => {
                self.scopes.push(ScopeKind::Loop);
                
                for word in words {
                    let values = self.expand_word_glob(word)?;
                    for value in values {
                        self.scopes.set(var, Value::Str(value))?;
                        self.eval_block(&for_stmt.body)?;
                        
                        match self.loop_control.take() {
                            Some(LoopControl::Break(_)) => {
                                self.scopes.pop();
                                return Ok(Value::Null);
                            }
                            Some(LoopControl::Continue(_)) => continue,
                            None => {}
                        }
                        
                        if self.return_value.is_some() {
                            break;
                        }
                    }
                }
                
                self.scopes.pop();
            }
            ForKind::Iterate { var, expr } => {
                let iterable = self.eval_expression(expr)?;
                self.scopes.push(ScopeKind::Loop);
                
                for item in self.iter_value(&iterable)? {
                    self.scopes.set(var, item)?;
                    self.eval_block(&for_stmt.body)?;
                    
                    match self.loop_control.take() {
                        Some(LoopControl::Break(_)) => break,
                        Some(LoopControl::Continue(_)) => continue,
                        None => {}
                    }
                    
                    if self.return_value.is_some() {
                        break;
                    }
                }
                
                self.scopes.pop();
            }
            ForKind::Enumerate { index, var, expr } => {
                let iterable = self.eval_expression(expr)?;
                self.scopes.push(ScopeKind::Loop);
                
                for (i, item) in self.iter_value(&iterable)?.into_iter().enumerate() {
                    self.scopes.set(index, Value::Int(i as i64))?;
                    self.scopes.set(var, item)?;
                    self.eval_block(&for_stmt.body)?;
                    
                    match self.loop_control.take() {
                        Some(LoopControl::Break(_)) => break,
                        Some(LoopControl::Continue(_)) => continue,
                        None => {}
                    }
                    
                    if self.return_value.is_some() {
                        break;
                    }
                }
                
                self.scopes.pop();
            }
            ForKind::CStyle { init, condition, step } => {
                self.scopes.push(ScopeKind::Loop);
                
                // Init
                if let Some(init_expr) = init {
                    self.eval_expression(init_expr)?;
                }
                
                loop {
                    // Check condition
                    if let Some(cond_expr) = condition {
                        let cond = self.eval_expression(cond_expr)?;
                        if !cond.is_truthy() {
                            break;
                        }
                    }
                    
                    // Body
                    self.eval_block(&for_stmt.body)?;
                    
                    match self.loop_control.take() {
                        Some(LoopControl::Break(_)) => break,
                        Some(LoopControl::Continue(_)) => {}
                        None => {}
                    }
                    
                    if self.return_value.is_some() {
                        break;
                    }
                    
                    // Step
                    if let Some(step_expr) = step {
                        self.eval_expression(step_expr)?;
                    }
                }
                
                self.scopes.pop();
            }
        }
        
        Ok(Value::Null)
    }
    
    /// Evaluate while loop
    fn eval_while(&mut self, while_stmt: &WhileStatement) -> Result<Value, EvalError> {
        self.scopes.push(ScopeKind::Loop);
        
        while self.eval_condition(&while_stmt.condition)? {
            self.eval_block(&while_stmt.body)?;
            
            match self.loop_control.take() {
                Some(LoopControl::Break(_)) => break,
                Some(LoopControl::Continue(_)) => continue,
                None => {}
            }
            
            if self.return_value.is_some() {
                break;
            }
        }
        
        self.scopes.pop();
        Ok(Value::Null)
    }
    
    /// Iterate over a value
    fn iter_value(&self, value: &Value) -> Result<Vec<Value>, EvalError> {
        match value {
            Value::List(items) => Ok(items.clone()),
            Value::Dict(map) => {
                Ok(map.keys().map(|k| Value::Str(k.clone())).collect())
            }
            Value::Range(r) => {
                Ok(r.iter().map(Value::Int).collect())
            }
            Value::Str(s) => {
                Ok(s.chars().map(|c| Value::Str(c.to_string())).collect())
            }
            Value::Table(t) => {
                Ok(t.iter_rows()
                    .map(|row| {
                        let d: IndexMap<String, Value> = row.into_iter()
                            .map(|(k, v)| (k.to_string(), v.clone()))
                            .collect();
                        Value::Dict(d)
                    })
                    .collect())
            }
            _ => Err(EvalError::NotIterable(value.type_name().to_string())),
        }
    }
}
```

## 5.7 Expression Evaluation

```rust
impl Evaluator {
    /// Evaluate expression
    pub fn eval_expression(&mut self, expr: &Expression) -> Result<Value, EvalError> {
        match expr {
            Expression::Null(_) => Ok(Value::Null),
            Expression::Bool(b, _) => Ok(Value::Bool(*b)),
            Expression::Int(n, _) => Ok(Value::Int(*n)),
            Expression::Float(f, _) => Ok(Value::Float(*f)),
            Expression::Str(s, _) => Ok(Value::Str(s.clone())),
            
            Expression::List { items, .. } => {
                let values: Result<Vec<_>, _> = items.iter()
                    .map(|e| self.eval_expression(e))
                    .collect();
                Ok(Value::List(values?))
            }
            
            Expression::Dict { entries, .. } => {
                let mut map = IndexMap::new();
                for (k, v) in entries {
                    let key = match self.eval_expression(k)? {
                        Value::Str(s) => s,
                        other => other.to_string(),
                    };
                    let value = self.eval_expression(v)?;
                    map.insert(key, value);
                }
                Ok(Value::Dict(map))
            }
            
            Expression::Range { start, end, inclusive, .. } => {
                let start = match self.eval_expression(start)? {
                    Value::Int(n) => n,
                    _ => return Err(EvalError::TypeError("Range requires integers".to_string())),
                };
                let end = match self.eval_expression(end)? {
                    Value::Int(n) => n,
                    _ => return Err(EvalError::TypeError("Range requires integers".to_string())),
                };
                Ok(Value::Range(Box::new(RangeValue::new(start, end, *inclusive))))
            }
            
            Expression::Tuple { items, .. } => {
                let values: Result<Vec<_>, _> = items.iter()
                    .map(|e| self.eval_expression(e))
                    .collect();
                Ok(Value::Tuple(values?))
            }
            
            Expression::Variable { name, .. } => {
                self.scopes.get(name)
                    .cloned()
                    .ok_or_else(|| EvalError::UndefinedVariable(name.clone()))
            }
            
            Expression::Index { target, index, .. } => {
                let target = self.eval_expression(target)?;
                let index = self.eval_expression(index)?;
                self.index_value(&target, &index)
            }
            
            Expression::Field { target, field, .. } => {
                let target = self.eval_expression(target)?;
                self.field_value(&target, field)
            }
            
            Expression::Binary { left, op, right, .. } => {
                self.eval_binary(*op, left, right)
            }
            
            Expression::Unary { op, operand, .. } => {
                let value = self.eval_expression(operand)?;
                self.eval_unary(*op, value)
            }
            
            Expression::Ternary { condition, then_expr, else_expr, .. } => {
                let cond = self.eval_expression(condition)?;
                if cond.is_truthy() {
                    self.eval_expression(then_expr)
                } else {
                    self.eval_expression(else_expr)
                }
            }
            
            Expression::NullCoalesce { left, right, .. } => {
                let value = self.eval_expression(left)?;
                if matches!(value, Value::Null) {
                    self.eval_expression(right)
                } else {
                    Ok(value)
                }
            }
            
            Expression::FuncCall { name, args, named_args, .. } => {
                self.eval_func_call(name, args, named_args)
            }
            
            Expression::MethodCall { target, method, args, .. } => {
                let target = self.eval_expression(target)?;
                self.eval_method_call(target, method, args)
            }
            
            Expression::CommandSub { command, .. } => {
                self.eval_command_substitution(command)
            }
            
            Expression::Lambda { params, body, .. } => {
                // Capture environment for closure
                Ok(Value::Func(Arc::new(FuncDef {
                    name: "<lambda>".to_string(),
                    params: params.iter()
                        .map(|p| FuncParam {
                            name: p.clone(),
                            type_annotation: None,
                            default: None,
                        })
                        .collect(),
                    return_type: None,
                    body: FuncBody::Expression(*body.clone()),
                    span: Span::DUMMY,
                })))
            }
            
            Expression::Paren { inner, .. } => {
                self.eval_expression(inner)
            }
        }
    }
    
    /// Evaluate binary operation
    fn eval_binary(&mut self, op: BinaryOp, left: &Expression, right: &Expression) -> Result<Value, EvalError> {
        // Short-circuit for logical operators
        match op {
            BinaryOp::And => {
                let l = self.eval_expression(left)?;
                if !l.is_truthy() {
                    return Ok(Value::Bool(false));
                }
                let r = self.eval_expression(right)?;
                return Ok(Value::Bool(r.is_truthy()));
            }
            BinaryOp::Or => {
                let l = self.eval_expression(left)?;
                if l.is_truthy() {
                    return Ok(Value::Bool(true));
                }
                let r = self.eval_expression(right)?;
                return Ok(Value::Bool(r.is_truthy()));
            }
            _ => {}
        }
        
        let l = self.eval_expression(left)?;
        let r = self.eval_expression(right)?;
        
        match op {
            BinaryOp::Add => self.add_values(l, r),
            BinaryOp::Sub => self.sub_values(l, r),
            BinaryOp::Mul => self.mul_values(l, r),
            BinaryOp::Div => self.div_values(l, r),
            BinaryOp::IntDiv => self.intdiv_values(l, r),
            BinaryOp::Mod => self.mod_values(l, r),
            BinaryOp::Pow => self.pow_values(l, r),
            BinaryOp::Eq => Ok(Value::Bool(self.values_equal(&l, &r))),
            BinaryOp::Ne => Ok(Value::Bool(!self.values_equal(&l, &r))),
            BinaryOp::Lt => self.compare_values(&l, &r).map(|o| Value::Bool(o == Ordering::Less)),
            BinaryOp::Le => self.compare_values(&l, &r).map(|o| Value::Bool(o != Ordering::Greater)),
            BinaryOp::Gt => self.compare_values(&l, &r).map(|o| Value::Bool(o == Ordering::Greater)),
            BinaryOp::Ge => self.compare_values(&l, &r).map(|o| Value::Bool(o != Ordering::Less)),
            BinaryOp::Concat => self.concat_values(l, r),
            BinaryOp::BitAnd => self.bitand_values(l, r),
            BinaryOp::BitOr => self.bitor_values(l, r),
            BinaryOp::BitXor => self.bitxor_values(l, r),
            BinaryOp::Shl => self.shl_values(l, r),
            BinaryOp::Shr => self.shr_values(l, r),
            BinaryOp::Match => self.match_pattern(l, r),
            BinaryOp::NotMatch => self.match_pattern(l, r).map(|v| {
                match v {
                    Value::Bool(b) => Value::Bool(!b),
                    _ => Value::Bool(false),
                }
            }),
            BinaryOp::And | BinaryOp::Or => unreachable!(), // Handled above
        }
    }
    
    /// Evaluate unary operation
    fn eval_unary(&self, op: UnaryOp, value: Value) -> Result<Value, EvalError> {
        match op {
            UnaryOp::Neg => match value {
                Value::Int(n) => Ok(Value::Int(-n)),
                Value::Float(f) => Ok(Value::Float(-f)),
                _ => Err(EvalError::TypeError("Cannot negate non-numeric".to_string())),
            },
            UnaryOp::Not => Ok(Value::Bool(!value.is_truthy())),
            UnaryOp::BitNot => match value {
                Value::Int(n) => Ok(Value::Int(!n)),
                _ => Err(EvalError::TypeError("Bitwise not requires integer".to_string())),
            },
        }
    }
}
```

## 5.8 Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("Command not found: {0}: {1}")]
    CommandNotFound(String, std::io::Error),
    
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),
    
    #[error("Type error: {0}")]
    TypeError(String),
    
    #[error("Cannot iterate over {0}")]
    NotIterable(String),
    
    #[error("Missing required argument: {0}")]
    MissingArgument(String),
    
    #[error("Cannot coerce '{0}' to {1}")]
    TypeCoercion(String, String),
    
    #[error("Redirect error for '{0}': {1}")]
    RedirectError(String, std::io::Error),
    
    #[error("Division by zero")]
    DivisionByZero,
    
    #[error("Index out of bounds: {0}")]
    IndexOutOfBounds(i64),
    
    #[error("Field not found: {0}")]
    FieldNotFound(String),
    
    #[error("Scope error: {0}")]
    ScopeError(#[from] ScopeError),
    
    #[error("Agent error: {0}")]
    AgentError(String),
    
    #[error("Pipeline error: {0}")]
    PipelineError(String),
    
    #[error("User error: {0}")]
    UserError(String),
}
```

## 5.9 Error Catalog

This section provides a comprehensive catalog of all error types with codes, causes, and remediation.

### 5.9.1 Error Code Format

Errors follow the format `COL-XXXX` where:
- `COL-1xxx`: Parse errors
- `COL-2xxx`: Expansion errors
- `COL-3xxx`: Execution errors
- `COL-4xxx`: Type errors
- `COL-5xxx`: I/O errors
- `COL-6xxx`: Agent errors
- `COL-7xxx`: Internal errors

### 5.9.2 Parse Errors (COL-1xxx)

| Code | Name | Description | Remediation |
|------|------|-------------|-------------|
| COL-1001 | `UnexpectedToken` | Unexpected token in input | Check syntax near reported position |
| COL-1002 | `UnmatchedQuote` | Unclosed quote string | Add closing quote character |
| COL-1003 | `UnmatchedBrace` | Unclosed brace group | Add closing `}` |
| COL-1004 | `UnmatchedParen` | Unclosed parenthesis | Add closing `)` |
| COL-1005 | `UnmatchedBracket` | Unclosed bracket | Add closing `]` |
| COL-1006 | `InvalidHeredoc` | Malformed here-document | Check delimiter and content |
| COL-1007 | `InvalidRedirect` | Malformed redirection | Check fd number and target |
| COL-1008 | `ReservedWord` | Reserved word used as name | Use different identifier |
| COL-1009 | `InvalidEscape` | Invalid escape sequence | Check escape character |
| COL-1010 | `UnterminatedCommand` | Command missing terminator | Add `;` or newline |

### 5.9.3 Expansion Errors (COL-2xxx)

| Code | Name | Description | Remediation |
|------|------|-------------|-------------|
| COL-2001 | `BadSubstitution` | Malformed parameter expansion | Check `${...}` syntax |
| COL-2002 | `UnsetVariable` | Required variable is unset | Set variable or use default |
| COL-2003 | `BadArithmetic` | Arithmetic expression error | Check expression syntax |
| COL-2004 | `CommandSubFailed` | Command substitution failed | Check inner command |
| COL-2005 | `GlobNoMatch` | Glob pattern matched no files | Check pattern or disable `strict_glob` |
| COL-2006 | `TildeExpansion` | Tilde expansion failed | Check username or HOME |
| COL-2007 | `BraceExpansion` | Brace expansion error | Check `{a,b}` syntax |
| COL-2008 | `ProcessSubFailed` | Process substitution failed | Check inner command |

### 5.9.4 Execution Errors (COL-3xxx)

| Code | Name | Description | Remediation |
|------|------|-------------|-------------|
| COL-3001 | `CommandNotFound` | Command not in PATH | Install command or check PATH |
| COL-3002 | `PermissionDenied` | Insufficient permissions | Check file permissions |
| COL-3003 | `NotExecutable` | File is not executable | Run `chmod +x` on file |
| COL-3004 | `ForkFailed` | Unable to fork process | Check system resources |
| COL-3005 | `ExecFailed` | Unable to exec command | Check binary format |
| COL-3006 | `SignalReceived` | Process terminated by signal | Handle signal appropriately |
| COL-3007 | `PipelineError` | Pipeline construction failed | Check pipe limits |
| COL-3008 | `JobNotFound` | Referenced job doesn't exist | Check job number |
| COL-3009 | `BackgroundFailed` | Cannot background process | Check terminal control |
| COL-3010 | `FunctionNotFound` | Called function not defined | Define function first |
| COL-3011 | `MaxRecursion` | Maximum recursion exceeded | Check for infinite loops |
| COL-3012 | `BreakOutsideLoop` | Break outside loop | Move break inside loop |
| COL-3013 | `ContinueOutsideLoop` | Continue outside loop | Move continue inside loop |
| COL-3014 | `ReturnOutsideFunc` | Return outside function | Move return inside function |

### 5.9.5 Type Errors (COL-4xxx)

| Code | Name | Description | Remediation |
|------|------|-------------|-------------|
| COL-4001 | `TypeMismatch` | Value type doesn't match expected | Convert value or fix type |
| COL-4002 | `NotIterable` | Value cannot be iterated | Use iterable type |
| COL-4003 | `NotIndexable` | Value cannot be indexed | Use indexable type |
| COL-4004 | `NotCallable` | Value cannot be called | Use function/proc |
| COL-4005 | `DivisionByZero` | Division by zero attempted | Check divisor |
| COL-4006 | `IndexOutOfBounds` | Index exceeds bounds | Check array length |
| COL-4007 | `FieldNotFound` | Object field doesn't exist | Check field name |
| COL-4008 | `ReadOnlyVariable` | Cannot modify readonly var | Use different variable |
| COL-4009 | `TypeCoercionFailed` | Cannot coerce value | Check input format |
| COL-4010 | `InvalidOperation` | Operation not valid for type | Check operator usage |
| COL-4011 | `NullAccess` | Accessed null value | Check for null first |

### 5.9.6 I/O Errors (COL-5xxx)

| Code | Name | Description | Remediation |
|------|------|-------------|-------------|
| COL-5001 | `FileNotFound` | File doesn't exist | Check path |
| COL-5002 | `DirectoryNotFound` | Directory doesn't exist | Create directory |
| COL-5003 | `NotAFile` | Path is not a regular file | Use correct path type |
| COL-5004 | `NotADirectory` | Path is not a directory | Use correct path type |
| COL-5005 | `RedirectFailed` | Redirection failed | Check path permissions |
| COL-5006 | `HeredocError` | Here-document error | Check delimiter |
| COL-5007 | `BadFd` | Invalid file descriptor | Check fd number |
| COL-5008 | `FdNotOpen` | File descriptor not open | Open fd first |
| COL-5009 | `PipeClosed` | Pipe was closed | Check pipeline |
| COL-5010 | `IoError` | Generic I/O error | Check errno |

### 5.9.7 Agent Errors (COL-6xxx)

| Code | Name | Description | Remediation |
|------|------|-------------|-------------|
| COL-6001 | `AgentNotFound` | Agent not defined | Define agent first |
| COL-6002 | `AgentTimeout` | Agent execution timed out | Increase timeout |
| COL-6003 | `MaxIterations` | Agent exceeded max iterations | Increase limit or fix loop |
| COL-6004 | `ToolNotFound` | Requested tool not available | Register tool |
| COL-6005 | `ToolDenied` | Tool execution denied | Approve tool or check permissions |
| COL-6006 | `LlmError` | LLM API error | Check API key and quota |
| COL-6007 | `McpError` | MCP server error | Check server status |
| COL-6008 | `PromptError` | Prompt template error | Check template syntax |
| COL-6009 | `SessionNotFound` | Agent session not found | Create session first |
| COL-6010 | `AgentAborted` | Agent was aborted | Check abort reason |

### 5.9.8 Internal Errors (COL-7xxx)

| Code | Name | Description | Remediation |
|------|------|-------------|-------------|
| COL-7001 | `InternalError` | Unexpected internal error | Report bug |
| COL-7002 | `StackOverflow` | Evaluation stack overflow | Reduce nesting |
| COL-7003 | `OutOfMemory` | Memory allocation failed | Reduce memory usage |
| COL-7004 | `InvalidState` | Invalid internal state | Report bug |

### 5.9.9 Error Struct Extension

```rust
#[derive(Debug, thiserror::Error)]
pub struct ColonyError {
    pub code: ErrorCode,
    pub message: String,
    pub span: Option<Span>,
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
    pub help: Option<String>,
}

impl ColonyError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            span: None,
            source: None,
            help: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl std::fmt::Display for ColonyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(ref help) = self.help {
            write!(f, "\n  help: {}", help)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ErrorCode(u16);

impl ErrorCode {
    pub const COMMAND_NOT_FOUND: ErrorCode = ErrorCode(3001);
    pub const UNDEFINED_VARIABLE: ErrorCode = ErrorCode(2002);
    pub const TYPE_MISMATCH: ErrorCode = ErrorCode(4001);
    // ... etc
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "COL-{:04}", self.0)
    }
}
```

### 5.9.10 Error Display Examples

```
error[COL-3001]: Command not found: 'gitf': No such file or directory
  --> script.sh:15:1
   |
15 | gitf status
   | ^^^^
   |
  help: did you mean 'git'?

error[COL-2002]: Required variable is unset: API_KEY
  --> deploy.sh:23:12
   |
23 | curl -H "Authorization: $API_KEY" ...
   |            ^^^^^^^^
   |
  help: set variable with 'export API_KEY=...' or use default '${API_KEY:-default}'

error[COL-4005]: Division by zero attempted
  --> math.sh:8:10
   |
 8 | var result = 100 / $divisor
   |              ^^^^^^^^^^^^^^
   |
  help: check that divisor is not zero before dividing
```

---

*Continue to Part 6: Pipelines*