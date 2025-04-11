module.exports = grammar({
  name: 'bhdl',

  extras: $ => [
    /\s+/,       // Whitespace
    $.comment
  ],

  // Remove external scanner config
  // externals: $ => [...],

  word: $ => $.identifier,

  // Define precedence levels for expressions
  precedences: $ => [
    ['member'], // Add back member/subscript access
    ['unary'],
    ['multiplicative'],
    ['additive'],
    ['range'],
    // Add other levels later if needed (e.g., comparison, logical)
  ],

  rules: {
    source_file: $ => repeat(choice(
      $.import_statement,
      $.board_definition,
      $.module_definition,
      $.component_definition,
      $.typedef_definition,
      $.property_set_definition,
      $.interface_definition,
      $.net_class_definition,
      $.via_style_definition,
      // Allow others for testing
      $.boolean_literal,
      $.string_literal,
      $.numeric_literal,
      $.integer_literal,
      $.identifier,
      $.comment,
      $.constraint_block
    )),

    // === Structural Elements ===
    import_statement: $ => seq(
      'import',
      field('path', $.import_path),
      optional(field('items', choice($.import_list, '*'))),
      ';'
    ),
    import_path: $ => seq($.identifier, repeat(seq('.', $.identifier))),
    import_list: $ => seq(
      '{',
      optional(seq($.identifier, repeat(seq(',', $.identifier)))),
      '}'
    ),

    board_definition: $ => seq(
      'board',
      field('name', $.identifier),
      '{',
      repeat(choice(
        $.parameters_block,
        $.ports_block,
        $.components_block,
        $.connections_block,
        $.constraint_block,
        $.layer_stackup_block,
        $.default_design_rules_block
      )),
      '}'
    ),

    module_definition: $ => seq(
      'module',
      field('name', $.identifier),
      '{',
      repeat(choice(
         $.parameters_block,
         $.ports_block,
         $.components_block,
         $.connections_block
      )),
      '}'
    ),

    component_definition: $ => seq(
      'component',
      field('name', $.identifier),
      '{',
       repeat(choice(
         $.parameters_block,
         $.pins_block,
         $.interfaces_block
       )),
      '}'
    ),

    typedef_definition: $ => seq(
      'typedef',
      field('name', $.identifier),
      ':',
      field('type', $.type_specification_signal_power),
      ';'
    ),

    property_set_definition: $ => seq(
      'property_set',
      field('name', $.identifier),
      '{',
      repeat($.property_assignment),
      '}'
    ),

    interface_definition: $ => seq(
      'interface',
      field('name', $.identifier),
      optional(field('declaration_parameters', $.interface_declaration_parameter_list)),
      '{',
      repeat(choice(
        $.parameters_block,
        $.pins_block
      )),
      '}'
    ),

    interface_declaration_parameter_list: $ => seq(
        '(',
        optional(seq(
          $.interface_parameter_declaration,
          repeat(seq(',', $.interface_parameter_declaration))
        )),
        optional(','),
        ')'
    ),

    net_class_definition: $ => seq(
      'net_class',
      field('name', $.identifier),
      '{',
      repeat($.property_assignment),
      '}'
    ),

    via_style_definition: $ => seq(
      'via_style',
      field('name', $.identifier),
      '{',
      repeat($.property_assignment),
      '}'
    ),

    // === Blocks ===
    parameters_block: $ => seq(
      'parameters',
      '{',
      repeat($.parameter_declaration),
      '}'
    ),

    ports_block: $ => seq(
      'ports',
      '{',
      repeat($.pin_port_declaration),
      '}'
    ),

    pins_block: $ => seq(
      'pins',
      '{',
      repeat($.pin_port_declaration),
      '}'
    ),

    interfaces_block: $ => seq(
      'interfaces',
      '{',
      repeat($.interface_usage_declaration),
      '}'
    ),

    components_block: $ => seq(
      'components',
      '{',
      repeat($.component_instantiation),
      '}'
    ),

    connections_block: $ => seq(
      'connections',
      '{',
      repeat($.connection_statement),
      '}'
    ),

    constraint_block: $ => seq(
      'constrain',
      '(',
      field('target', $._connection_endpoint),
      ')',
      '{',
      repeat($.property_assignment),
      '}'
    ),

    layer_stackup_block: $ => seq(
      'layer_stackup',
      '{',
      repeat($.layer_definition),
      '}'
    ),

    layer_definition: $ => seq(
      'layer',
      field('name', $.identifier),
      ':',
      '{',
      repeat($.property_assignment),
      '}',
      ';'
    ),

    default_design_rules_block: $ => seq(
      'default_design_rules',
      '{',
      repeat($.property_assignment),
      '}'
    ),

    // === Declarations / Instantiations / Statements ===
    parameter_declaration: $ => choice(
      seq( // With type: name : type [ = value ];
        field('name', $.identifier),
        ':',
        field('param_type', $.identifier),
        optional(seq('=', field('default_value', $._expression))),
        ';'
      ),
      seq( // Without type: name = value ;
        field('name', $.identifier),
        '=',
        field('default_value', $._expression),
        ';'
      )
    ),

    // For use in interface definition parameter lists (no semicolon)
    interface_parameter_declaration: $ => choice(
      seq( // With type: name : type [ = value ]
        field('name', $.identifier),
        ':',
        field('param_type', $.identifier),
        optional(seq('=', field('default_value', choice($.integer_literal, $.numeric_literal, $.string_literal, $.boolean_literal, $.identifier)))),
        // No semicolon
      ),
      seq( // Without type: name = value
        field('name', $.identifier),
        '=',
        field('default_value', choice($.integer_literal, $.numeric_literal, $.string_literal, $.boolean_literal, $.identifier)),
        // No semicolon
      )
    ),

    // Use explicit keyword rules
    pin_port_declaration: $ => seq(
      field('name', $.identifier),
      optional($.bus_specifier),
      ':',
      choice(
        seq(field('direction', $._direction_keyword), field('type', $.type_specification_signal_power)),
        seq(field('type', $.type_specification_ground))
      ),
      ';'
    ),

    component_instantiation: $ => seq(
      field('type', $.identifier),
      field('name', $.identifier),
      '{',
      repeat($.property_assignment),
      '}',
      optional(';')
    ),

    property_assignment: $ => seq(
      field('property_name', $.identifier),
      ':',
      field('value', $._expression),
      ';'
    ),

    // Placeholder connection statement
    connection_statement: $ => seq(
      field('source', $._connection_endpoint),
      field('operator', choice('->', '<=>')),
      field('target', $._connection_endpoint),
      ';'
    ),

    // Restore standard endpoint definition
    _connection_endpoint: $ => choice(
      $.identifier,
      $.member_access,
      $.subscript_access
    ),

    // Restore member and subscript access rules
    member_access: $ => prec.left('member', seq(
      field('object', $._connection_endpoint),
      '.',
      field('property', $.identifier)
    )),

    subscript_access: $ => prec.left('member', seq(
      field('object', $._connection_endpoint),
      field('index', $.bus_specifier)
    )),

    interface_usage_declaration: $ => seq(
      field('name', $.identifier),
      ':',
      $.kw_interface,
      field('type', $.identifier),
      optional(field('parameters', $.interface_parameter_list)),
      ';'
    ),

    interface_parameter_list: $ => seq(
      '(',
      // Allow comma-separated list of assignments
      optional(seq(
        $.interface_parameter_assignment,
        repeat(seq(',', $.interface_parameter_assignment))
      )),
      // Allow optional trailing comma
      optional(','),
      ')'
    ),

    interface_parameter_assignment: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $._expression) // Reuse expression rule
    ),

    // ... (bus_specifier, type_specification, keywords, expressions, literals, identifiers, comments) ...
    bus_specifier: $ => seq(
      '[',
      field('high', $.integer_literal),
      optional(seq(':', field('low', $.integer_literal))),
      ']'
    ),

    type_specification_signal_power: $ => seq(
      field('base', $._base_type_signal_power_keyword),
      optional(seq('(', field('subtype', $.identifier), ')'))
    ),
    type_specification_ground: $ => seq(
      field('base', $._base_type_ground_keyword),
      optional(seq('(', field('subtype', $.identifier), ')'))
    ),

    // === Keywords (defined as rules) ===
    _direction_keyword: $ => choice($.kw_in, $.kw_out, $.kw_inout),
    kw_in: $ => 'in',
    kw_out: $ => 'out',
    kw_inout: $ => 'inout',
    kw_ground: $ => 'ground',
    kw_interface: $ => 'interface',

    _base_type_signal_power_keyword: $ => choice($.kw_signal, $.kw_power),
    _base_type_ground_keyword: $ => $.kw_ground,
    kw_signal: $ => 'signal',
    kw_power: $ => 'power',

    // === Expressions ===
    _expression: $ => choice(
      $.numeric_literal,
      $.string_literal,
      $.boolean_literal,
      $.identifier,
      $.integer_literal,
      $.parenthesized_expression,
      $.unary_expression,
      $.binary_expression,
      $.range_expression,
      $.array_literal
    ),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    // Add binary expressions with precedence
    binary_expression: $ => choice(
      prec.left('additive', seq($._expression, '+', $._expression)),
      prec.left('additive', seq($._expression, '-', $._expression)),
      prec.left('multiplicative', seq($._expression, '*', $._expression)),
      prec.left('multiplicative', seq($._expression, '/', $._expression))
    ),

    // Add unary expression with precedence
    unary_expression: $ => prec.right('unary', seq(
      '-', // Only minus for now
      $._expression
    )),

    // Add range expression with precedence
    range_expression: $ => prec.left('range', seq(
      $._expression, 'to', $._expression
    )),

    array_literal: $ => seq(
      '[',
      optional(seq(
        $._expression,
        repeat(seq(',', $._expression))
      )),
      optional(','), // Allow trailing comma
      ']'
    ),

    // === Literals ===
    boolean_literal: $ => choice('true', 'false'),
    string_literal: $ => /"[^"]*"/,
    integer_literal: $ => token(/\d+/),
    numeric_literal: $ => {
      const number = /\d+(\.\d*)?|\.\d+/;
      const prefixes = /[TGMkmunpf]/;
      const units = /Vdc|Vac|Vrms|Vpp|V|A|Ohm|F|H|W|Hz|s|degC|pct|S|dB|bit|Bd|cd|lm|lx|mm|um|mil|in/;
      return token(seq(
        number,
        optional(seq(optional(prefixes), units))
      ));
    },

    // === Identifiers ===
    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    // === Comments ===
    comment: $ => token(choice(
      seq('//', /.*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')
    )),
  },
});