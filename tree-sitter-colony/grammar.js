/**
 * Tree-sitter grammar for Colony Shell
 * 
 * Colony is a POSIX-compatible shell with extensions for:
 * - Extended I/O channels (beyond 0,1,2)
 * - Named channels (@name)
 * - Buffer references ($[n] or $[name])
 * - Tuple space operations (out, in, rd, inp, rdp)
 * - Agent invocation and piping
 * - Reactive bindings
 * - Capability-based security
 */

module.exports = grammar({
  name: 'colony',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  externals: $ => [
    $.heredoc_start,
    $.heredoc_body,
    $.heredoc_end,
    $._newline,
    $._concat,  // For adjacent word concatenation
  ],

  word: $ => $.word,

  conflicts: $ => [
    [$.command, $.variable_assignment],
    [$.redirected_statement, $.command],
    [$.channel_number, $.file_descriptor],
    [$.function_definition, $._literal],
  ],

  supertypes: $ => [
    $._statement,
    $._expression,
    $._primary_expression,
  ],

  rules: {
    program: $ => optional($._statements),

    _statements: $ => prec(1, seq(
      repeat(seq(
        $._statement,
        $._terminator,
      )),
      $._statement,
      optional($._terminator),
    )),

    _terminator: $ => choice(
      ';',
      ';;',
      $._newline,
      '&',
    ),

    // ========================================
    // STATEMENTS
    // ========================================

    _statement: $ => choice(
      $.redirected_statement,
      $._compound_statement,
      $.variable_assignment,
      $.command,
      $.declaration_command,
      $.negated_command,
      $.pipeline,
      $.list,
      $.subshell,
      $.function_definition,
      // Colony extensions
      $.tuple_operation,
      $.agent_command,
      $.reactive_binding,
      $.capability_block,
      $.watch_statement,
    ),

    redirected_statement: $ => prec.dynamic(-1, seq(
      field('body', $._statement),
      repeat1(choice(
        $.file_redirect,
        $.heredoc_redirect,
        $.herestring_redirect,
        // Colony: extended channel redirects
        $.extended_channel_redirect,
        $.named_channel_redirect,
        $.buffer_redirect,
      )),
    )),

    // ========================================
    // CONTROL FLOW (POSIX)
    // ========================================

    _compound_statement: $ => choice(
      $.if_statement,
      $.while_statement,
      $.for_statement,
      $.case_statement,
      $.compound_statement,
    ),

    if_statement: $ => seq(
      'if',
      field('condition', $._statements),
      'then',
      field('consequence', optional($._statements)),
      repeat($.elif_clause),
      optional($.else_clause),
      'fi',
    ),

    elif_clause: $ => seq(
      'elif',
      field('condition', $._statements),
      'then',
      field('consequence', optional($._statements)),
    ),

    else_clause: $ => seq(
      'else',
      optional($._statements),
    ),

    while_statement: $ => seq(
      choice('while', 'until'),
      field('condition', $._statements),
      'do',
      field('body', optional($._statements)),
      'done',
    ),

    for_statement: $ => seq(
      'for',
      field('variable', $.variable_name),
      optional(seq(
        'in',
        field('value', repeat1($._literal)),
      )),
      $._terminator,
      'do',
      field('body', optional($._statements)),
      'done',
    ),

    case_statement: $ => seq(
      'case',
      field('value', $._literal),
      optional($._terminator),
      'in',
      optional($._terminator),
      repeat($.case_item),
      'esac',
    ),

    case_item: $ => seq(
      optional('('),
      field('value', seq($._literal, repeat(seq('|', $._literal)))),
      ')',
      optional($._statements),
      prec(1, choice(';;', choice(';&', ';;&'))),
    ),

    compound_statement: $ => seq(
      '{',
      optional($._statements),
      '}',
    ),

    subshell: $ => seq(
      '(',
      $._statements,
      ')',
    ),

    // ========================================
    // PIPELINES AND LISTS
    // ========================================

    pipeline: $ => prec.left(1, seq(
      choice(
        seq(
          $._statement,
          choice('|', '|&'),
          $._statement,
        ),
        // Colony: extended channel pipe
        seq(
          $._statement,
          $.extended_pipe,
          $._statement,
        ),
        // Colony: agent pipe
        seq(
          $._statement,
          '|>',
          $.agent_command,
        ),
      ),
    )),

    // Colony: Extended pipe syntax (3|4 means connect fd 3 to fd 4)
    extended_pipe: $ => seq(
      field('from_channel', $.channel_number),
      '|',
      field('to_channel', $.channel_number),
    ),

    list: $ => prec.left(-1, seq(
      $._statement,
      choice('&&', '||'),
      $._statement,
    )),

    negated_command: $ => seq(
      '!',
      choice(
        $.command,
        $.subshell,
        $.compound_statement,
      ),
    ),

    // ========================================
    // COMMANDS
    // ========================================

    command: $ => prec.left(seq(
      repeat(choice(
        $.variable_assignment,
        $.file_redirect,
        $.extended_channel_redirect,
        $.named_channel_redirect,
        $.buffer_redirect,
      )),
      field('name', $.command_name),
      repeat(choice(
        field('argument', $._literal),
        seq('=', field('argument', $._literal)),
        $.file_redirect,
        $.extended_channel_redirect,
        $.named_channel_redirect,
        $.buffer_redirect,
      )),
    )),

    command_name: $ => $._literal,

    declaration_command: $ => seq(
      choice('declare', 'typeset', 'export', 'readonly', 'local'),
      repeat(choice(
        $._literal,
        $.variable_assignment,
      )),
    ),

    variable_assignment: $ => seq(
      field('name', choice(
        $.variable_name,
        $.subscript,
      )),
      choice(
        '=',
        '+=',
      ),
      field('value', optional(choice(
        $._literal,
        $.array,
      ))),
    ),

    subscript: $ => seq(
      field('name', $.variable_name),
      '[',
      field('index', $._literal),
      ']',
    ),

    function_definition: $ => seq(
      optional('function'),
      field('name', $.word),
      optional(seq('(', ')')),
      field('body', $.compound_statement),
    ),

    // ========================================
    // COLONY: AGENT COMMANDS
    // ========================================

    agent_command: $ => seq(
      'agent',
      repeat($.agent_flag),
      field('prompt', choice(
        $.string,
        $.raw_string,
        $.agent_block,
      )),
    ),

    agent_flag: $ => choice(
      seq('-m', field('model', $.word)),
      seq('--model', '=', field('model', $.word)),
      seq('--caps', '=', field('capabilities', $.capability_list)),
      seq('--timeout', '=', field('timeout', $.number)),
      seq('-v', ),  // verbose
      seq('--stream'),
    ),

    agent_block: $ => seq(
      '{',
      repeat(choice(
        $.agent_block_content,
        $.expansion,
        $.command_substitution,
      )),
      '}',
    ),

    agent_block_content: $ => /[^{}$]+/,

    // ========================================
    // COLONY: TUPLE SPACE OPERATIONS
    // ========================================

    tuple_operation: $ => choice(
      $.tuple_out,
      $.tuple_in,
      $.tuple_rd,
      $.tuple_inp,
      $.tuple_rdp,
    ),

    // out { key: value, ... } - Write tuple to space
    tuple_out: $ => seq(
      'out',
      optional(field('space', $.tuple_space_name)),
      field('tuple', $.tuple_literal),
    ),

    // in { pattern } - Blocking read and remove
    tuple_in: $ => seq(
      'in',
      optional(field('space', $.tuple_space_name)),
      field('pattern', $.tuple_pattern),
      optional(field('timeout', $.timeout_clause)),
    ),

    // rd { pattern } - Blocking read without remove
    tuple_rd: $ => seq(
      'rd',
      optional(field('space', $.tuple_space_name)),
      field('pattern', $.tuple_pattern),
      optional(field('timeout', $.timeout_clause)),
    ),

    // inp { pattern } - Non-blocking in
    tuple_inp: $ => seq(
      'inp',
      optional(field('space', $.tuple_space_name)),
      field('pattern', $.tuple_pattern),
    ),

    // rdp { pattern } - Non-blocking rd
    tuple_rdp: $ => seq(
      'rdp',
      optional(field('space', $.tuple_space_name)),
      field('pattern', $.tuple_pattern),
    ),

    tuple_space_name: $ => seq(
      '@',
      $.word,
    ),

    timeout_clause: $ => seq(
      'timeout',
      field('duration', $.number),
      optional(field('unit', choice('s', 'ms', 'm'))),
    ),

    // Tuple literal: { key: value, key2: value2 }
    tuple_literal: $ => seq(
      '{',
      optional(seq(
        $.tuple_field,
        repeat(seq(',', $.tuple_field)),
        optional(','),
      )),
      '}',
    ),

    tuple_field: $ => seq(
      field('key', choice($.word, $.string)),
      ':',
      field('value', $._expression),
    ),

    // Tuple pattern: { key: ?binding, key2: *, key3: literal }
    tuple_pattern: $ => seq(
      '{',
      optional(seq(
        $.pattern_field,
        repeat(seq(',', $.pattern_field)),
        optional(','),
      )),
      '}',
    ),

    pattern_field: $ => seq(
      field('key', choice($.word, $.string, $.pattern_wildcard)),
      ':',
      field('pattern', choice(
        $.pattern_binding,     // ?var - bind to variable
        $.pattern_wildcard,    // * - match anything
        $._expression,         // literal match
      )),
    ),

    pattern_binding: $ => seq(
      '?',
      field('name', $.variable_name),
    ),

    pattern_wildcard: $ => '*',

    // ========================================
    // COLONY: BUFFER REFERENCES
    // ========================================

    buffer_reference: $ => seq(
      '$[',
      field('buffer', choice(
        $.number,           // $[0], $[1]
        $.word,             // $[ctx], $[result]
      )),
      ']',
    ),

    buffer_redirect: $ => choice(
      // > $[n] - write to buffer
      seq(
        field('fd', optional($.file_descriptor)),
        choice('>', '>>'),
        field('buffer', $.buffer_reference),
      ),
      // < $[n] - read from buffer
      seq(
        field('fd', optional($.file_descriptor)),
        '<',
        field('buffer', $.buffer_reference),
      ),
    ),

    // ========================================
    // COLONY: EXTENDED CHANNEL REDIRECTS
    // ========================================

    channel_number: $ => /[0-9]+/,

    // 3> file, 4>> file, 5< file
    extended_channel_redirect: $ => seq(
      field('channel', $.channel_number),
      field('operator', choice('>', '>>', '<', '<>', '>&', '<&')),
      field('destination', choice(
        $._literal,
        $.buffer_reference,
        $.channel_number,  // 3>&4
      )),
    ),

    // @name> file - named channel redirect
    named_channel_redirect: $ => seq(
      '@',
      field('channel_name', $.word),
      field('operator', choice('>', '>>', '<', '<>')),
      field('destination', choice(
        $._literal,
        $.buffer_reference,
      )),
    ),

    // ========================================
    // COLONY: REACTIVE BINDINGS
    // ========================================

    reactive_binding: $ => seq(
      'let!',
      field('name', $.variable_name),
      '=',
      field('expression', choice(
        $.command_substitution,
        $._expression,
      )),
    ),

    watch_statement: $ => seq(
      'watch',
      field('target', choice(
        $.buffer_reference,
        $.simple_expansion,
      )),
      field('body', $.compound_statement),
    ),

    // ========================================
    // COLONY: CAPABILITY BLOCKS
    // ========================================

    capability_block: $ => seq(
      'with',
      'caps',
      '(',
      field('capabilities', $.capability_list),
      ')',
      field('body', $.compound_statement),
    ),

    capability_list: $ => seq(
      $.capability,
      repeat(seq(',', $.capability)),
    ),

    capability: $ => seq(
      field('type', choice('fs', 'net', 'env', 'exec', 'agent')),
      optional(seq(
        ':',
        field('scope', $.capability_scope),
      )),
    ),

    capability_scope: $ => choice(
      $.word,
      $.string,
      $.glob_pattern,
    ),

    glob_pattern: $ => /[a-zA-Z0-9_.*?\[\]\/:-]+/,

    // ========================================
    // STANDARD REDIRECTIONS
    // ========================================

    file_redirect: $ => prec.left(seq(
      field('fd', optional($.file_descriptor)),
      choice(
        seq(
          field('operator', choice('<', '>', '>>', '&>', '&>>', '<&', '>&', '>|')),
          field('destination', $._literal),
        ),
        seq(
          field('operator', choice('<<<')),
          field('destination', $._literal),
        ),
      ),
    )),

    file_descriptor: $ => /[0-9]+/,

    heredoc_redirect: $ => seq(
      field('operator', choice('<<', '<<-')),
      $.heredoc_start,
      /\n/,
      optional($.heredoc_body),
      $.heredoc_end,
    ),

    herestring_redirect: $ => seq(
      '<<<',
      $._literal,
    ),

    // ========================================
    // EXPRESSIONS
    // ========================================

    _expression: $ => choice(
      $._primary_expression,
      $.binary_expression,
      $.unary_expression,
      $.ternary_expression,
      $.parenthesized_expression,
    ),

    _primary_expression: $ => choice(
      $._literal,
      $.buffer_reference,
      $.tuple_literal,
    ),

    binary_expression: $ => choice(
      ...[
        ['||', 0],
        ['&&', 1],
        ['|', 2],
        ['^', 3],
        ['&', 4],
        ['==', 5], ['!=', 5],
        ['<', 6], ['>', 6], ['<=', 6], ['>=', 6],
        ['<<', 7], ['>>', 7],
        ['+', 8], ['-', 8],
        ['*', 9], ['/', 9], ['%', 9],
      ].map(([op, prec_val]) =>
        prec.left(prec_val, seq(
          field('left', $._expression),
          field('operator', op),
          field('right', $._expression),
        ))
      ),
    ),

    unary_expression: $ => prec.right(10, seq(
      field('operator', choice('!', '-', '+', '~')),
      field('argument', $._expression),
    )),

    ternary_expression: $ => prec.right(seq(
      field('condition', $._expression),
      '?',
      field('consequence', $._expression),
      ':',
      field('alternative', $._expression),
    )),

    parenthesized_expression: $ => seq(
      '(',
      $._expression,
      ')',
    ),

    // ========================================
    // LITERALS AND EXPANSIONS
    // ========================================

    _literal: $ => choice(
      $.concatenation,
      $.word,
      $.string,
      $.raw_string,
      $.ansii_c_string,
      $.number,
      $.expansion,
      $.simple_expansion,
      $.command_substitution,
      $.process_substitution,
      $.arithmetic_expansion,
      $.brace_expression,
      $.buffer_reference,
    ),

    concatenation: $ => prec(-1, seq(
      choice(
        $.word,
        $.string,
        $.raw_string,
        $.ansii_c_string,
        $.expansion,
        $.simple_expansion,
        $.command_substitution,
        $.process_substitution,
        $.buffer_reference,
      ),
      repeat1(seq(
        $._concat,
        choice(
          $.word,
          $.string,
          $.raw_string,
          $.ansii_c_string,
          $.expansion,
          $.simple_expansion,
          $.command_substitution,
          $.process_substitution,
          $.buffer_reference,
        ),
      )),
    )),

    word: $ => /[a-zA-Z_][a-zA-Z0-9_-]*/,

    number: $ => /[0-9]+(\.[0-9]+)?/,

    // Double-quoted string with expansions
    string: $ => seq(
      '"',
      repeat(choice(
        $.string_content,
        $.expansion,
        $.simple_expansion,
        $.command_substitution,
        $.arithmetic_expansion,
        $.buffer_reference,
      )),
      '"',
    ),

    string_content: $ => prec.right(repeat1(choice(
      /[^"$\\`]+/,
      /\\./,
    ))),

    // Single-quoted raw string
    raw_string: $ => seq(
      "'",
      optional(/[^']*/),
      "'",
    ),

    ansii_c_string: $ => seq(
      "$'",
      repeat(choice(
        /[^'\\]+/,
        /\\./,
      )),
      "'",
    ),

    // ${...} expansion
    expansion: $ => seq(
      '${',
      optional('#'),
      choice(
        seq(
          $.variable_name,
          optional(choice(
            seq(
              choice(':', ''),
              choice('-', '=', '+', '?'),
              optional($._literal),
            ),
            seq(
              choice('/', '//', '#', '##', '%', '%%'),
              optional($._literal),
            ),
            seq(
              ':',
              optional($._literal),
              optional(seq(':', optional($._literal))),
            ),
            seq(
              choice('@', '*'),
            ),
          )),
        ),
        $.command_substitution,
      ),
      '}',
    ),

    // $var expansion
    simple_expansion: $ => seq(
      '$',
      choice(
        $.variable_name,
        $.special_variable_name,
      ),
    ),

    variable_name: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    special_variable_name: $ => choice(
      /[0-9]+/,
      '$',
      '!',
      '@',
      '*',
      '#',
      '?',
      '-',
      '_',
    ),

    // $(command) or `command`
    command_substitution: $ => choice(
      seq('$(', $._statements, ')'),
      prec(-1, seq('`', /[^`]+/, '`')),
    ),

    // <(command) or >(command)
    process_substitution: $ => seq(
      choice('<(', '>('),
      $._statements,
      ')',
    ),

    // $((expression))
    arithmetic_expansion: $ => seq(
      '$((',
      $._expression,
      '))',
    ),

    // {a,b,c} or {1..10}
    brace_expression: $ => seq(
      '{',
      choice(
        seq($._literal, repeat1(seq(',', $._literal))),
        seq($.number, '..', $.number, optional(seq('..', $.number))),
      ),
      '}',
    ),

    // Array literal
    array: $ => seq(
      '(',
      repeat($._literal),
      ')',
    ),

    // ========================================
    // COMMENTS
    // ========================================

    comment: $ => token(prec(-10, seq('#', /.*/))),
  },
});
