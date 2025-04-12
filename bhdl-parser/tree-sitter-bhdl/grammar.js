// Helper function for comma-separated lists
function commaSep(rule) {
  return seq(rule, repeat(seq(',', rule)), optional(','));
}

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

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
    ['call', 'member', 'instantiation'],
    ['member', 'subscript'],
    ['unary'],
    ['multiplicative'],
    ['additive'],
    ['comparative'],
    ['logical_and'],
    ['logical_or'],
    ['range'],
    ['ternary'],
    ['definition'],
  ],

  conflicts: $ => [
    // [$.component_instantiation, $.connection_statement]
  ],

  rules: {
    source_file: $ => repeat($._top_level_item),

    _top_level_item: $ => choice(
      // Definitions
      $.board_definition,
      $.module_definition,
      $.component_definition,
      $.typedef_definition,
      $.property_set_definition,
      $.interface_definition,
      $.net_class_definition,
      $.via_style_definition,

      // Other Blocks/Statements
      $.constraint_block, // Should constraint be top-level?
      $.library_statement,
      $.use_statement,
      $.generate_block, // Should generate be top-level?

      // Test cases / Simple top-level statements
      $._top_level_expression_statement,
      $._top_level_boolean_statement, 
      $._top_level_string_statement,  
      $._top_level_char_statement,    
      $._top_level_enum_statement,    
      $._top_level_physical_statement,
      $._top_level_time_statement,    
      $.comment
    ),

    // Wrapper rules for top-level literals followed by a semicolon
    _top_level_boolean_statement: $ => seq($.boolean_literal, ';'),
    _top_level_string_statement: $ => seq($.string_literal, ';'),
    _top_level_char_statement: $ => seq($.char_literal, ';'),
    _top_level_enum_statement: $ => seq($.enum_literal, ';'),
    _top_level_physical_statement: $ => seq($.physical_literal, ';'),
    _top_level_time_statement: $ => seq($.time_literal, ';'),

    // Added rule for top-level expression statements
    _top_level_expression_statement: $ => seq($._base_expression, ';'),

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

    typedef_definition: $ => prec('definition', seq(
      'typedef',
      field('name', $.identifier),
      optional(seq('extends', field('parent', $.identifier))),
      '{',
      repeat($.property_assignment),
      '}'
    )),

    property_set_definition: $ => prec('definition', seq(
      'property_set',
      field('name', $.identifier),
      '{',
      repeat($.property_assignment),
      '}'
    )),

    property_assignment: $ => seq(
      field('property_name', $.identifier),
      ':',
      field('value', $._expression),
      ';'
    ),

    // Property assignment without trailing semicolon (for use in {} blocks)
    _property_assignment_no_semi: $ => seq(
      field('property_name', $.identifier),
      ':',
      field('value', $._expression)
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
      repeat(choice($.parameter_declaration, $.generate_block)),
      '}'
    ),

    ports_block: $ => seq(
      'ports',
      '{',
      repeat(choice($.pin_port_declaration, $.generate_block)),
      '}'
    ),

    pins_block: $ => seq(
      'pins',
      '{',
      repeat(choice($.pin_port_declaration, $.generate_block)),
      '}'
    ),

    interfaces_block: $ => seq(
      'interfaces',
      '{',
      repeat(choice($.interface_usage_declaration, $.generate_block)),
      '}'
    ),

    components_block: $ => seq(
      'components',
      '{',
      repeat(choice($.component_instantiation, $.generate_block)),
      '}'
    ),

    connections_block: $ => seq(
      'connections',
      '{',
      repeat(choice($.connection_statement, $.generate_block)),
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
      field('body', repeat($._generate_statement)),
      '}'
    ),

    // Define what can go inside a generate block body
    _generate_statement: $ => choice(
        $.local_variable_declaration,
        $.component_instantiation,
        $.connection_statement,
        $.pin_port_declaration
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

    // BHDL Pin/Port Declaration (aligned with Spec v1)
    pin_port_declaration: $ => choice(
      // Case 1: Ground pin (no direction, no subtype)
      seq(
        'pin',
        field('name', seq(field('base_name', $.identifier), optional(field('bus', $.bus_specifier)))),
        ':',
        $.kw_ground,
        repeat($.attribute),
        ';'
      ),
      // Case 2: Signal/Power pin (requires direction)
      seq(
        'pin',
        field('name', seq(field('base_name', $.identifier), optional(field('bus', $.bus_specifier)))),
        ':',
        field('direction', $._pin_direction),
        field('base_type', choice($.kw_signal, $.kw_power)),
        optional(seq('(', field('subtype', $._type_name), ')')),
        repeat($.attribute),
        ';'
      )
    ),

    // Correct pin directions based on spec
    _pin_direction: $ => choice($.kw_in, $.kw_out, $.kw_inout),

    // New rule for pin/port subscript using expression
    pin_port_subscript: $ => seq(
        '[',
        field('index', $._expression),
        ']'
    ),

    // === Component Instantiation Parameter Rules ===
    component_parameter_list_curly: $ => seq(
      '{',
      optional(commaSep($.component_parameter_assignment)),
      optional(','),
      '}'
    ),

    component_parameter_assignment: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $._expression)
    ),

    // component_instantiation definition - REQUIRE curly braces
    component_instantiation: $ => seq(
      field('type', $.identifier),
      field('name', seq(field('base_name', $.identifier), optional(field('bus', $.bus_specifier)))), // Inlined name
      field('parameters', $.component_parameter_list_curly), // Parameters field is now MANDATORY
      ';'
    ),

    // Connection statement rule
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

    // RE-ADD interface_parameter_list definition
    interface_parameter_list: $ => seq(
      '(',
      optional(seq(
        $.interface_parameter_assignment,
        repeat(seq(',', $.interface_parameter_assignment))
      )),
      optional(','),
      ')'
    ),

    // RE-ADD interface_parameter_assignment definition
    interface_parameter_assignment: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $._expression)
    ),

    // ... (bus_specifier, type_specification, keywords, expressions, literals, identifiers, comments) ...
    bus_specifier: $ => seq(
      '[',
      field('high', $._expression),
      optional(seq(':', field('low', $._expression))),
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
    _base_expression: $ => choice(
      // Literals & Base Cases (excluding physical, time, string, boolean, null, enum, char)
      $.identifier,
      $.integer_literal,
      $.float_literal,
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

    // Original _expression rule (includes ALL expression types)
    _expression: $ => choice(
      // Literals & Base Cases
      $.physical_literal,
      $.time_literal,
      $.string_literal,
      $.boolean_literal,
      $.null_literal,
      $.enum_literal,
      $.char_literal,
      $.identifier,
      $.integer_literal,
      $.float_literal,
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
      field('function', $._connection_endpoint),
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
    string_literal: $ => /\"([^\"\\\\]|\\\\.)*\"/,
    integer_literal: $ => token(/-?\d([\d_]*\d)?/),
    float_literal: $ => token(/-?(\d([\d_]*\d)?\.\d*|\.\d([\d_]*\d)?)([eE][-+]?\d([\d_]*\d)?)?/),
    null_literal: $ => 'null',

    // Re-add char_literal definition
    char_literal: $ => /'[^']'/,

    enum_literal: $ => seq(
      $.identifier,
      '::',
      $.identifier
    ),

    // Redefine physical_literal as a single token
    physical_literal: $ => token(/-?\d([\d_]*\d)?(\.\d([\d_]*\d)?)?([eE][-+]?\d([\d_]*\d)?)?[TGMKkµmunpf]?(Vdc|Vac|Vrms|Vpp|V|A|Ohm|F|H|W|Hz|degC|pct|S|dB|bit|Bd|cd|lm|lx|mm|µm|mil|in)/),

    // Redefine time_literal as a single token
    time_literal: $ => token(/-?\d([\d_]*\d)?(\.\d([\d_]*\d)?)?([eE][-+]?\d([\d_]*\d)?)?[mµunpfa]?s/),

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

    // Add library_statement definition here
    library_statement: $ => seq(
      'library',
      field('name', $.identifier),
      ';'
    ),

    // Add use_statement definition here
    use_statement: $ => seq(
      'use',
      field('library_name', $.identifier),
      '.',
      choice(
        field('item_name', $.identifier),
        '*'
      ),
      ';'
    ),

    // Added definition for instance_subscript_expr
    instance_subscript_expr: $ =>
      seq($.identifier, '[', $._expression, ']'),

    instance_parameter_assignment: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $._expression)
    ),

    instance_parameter_list: $ => choice(
      seq('{', commaSep($.instance_parameter_assignment), '}'),
      seq('(', commaSep($.instance_parameter_assignment), ')')
    ),

    component_parameter_list_curly: $ => seq(
      '{',
      optional(commaSep($.component_parameter_assignment)),
      optional(','),
      '}'
    ),

    component_parameter_assignment: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $._expression)
    ),

    // Define attribute rule
    attribute: $ => choice(
      seq(field('key', $.identifier), '=', field('value', $._expression)),
      field('flag', $.identifier) // For standalone attributes like VCC
    ),
  },
});