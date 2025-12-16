; Highlighting queries for Colony Shell

; Keywords
[
  "if"
  "then"
  "elif"
  "else"
  "fi"
  "case"
  "in"
  "esac"
  "for"
  "do"
  "done"
  "while"
  "until"
  "function"
] @keyword

; Colony-specific keywords
[
  "var"
  "const"
  "setvar"
  "proc"
] @keyword

; Control flow
[
  "return"
  "break"
  "continue"
] @keyword.control

; Operators
[
  "&&"
  "||"
  "|"
  "->"
  "=>"
  "~>"
  "|&"
  ";"
  "&"
  "!"
] @operator

; Comparison operators
[
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
] @operator

; Arithmetic operators
[
  "+"
  "-"
  "*"
  "/"
  "%"
  "**"
] @operator

; Redirections
[
  ">"
  ">>"
  "<"
  ">&"
  "<&"
  "<<"
  "<<<"
] @operator

; Literals
(number) @number
(string) @string
(single_quoted_string) @string
(double_quoted_string) @string

[
  "true"
  "false"
  "null"
] @constant.builtin

; Variables
(variable_name) @variable
(variable_reference) @variable
(parameter_expansion) @variable

; Functions
(function_name) @function
(function_definition
  name: (function_name) @function)

; Commands
(command_name) @function.builtin

; Expansions
(tilde_expansion) @constant
(arithmetic_expansion) @number
(command_substitution) @embedded

; Comments
(comment) @comment

; Punctuation
[
  "("
  ")"
  "{"
  "}"
  "["
  "]"
] @punctuation.bracket

[
  "="
  ":="
  ":-"
  ":?"
  ":+"
] @punctuation.special

; File descriptors
(file_descriptor) @number
