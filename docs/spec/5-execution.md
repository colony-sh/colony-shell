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

---

*Continue to Part 6: Pipelines*