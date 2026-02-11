# Part 15: Formal Grammar

## 15.1 Overview

This section provides the formal grammar for the Colony shell language in Extended Backus-Naur Form (EBNF).

## 15.2 Notation

```
=       Definition
|       Alternation
[ ]     Optional (0 or 1)
{ }     Repetition (0 or more)
( )     Grouping
' '     Terminal string
" "     Terminal string
;       Rule terminator
(* *)   Comment
```

## 15.3 Lexical Grammar

### 15.3.1 Characters

```ebnf
letter          = 'A'..'Z' | 'a'..'z' | '_' ;
digit           = '0'..'9' ;
hex_digit       = digit | 'A'..'F' | 'a'..'f' ;
oct_digit       = '0'..'7' ;
bin_digit       = '0' | '1' ;

whitespace      = ' ' | '\t' ;
newline         = '\n' | '\r\n' ;
line_continue   = '\\' newline ;

identifier      = letter { letter | digit } ;
bareword        = bareword_char { bareword_char } ;
bareword_char   = letter | digit | '-' | '_' | '.' | '/' | ':' | '@' | '~' ;
```

### 15.3.2 Numbers

```ebnf
integer         = decimal_int | hex_int | oct_int | bin_int ;
decimal_int     = digit { digit | '_' } ;
hex_int         = '0' ('x' | 'X') hex_digit { hex_digit | '_' } ;
oct_int         = '0' ('o' | 'O') oct_digit { oct_digit | '_' } ;
bin_int         = '0' ('b' | 'B') bin_digit { bin_digit | '_' } ;

float           = decimal_int '.' decimal_int [ exponent ]
                | decimal_int exponent ;
exponent        = ('e' | 'E') [ '+' | '-' ] decimal_int ;
```

### 15.3.3 Strings

```ebnf
string          = single_quoted | double_quoted | raw_string | triple_quoted ;

single_quoted   = "'" { sq_char } "'" ;
sq_char         = (* any char except ' *) | "''" ;

double_quoted   = '"' { dq_char | escape | expansion } '"' ;
dq_char         = (* any char except " \ $ ` *) ;

raw_string      = 'r' "'" { (* any char except ' *) } "'"
                | 'r' '"' { (* any char except " *) } '"' ;

triple_quoted   = "'''" { (* any char except ''' *) } "'''"
                | '"""' { (* any char except """ *) | expansion } '"""' ;

escape          = '\\' escape_char ;
escape_char     = 'n' | 't' | 'r' | '\\' | '"' | "'" | '$' | '`' | '0'
                | 'x' hex_digit hex_digit
                | 'u' hex_digit hex_digit hex_digit hex_digit ;
```

### 15.3.4 Expansions

```ebnf
expansion       = var_expansion | cmd_sub | expr_sub | arith_sub | proc_sub ;

var_expansion   = '$' identifier
                | '$' '{' var_expr '}' ;

var_expr        = identifier [ var_modifier ] ;

var_modifier    = ':' '-' word           (* default *)
                | ':' '=' word           (* assign *)
                | ':' '?' word           (* error *)
                | ':' '+' word           (* alternate *)
                | '#'                    (* length *)
                | '#' pattern            (* strip prefix *)
                | '##' pattern           (* strip prefix greedy *)
                | '%' pattern            (* strip suffix *)
                | '%%' pattern           (* strip suffix greedy *)
                | '/' pattern '/' word   (* replace first *)
                | '//' pattern '/' word  (* replace all *)
                | '^'                    (* uppercase first *)
                | '^^'                   (* uppercase all *)
                | ','                    (* lowercase first *)
                | ',,'                   (* lowercase all *)
                | '[' slice ']' ;        (* slice *)

slice           = expr [ ':' expr ] ;

cmd_sub         = '$' '(' command_list ')'
                | '`' command_list '`' ;

expr_sub        = '$' '[' expression ']' ;

arith_sub       = '$' '(' '(' expression ')' ')' ;

proc_sub        = '<' '(' command_list ')'
                | '>' '(' command_list ')' ;
```

### 15.3.5 Comments

```ebnf
comment         = '#' { (* any char except newline *) } ;
block_comment   = '/#' { (* any char except #/ *) } '#/' ;
```

### 15.3.6 Operators

```ebnf
(* Pipeline operators *)
pipe_op         = '|' | '|>' | '?>' | '!>' | '->' | '=>' | '~>' | '|&' ;

(* List operators *)
list_op         = '&&' | '||' | ';' | '&' ;

(* Redirect operators *)
redirect_op     = '<' | '>' | '>>' | '>&' | '<&' | '>|' | '<>'
                | '<<' | '<<-' | '<<<' ;

(* Arithmetic operators *)
arith_op        = '+' | '-' | '*' | '/' | '%' | '**'
                | '&' | '|' | '^' | '~' | '<<' | '>>' ;

(* Comparison operators *)
compare_op      = '==' | '!=' | '<' | '<=' | '>' | '>='
                | '=~' | '!~' | '~~' ;

(* Logical operators *)
logic_op        = '&&' | '||' | '!' ;

(* Assignment operators *)
assign_op       = '=' | '+=' | '-=' | '*=' | '/=' | '%='
                | '&=' | '|=' | '^=' | '<<=' | '>>=' | '??=' ;
```

## 15.4 Syntactic Grammar

### 15.4.1 Program Structure

```ebnf
program         = { statement } ;

statement       = simple_command
                | pipeline
                | command_list
                | compound_command
                | function_def
                | proc_def
                | agent_def
                | var_decl
                | assignment
                | comment ;

terminator      = newline | ';' ;
```

### 15.4.2 Commands

```ebnf
simple_command  = [ env_assignment { env_assignment } ]
                  command_name { word } { redirect } ;

env_assignment  = identifier '=' word ;

command_name    = word ;

word            = bareword
                | string
                | expansion
                | brace_expansion
                | glob_pattern
                | '~' [ identifier ] ;
```

### 15.4.3 Pipelines

```ebnf
pipeline        = ['!'] pipeline_element { pipe_op pipeline_element } ;

pipeline_element = simple_command | compound_command | '(' pipeline ')' ;
```

### 15.4.4 Command Lists

```ebnf
command_list    = and_or_list { list_op and_or_list } ;

and_or_list     = pipeline { ('&&' | '||') pipeline } ;
```

### 15.4.5 Compound Commands

```ebnf
compound_command = brace_group
                 | subshell
                 | if_statement
                 | case_statement
                 | for_statement
                 | while_statement
                 | until_statement
                 | match_statement
                 | try_statement ;

brace_group     = '{' command_list '}' ;

subshell        = '(' command_list ')' ;
```

### 15.4.6 If Statement

```ebnf
if_statement    = 'if' condition block
                  { 'elif' condition block }
                  [ 'else' block ] ;

condition       = expression | command_list ;

block           = '{' { statement } '}' ;
```

### 15.4.7 Case Statement

```ebnf
case_statement  = 'case' word 'in'
                  { case_item }
                  'esac' ;

case_item       = pattern { '|' pattern } ')' { statement } ';;' ;
```

### 15.4.8 For Statement

```ebnf
for_statement   = 'for' identifier 'in' word_list block
                | 'for' identifier ',' identifier 'in' 'enumerate' '(' expression ')' block
                | 'for' '(' [ for_init ] ';' [ expression ] ';' [ for_update ] ')' block ;

for_init       = var_decl | assignment ;
for_update     = assignment | expression ;

word_list      = { word } ;
```

### 15.4.9 While/Until Statement

```ebnf
while_statement = 'while' condition block ;

until_statement = 'until' condition block ;
```

### 15.4.10 Match Statement

```ebnf
match_statement = 'match' expression '{'
                  { match_arm }
                  '}' ;

match_arm       = pattern [ 'if' expression ] '=>' (expression | block) ;
```

### 15.4.11 Try Statement

```ebnf
try_statement   = 'try' block
                  { 'catch' [ pattern ] block }
                  [ 'finally' block ] ;
```

### 15.4.12 Function Definition

```ebnf
function_def    = 'function' identifier '(' ')' '{' { statement } '}'
                | identifier '(' ')' '{' { statement } '}' ;

fn_def          = 'fn' identifier '(' [ param_list ] ')' [ '->' type ] block_or_expr ;

block_or_expr   = block | expression ;

proc_def        = 'proc' identifier '(' [ param_list ] ')' [ pipeline_type ] block ;

pipeline_type   = '[' type '->' type ']' ;

param_list      = param { ',' param } [ ',' ] ;

param           = identifier [ ':' type ] [ '=' expression ] ;
```

### 15.4.13 Agent Definition

```ebnf
agent_def       = 'agent' identifier '{'
                  { agent_field }
                  '}' ;

agent_field     = 'description' ':' string
                | 'param' identifier ':' type [ '=' expression ]
                | 'tools' ':' '[' identifier_list ']'
                | 'prompt' ':' string
                | 'on_start' block
                | 'on_complete' '(' identifier ')' block
                | 'on_tool_call' '(' identifier ',' identifier ')' block
                | 'on_error' '(' identifier ')' block
                | agent_capability ;

agent_capability = 'model' ':' string
                 | 'max_tokens' ':' integer
                 | 'temperature' ':' float
                 | 'timeout' ':' duration
                 | 'context' ':' identifier ;

identifier_list = identifier { ',' identifier } [ ',' ] ;
```

### 15.4.14 Variable Declaration

```ebnf
var_decl        = 'var' identifier [ ':' type ] [ '=' expression ]
                | 'const' identifier [ ':' type ] '=' expression ;

assignment      = lvalue assign_op expression ;

lvalue          = identifier
                | identifier '[' expression ']'
                | identifier '.' identifier ;
```

### 15.4.15 Redirections

```ebnf
redirect        = redirect_op word
                | fd redirect_op word
                | fd redirect_op fd
                | here_doc ;

fd              = digit { digit } ;

here_doc        = '<<' [ '-' ] delimiter newline
                  { (* lines until delimiter *) }
                  delimiter ;

here_string     = '<<<' word ;
```

### 15.4.16 Expressions

```ebnf
expression      = ternary_expr ;

ternary_expr    = null_coalesce_expr [ '?' expression ':' expression ] ;

null_coalesce_expr = or_expr { '??' or_expr } ;

or_expr         = and_expr { '||' and_expr } ;

and_expr        = equality_expr { '&&' equality_expr } ;

equality_expr   = comparison_expr { ('==' | '!=' | '=~' | '!~' | '~~') comparison_expr } ;

comparison_expr = bitwise_or_expr { ('<' | '<=' | '>' | '>=') bitwise_or_expr } ;

bitwise_or_expr = bitwise_xor_expr { '|' bitwise_xor_expr } ;

bitwise_xor_expr = bitwise_and_expr { '^' bitwise_and_expr } ;

bitwise_and_expr = shift_expr { '&' shift_expr } ;

shift_expr      = additive_expr { ('<<' | '>>') additive_expr } ;

additive_expr   = multiplicative_expr { ('+' | '-') multiplicative_expr } ;

multiplicative_expr = power_expr { ('*' | '/' | '%') power_expr } ;

power_expr      = unary_expr { '**' unary_expr } ;

unary_expr      = '-' unary_expr
                | '!' unary_expr
                | '~' unary_expr
                | postfix_expr ;

postfix_expr    = primary_expr { postfix_op } ;

postfix_op      = '[' expression ']'           (* index *)
                | '.' identifier               (* field *)
                | '(' [ arg_list ] ')'         (* call *)
                | '?'                          (* optional chain *)
                | '!' ;                        (* unwrap *)

primary_expr    = 'null'
                | 'true' | 'false'
                | integer
                | float
                | string
                | identifier
                | list_expr
                | dict_expr
                | tuple_expr
                | range_expr
                | lambda_expr
                | '(' expression ')' ;

list_expr       = '[' [ expression { ',' expression } [ ',' ] ] ']' ;

dict_expr       = '{' [ dict_entry { ',' dict_entry } [ ',' ] ] '}' ;

dict_entry      = (identifier | string | '[' expression ']') ':' expression ;

tuple_expr      = '(' expression ',' [ expression { ',' expression } ] [ ',' ] ')' ;

range_expr      = expression '..' [ '=' ] expression [ '..' expression ] ;

lambda_expr     = '|' [ param_list ] '|' (expression | block) ;

arg_list        = expression { ',' expression } [ ',' ] ;
```

### 15.4.17 Types

```ebnf
type            = simple_type
                | generic_type
                | union_type
                | optional_type
                | tuple_type
                | function_type
                | pipeline_type_annot ;

simple_type     = 'Null' | 'Bool' | 'Int' | 'Float' | 'Str' | 'Bytes'
                | 'Any' | 'Never' | identifier ;

generic_type    = identifier '[' type { ',' type } ']' ;

union_type      = type '|' type ;

optional_type   = type '?' ;

tuple_type      = '(' type { ',' type } ')' ;

function_type   = '(' [ type { ',' type } ] ')' '->' type ;

pipeline_type_annot = type '->' type ;
```

### 15.4.18 Patterns

```ebnf
pattern         = literal_pattern
                | identifier_pattern
                | wildcard_pattern
                | list_pattern
                | dict_pattern
                | tuple_pattern
                | type_pattern
                | or_pattern
                | guard_pattern ;

literal_pattern = integer | float | string | 'true' | 'false' | 'null' ;

identifier_pattern = identifier ;

wildcard_pattern = '_' ;

list_pattern    = '[' [ pattern { ',' pattern } [ ',' '..' [ identifier ] ] ] ']' ;

dict_pattern    = '{' [ dict_pattern_entry { ',' dict_pattern_entry } [ ',' '..' ] ] '}' ;

dict_pattern_entry = identifier [ ':' pattern ] ;

tuple_pattern   = '(' pattern { ',' pattern } ')' ;

type_pattern    = identifier ':' type ;

or_pattern      = pattern '|' pattern ;

guard_pattern   = pattern 'if' expression ;
```

### 15.4.19 Glob Patterns

```ebnf
glob_pattern    = { glob_element } ;

glob_element    = '*'                          (* match any *)
                | '?'                          (* match one *)
                | '[' char_class ']'           (* character class *)
                | '[!' char_class ']'          (* negated class *)
                | '**'                         (* recursive *)
                | ext_glob
                | bareword_char ;

char_class      = { char_range | bareword_char } ;
char_range      = bareword_char '-' bareword_char ;

ext_glob        = '?(' pattern_list ')'        (* zero or one *)
                | '*(' pattern_list ')'        (* zero or more *)
                | '+(' pattern_list ')'        (* one or more *)
                | '@(' pattern_list ')'        (* exactly one *)
                | '!(' pattern_list ')' ;      (* none of *)

pattern_list    = glob_pattern { '|' glob_pattern } ;
```

### 15.4.20 Brace Expansion

```ebnf
brace_expansion = '{' brace_item { ',' brace_item } '}'
                | '{' sequence '}' ;

brace_item      = word ;

sequence        = integer '..' integer [ '..' integer ]
                | letter '..' letter [ '..' integer ] ;
```

## 15.5 Operator Precedence

From highest to lowest:

| Precedence | Operators | Associativity |
|------------|-----------|---------------|
| 1 | `()` `[]` `.` `?.` | Left |
| 2 | `!` `~` `-` (unary) | Right |
| 3 | `**` | Right |
| 4 | `*` `/` `%` | Left |
| 5 | `+` `-` | Left |
| 6 | `<<` `>>` | Left |
| 7 | `&` | Left |
| 8 | `^` | Left |
| 9 | `\|` (bitwise) | Left |
| 10 | `<` `<=` `>` `>=` | Left |
| 11 | `==` `!=` `=~` `!~` `~~` | Left |
| 12 | `&&` | Left |
| 13 | `\|\|` | Left |
| 14 | `??` | Left |
| 15 | `? :` | Right |
| 16 | `=` `+=` `-=` etc. | Right |

## 15.6 Reserved Words

```
if elif else fi
for in do done
while until
case esac
match
try catch finally
fn proc agent
var const setvar
export local readonly
return break continue
true false null
and or not
```

## 15.7 Lexer Mode Transitions

```
Mode        Trigger                 Exit
────────────────────────────────────────────────
Command     Default                 -
Expression  $[ $[[ [[ if/while cond Expression end
HereDoc     << <<-                  Delimiter line
Regex       =~ !~                   Unquoted whitespace
Arithmetic  $(( ))                  ))
```

---

*End of Colony Shell Implementation Specification*
