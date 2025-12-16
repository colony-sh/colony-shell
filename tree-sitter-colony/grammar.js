/**
 * Tree-sitter grammar for Colony Shell
 *
 * Supports:
 * - POSIX shell syntax (simple commands, pipelines, lists)
 * - Colony extensions (typed pipes, var/const, procs)
 * - Incremental parsing and error recovery
 */

const PREC = {
  COMMENT: 1,
  PIPELINE: 2,
  AND_OR: 3,
  BACKGROUND: 4,
  SEQUENCE: 5,
  REDIRECT: 6,
  COMMAND: 7,
};

module.exports = grammar({
  name: 'colony',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  externals: $ => [
    $._newline,
    $._heredoc_body_start,
    $._heredoc_body_middle,
    $._heredoc_body_end,
  ],

  word: $ => $.word,

  rules: {
    program: $ => repeat($._statement),

    _statement: $ => choice(
      $.command_list,
      $.pipeline,
      $.simple_command,
      $.compound_command,
      $.function_definition,
      $.variable_declaration,
    ),

    // ===== Commands =====

    command_list: $ => prec.left(PREC.AND_OR, seq(
      $._statement,
      choice(
        seq('&&', $._statement),
        seq('||', $._statement),
        seq(';', optional($._statement)),
        seq('&', optional($._statement)),
      ),
    )),

    pipeline: $ => prec.left(PREC.PIPELINE, seq(
      optional('!'),
      $._pipeline_element,
      repeat1(seq(
        choice('|', '->', '=>', '~>', '|&'),
        $._pipeline_element,
      )),
    )),

    _pipeline_element: $ => choice(
      $.simple_command,
      $.compound_command,
    ),

    simple_command: $ => prec.right(PREC.COMMAND, seq(
      repeat($.variable_assignment),
      choice(
        seq(
          $.command_name,
          repeat($.argument),
        ),
        repeat1($.variable_assignment),
      ),
      repeat($.redirection),
    )),

    command_name: $ => $.word,

    argument: $ => choice(
      $.word,
      $.string,
      $.expansion,
      $.command_substitution,
    ),

    // ===== Compound Commands =====

    compound_command: $ => choice(
      $.if_statement,
      $.case_statement,
      $.for_statement,
      $.while_statement,
      $.until_statement,
      $.subshell,
      $.brace_group,
    ),

    if_statement: $ => seq(
      'if',
      $._statement,
      'then',
      repeat($._statement),
      repeat($.elif_clause),
      optional($.else_clause),
      'fi',
    ),

    elif_clause: $ => seq(
      'elif',
      $._statement,
      'then',
      repeat($._statement),
    ),

    else_clause: $ => seq(
      'else',
      repeat($._statement),
    ),

    case_statement: $ => seq(
      'case',
      $.word,
      'in',
      repeat($.case_item),
      'esac',
    ),

    case_item: $ => seq(
      optional('('),
      $.word,
      repeat(seq('|', $.word)),
      ')',
      repeat($._statement),
      choice(';;', ';&', ';;&'),
    ),

    for_statement: $ => seq(
      'for',
      $.variable_name,
      optional(seq(
        'in',
        repeat($.word),
      )),
      'do',
      repeat($._statement),
      'done',
    ),

    while_statement: $ => seq(
      'while',
      $._statement,
      'do',
      repeat($._statement),
      'done',
    ),

    until_statement: $ => seq(
      'until',
      $._statement,
      'do',
      repeat($._statement),
      'done',
    ),

    subshell: $ => seq(
      '(',
      repeat($._statement),
      ')',
    ),

    brace_group: $ => seq(
      '{',
      repeat($._statement),
      '}',
    ),

    // ===== Function Definition =====

    function_definition: $ => seq(
      optional('function'),
      $.function_name,
      '(',
      ')',
      choice($.compound_command, $.brace_group),
    ),

    function_name: $ => $.word,

    // ===== Colony Extensions =====

    variable_declaration: $ => seq(
      choice('var', 'const', 'setvar'),
      $.variable_name,
      '=',
      $.expression,
    ),

    expression: $ => choice(
      $.literal,
      $.variable_reference,
      $.binary_expression,
      $.unary_expression,
      $.parenthesized_expression,
    ),

    literal: $ => choice(
      $.number,
      $.string,
      'null',
      'true',
      'false',
    ),

    binary_expression: $ => choice(
      prec.left(1, seq($.expression, '||', $.expression)),
      prec.left(2, seq($.expression, '&&', $.expression)),
      prec.left(3, seq($.expression, choice('==', '!='), $.expression)),
      prec.left(4, seq($.expression, choice('<', '<=', '>', '>='), $.expression)),
      prec.left(5, seq($.expression, choice('+', '-'), $.expression)),
      prec.left(6, seq($.expression, choice('*', '/', '%'), $.expression)),
      prec.right(7, seq($.expression, '**', $.expression)),
    ),

    unary_expression: $ => prec.right(8, seq(
      choice('-', '!', 'not'),
      $.expression,
    )),

    parenthesized_expression: $ => seq(
      '(',
      $.expression,
      ')',
    ),

    // ===== Expansions =====

    expansion: $ => choice(
      $.parameter_expansion,
      $.arithmetic_expansion,
      $.tilde_expansion,
    ),

    parameter_expansion: $ => seq(
      '${',
      $.variable_name,
      optional(seq(
        choice(':-', '-', ':=', '=', ':?', '?', ':+', '+'),
        $.word,
      )),
      '}',
    ),

    arithmetic_expansion: $ => seq(
      '$((',
      repeat(choice($.word, $.number, /[+\-*\/%()]/)),
      '))',
    ),

    tilde_expansion: $ => seq(
      '~',
      optional($.word),
    ),

    variable_reference: $ => seq(
      '$',
      choice(
        $.variable_name,
        seq('{', $.variable_name, '}'),
      ),
    ),

    command_substitution: $ => choice(
      seq('$(', repeat($._statement), ')'),
      seq('`', repeat(/[^`]/), '`'),
    ),

    // ===== Redirections =====

    redirection: $ => prec.right(PREC.REDIRECT, seq(
      optional($.file_descriptor),
      choice(
        seq('>', $.word),
        seq('>>', $.word),
        seq('<', $.word),
        seq('>&', choice($.word, $.file_descriptor)),
        seq('<&', choice($.word, $.file_descriptor)),
        seq('<<', $.heredoc_start),
        seq('<<<', $.word),
      ),
    )),

    heredoc_start: $ => $.word,

    file_descriptor: $ => /[0-9]+/,

    // ===== Primitives =====

    variable_assignment: $ => seq(
      $.variable_name,
      '=',
      optional(choice($.word, $.string, $.expansion)),
    ),

    variable_name: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    word: $ => /[a-zA-Z0-9_\-\.\/]+/,

    number: $ => choice(
      /[0-9]+/,
      /[0-9]+\.[0-9]+/,
    ),

    string: $ => choice(
      $.single_quoted_string,
      $.double_quoted_string,
    ),

    single_quoted_string: $ => seq(
      "'",
      repeat(choice(
        /[^']/,
      )),
      "'",
    ),

    double_quoted_string: $ => seq(
      '"',
      repeat(choice(
        /[^"$`\\]/,
        $.variable_reference,
        $.command_substitution,
        $.escape_sequence,
      )),
      '"',
    ),

    escape_sequence: $ => token(seq('\\', /./))),

    comment: $ => token(prec(PREC.COMMENT, seq('#', /.*/))),
  },
});
