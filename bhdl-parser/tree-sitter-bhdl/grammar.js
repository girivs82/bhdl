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
    ['call', 'member'],
    ['unary'],
    ['multiplicative'],
    ['additive'],
    ['comparative'],
    ['logical_and'],
    ['logical_or'],
    ['range'],
    ['ternary'],
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

    // === Generate Block ===
    generate_block: $ => seq(
      'generate',
      'for',
      field('variable', $.identifier),
      'in',
      field('range', $._generate_range),
      '{',
      field('body', repeat(choice(
        $.local_variable_declaration,
        $.pin_port_declaration,
        $.component_instantiation,
        $.connection_statement,
      ))),
      '}'
    ),

    // Add missing _generate_range rules
    _generate_range: $ => choice(
      $.range_to,
      $.range_upto,
      $.identifier
    ),

    range_to: $ => seq(
      field('start', $._expression),
      'to',
      field('end', $._expression)
    ),

    range_upto: $ => seq(
      field('start', $._expression),
      'upto',
      field('end', $._expression)
    ),

    // === Declarations / Instantiations / Statements ===
    parameter_declaration: $ => choice(
      seq( // With type: name : type [ = value ];
        field('name', $.identifier),
        ':',
        field('param_type', $._type_name),
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

    interface_parameter_declaration: $ => choice(
      seq( // With type: name : type [ = value ]
        field('name', $.identifier),
        ':',
        field('param_type', $._type_name),
        optional(seq('=', field('default_value', $._expression)))
      ),
      seq( // Without type: name = value
        field('name', $.identifier),
        '=',
        field('default_value', $._expression)
      )
    ),

    // Define what constitutes a type name (just identifier for now)
    _type_name: $ => $.identifier,

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
      // Literals & Base Cases
      $.numeric_literal,
      $.string_literal,
      $.boolean_literal,
      $.identifier,
      $.integer_literal,
      // Operators / Constructs
      $.parenthesized_expression,
      $.unary_expression,
      $.binary_expression,
      $.range_expression,
      $.array_literal,
      $.function_call_expression,
      $.member_access,
      $.subscript_access,
      $.ternary_expression
    ),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    // Add binary expressions with precedence
    binary_expression: $ => choice(
      prec.left('additive', seq($._expression, '+', $._expression)),
      prec.left('additive', seq($._expression, '-', $._expression)),
      prec.left('multiplicative', seq($._expression, '*', $._expression)),
      prec.left('multiplicative', seq($._expression, '/', $._expression)),
      prec.left('comparative', seq($._expression, '>', $._expression)),
      prec.left('comparative', seq($._expression, '>=', $._expression)),
      prec.left('comparative', seq($._expression, '<', $._expression)),
      prec.left('comparative', seq($._expression, '<=', $._expression)),
      prec.left('comparative', seq($._expression, '==', $._expression)),
      prec.left('comparative', seq($._expression, '!=', $._expression)),
      prec.left('logical_and', seq($._expression, '&&', $._expression)),
      prec.left('logical_or', seq($._expression, '||', $._expression))
    ),

    // Add unary expression with precedence
    unary_expression: $ => prec.right('unary', seq(
      choice('-', '!'),
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

    // Add function call expression
    function_call_expression: $ => prec('call', seq(
      field('function', $._connection_endpoint), // Function can be identifier or member access
      field('arguments', $.argument_list)
    )),

    argument_list: $ => seq(
      '(',
      optional(seq(
        $._expression,
        repeat(seq(',', $._expression))
      )),
      optional(','), // Allow trailing comma
      ')'
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

    // Add local variable declaration rule
    local_variable_declaration: $ => seq(
      'local',
      field('name', $.identifier),
      '=',
      field('value', $._expression),
      ';'
    ),

    // Add ternary conditional expression
    ternary_expression: $ => prec.right('ternary', seq(
      field('condition', $._expression),
      '?',
      field('consequence', $._expression),
      ':',
      field('alternative', $._expression)
    )),
  },
});