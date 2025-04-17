// Helper function for comma-separated lists
function commaSep(rule) {
  return seq(rule, repeat(seq(',', rule)), optional(','));
}

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

// Helper function for keywords using alias
function kw(keyword) {
  return alias(keyword, keyword);
}

module.exports = grammar({
  name: 'bhdl',

  extras: $ => [
    /\s+/,       // Whitespace
    $.comment
  ],

  word: $ => $.identifier,

  // Define precedence levels for expressions
  precedences: $ => [
    ['call', 'member', 'instantiation'],
    ['definition'],
    ['member', 'subscript'],
    ['unary'],
    ['multiplicative'],
    ['additive'],
    ['range_expr'],
    ['comparative'],
    ['logical_and'],
    ['logical_or'],
    ['ternary'],
  ],

  conflicts: $ => [
    // Explicitly resolve conflicts between keywords and identifier
    [$.kw_board, $.identifier],
    [$.kw_end, $.identifier],
    [$.kw_module, $.identifier],
    [$.kw_component, $.identifier],
    [$.kw_property_set, $.identifier],
    [$.kw_typedef, $.identifier],
    [$.kw_interface, $.identifier],
    [$.kw_net_class, $.identifier],
    [$.kw_via_style, $.identifier],
    [$.kw_library, $.identifier],
    [$.kw_use, $.identifier],
    [$.kw_generate, $.identifier],
    [$.kw_constraint, $.identifier],
    [$.kw_parameters, $.identifier],
    [$.kw_ports, $.identifier],
    [$.kw_components, $.identifier],
    [$.kw_connections, $.identifier],
    [$.kw_layer_stackup, $.identifier],
    [$.kw_default_design_rules, $.identifier],
    [$.kw_pins, $.identifier],
    [$.kw_interfaces, $.identifier],
    [$.kw_for, $.identifier],
    [$.kw_all, $.identifier],
    [$.kw_in, $.identifier],
    [$.kw_out, $.identifier],
    [$.kw_inout, $.identifier],
    [$.kw_signal, $.identifier],
    [$.kw_power, $.identifier],
    [$.kw_ground, $.identifier], // Added ground
    [$.kw_pin, $.identifier],
    [$.kw_loop, $.identifier], // Added loop
    [$.kw_time, $.identifier],
    [$.kw_boolean, $.identifier],
    [$.kw_string, $.identifier],
    [$.kw_char, $.identifier],
    [$.kw_physical, $.identifier],

    // Resolve potential conflict between range_expression and other binary ops
    [$.range_expression, $.binary_expression],
    // [_generate_range_expression, $._expression], // Removed
    [$.scoped_type_name, $._expression], // Resolve ambiguity in generate blocks
    [$._expression, $.physical_literal], // Resolve ambiguity in constraint blocks etc.
    [$._expression, $.pin_port_declaration], // Resolve generate block pin decl vs expression
    
    // Resolve conflict between parameter declaration keyword and identifier
    [$.kw_param, $.identifier],

    // Resolve ambiguity between expression list in parens and generic expression
    [$.component_parameter_list_paren, $._expression]
  ],

  rules: {
    source_file: $ => repeat($._top_level_item),

    _top_level_item: $ => choice(
      $.import_statement,
      $.board_definition,
      $.module_definition,
      $.component_definition,
      $.typedef_definition,
      $.property_set_definition,
      $.interface_definition,
      $.net_class_definition,
      $.via_style_definition,
      $._top_level_expression_statement, // Restore this rule
      $.assignment_statement, // Allow top-level assignment
      $.comment
    ),

    // === Assignment Statement ===
    assignment_statement: $ => seq(
      field('left', $.identifier), // Assuming only assign to simple identifier for now
      '=',
      field('right', $._expression),
      ';'
    ),

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
      optional(commaSep1($.identifier)), // Use commaSep1
      '}'
    ),

    board_definition: $ => prec('definition', seq(
      $.kw_board,
      field('name', $.identifier),
      optional(field('parameters', $.declaration_parameter_list)), // Added board parameters ()
      '{',
      repeat($._board_item),
      '}'
    )),

    _board_item: $ => choice(
        $.parameters_block,
        $.ports_block,
        $.components_block,
        $.connections_block,
        $.layer_stackup_block,
        $.default_design_rules_block,
        $.constraint_statement, // Allow constraints directly in board
        $.component_definition, // Allow nested definitions? (Check spec)
        $.module_definition,
        $.typedef_definition,
        $.interface_definition,
        $.net_class_definition,
        $.via_style_definition,
        $.property_set_definition,
        $.comment
    ),

    module_definition: $ => prec('definition', seq(
      $.kw_module,
      field('name', $.identifier),
      optional(field('parameters', $.declaration_parameter_list)), // Added module parameters ()
      '{',
      repeat($._module_item),
      '}',
      ';'
    )),

     _module_item: $ => choice(
         $.parameters_block,
         $.ports_block,
         $.components_block,
         $.connections_block,
         $.component_definition, // Allow nested definitions? (Check spec)
         $.module_definition,
         $.typedef_definition,
         $.interface_definition,
         $.property_set_definition,
         $.comment
     ),

    component_definition: $ => prec('definition', seq(
      $.kw_component,
      field('name', $.identifier),
      optional(field('parameters', $.declaration_parameter_list)), // Added component parameters ()
      optional(seq(
         '{',
         repeat($._component_item),
         '}'
      )),
      ';'
    )),

    _component_item: $ => choice(
       $.parameters_block,
       $.pins_block,
       $.interfaces_block,
       $.comment
    ),

    typedef_definition: $ => prec('definition', seq(
      $.kw_typedef,
      field('name', $.identifier),
      optional(seq('extends', field('parent', $.identifier))),
      '{',
      repeat($.property_assignment), // Uses :
      '}',
      ';'
    )),

    property_set_definition: $ => prec('definition', seq(
      $.kw_property_set,
      field('name', $.identifier),
      '{',
      repeat($.property_assignment), // Uses :
      '}',
      ';'
    )),

    interface_definition: $ => prec('definition', seq(
      $.kw_interface,
      field('name', $.identifier),
      optional(field('parameters', $.declaration_parameter_list)), // Use standard declaration parameters
      '{',
      repeat($._interface_item),
      '}',
      ';'
    )),

    _interface_item: $ => choice(
      $.parameters_block,
      $.pins_block, // Note: Spec uses 'pins' inside interface
      $.comment
    ),

    net_class_definition: $ => prec('definition', seq(
      $.kw_net_class,
      field('name', $.identifier),
      '{',
      repeat($.property_assignment), // Uses :
      '}',
      ';'
    )),

    via_style_definition: $ => prec('definition', seq(
      $.kw_via_style,
      field('name', $.identifier),
      '{',
      repeat($.property_assignment), // Uses :
      '}',
      ';'
    )),

    // === Blocks within structures ===
    parameters_block: $ => seq(
      $.kw_parameters,
      '{',
      repeat($.parameter_declaration),
      '}'
    ),

    ports_block: $ => seq(
      $.kw_ports,
      '{',
      repeat($.pin_port_declaration),
      '}'
    ),

    pins_block: $ => seq(
      $.kw_pins,
      '{',
      repeat($.pin_port_declaration),
      '}'
    ),

    interfaces_block: $ => seq( // Added based on spec
      $.kw_interfaces,
      '{',
      repeat($.interface_usage_declaration),
      '}'
    ),

    components_block: $ => seq(
      $.kw_components,
      '{',
      repeat($.component_instantiation),
      '}'
    ),

    connections_block: $ => seq(
      $.kw_connections,
      '{',
      repeat($.connection_statement),
      '}'
    ),

    layer_stackup_block: $ => seq(
      $.kw_layer_stackup,
      '{',
      repeat($.layer_definition),
      '}'
    ),

    layer_definition: $ => seq(
      'layer', // Using string literal as 'layer' is not a dedicated keyword
      field('name', $.identifier),
      ':',
      '{',
      repeat($.property_assignment), // Uses :
      '}',
      ';'
    ),

    default_design_rules_block: $ => seq(
      $.kw_default_design_rules,
      '{',
      repeat($.property_assignment), // Uses :
      '}'
    ),

    // === Generate Constructs ===
    generate_block: $ => seq(
      $.kw_generate,
      $.generate_for_statement // Currently only 'for' is supported
    ),

    generate_for_statement: $ => seq(
      'for',
      field('variable', $.identifier),
      $.kw_in,
      field('range', $._generate_range_expression),
      // Only allow {} block for generate for body
      seq(
        '{',
        field('body', repeat($._generate_body_item)),
        '}'
      )
    ),

    _generate_body_item: $ => choice(
      // $.local_variable_declaration, // Add if needed
      $.component_instantiation,
      $.connection_statement,
      $.pin_port_declaration,
      $.parameter_declaration,
      $.constraint_statement,
      $.generate_block // Nested generate
    ),

    // === Declarations / Instantiations / Statements ===

    // Parameter list for declarations ( board(), module(), component(), interface() )
    declaration_parameter_list: $ => seq(
        '(',
        optional(commaSep($.parameter_declaration_item)),
        ')'
    ),
    // Item within the above list
    parameter_declaration_item: $ => seq(
        field('name', $.identifier),
        ':',
        field('param_type', $._type_name),
        optional(seq('=', field('default_value', $._expression)))
    ),

    // Parameter declaration inside parameters { } block
    parameter_declaration: $ => seq(
      $.kw_param, // Add keyword here
      field('name', $.identifier),
      optional(seq(':', field('param_type', $._type_name))), // Optional type
      '=', // Use equals for assignment
      field('value', $._expression),
      ';'
    ),

    property_assignment: $ => seq(
      field('property_name', $.identifier),
      ':', // Use colon based on spec
      field('value', $._expression),
      ';'
    ),

    // For properties inside {} blocks like layer defs, constraints
    _property_assignment_no_semi: $ => seq(
      field('property_name', $.identifier),
      ':',
      field('value', $._expression)
      // No semicolon
    ),

    _type_name: $ => choice(
      $.identifier, // Simple type name
      $.scoped_type_name // e.g. Library.Type
    ),
    scoped_type_name: $ => prec.left(seq(
        field('scope', $.identifier),
        '.',
        field('name', $._type_name) // Allow nested scopes
    )),


    pin_port_declaration: $ => prec('definition', seq(
        optional(field('pin_port_kw', choice('pin', 'port'))), // Optional keyword
        field('name', $.identifier),                         // Name identifier
        ':',                                                 // Colon
        field('kind', choice(                                // Kind specifier
          // Case 1: Ground
          $.kw_ground, // Use keyword alias
          // Case 2: Signal/Power -> Give this seq a name
          $.pin_port_type_spec 
        )),
        optional(field('bus', $.bus_specifier)),             // Optional bus specifier
        ';'                                                  // Semicolon
    )),

    // Define the named rule for the signal/power sequence
    pin_port_type_spec: $ => seq(
        field('direction', $._pin_direction),           // Direction (in/out/inout)
        optional(field('base_type', choice($.kw_signal, $.kw_power))), // Optional signal/power
        optional(seq('(', field('subtype', $._type_name), ')')),       // Optional subtype
    ),

    _pin_direction: $ => choice($.kw_in, $.kw_out, $.kw_inout),

    bus_specifier: $ => seq( // For things like DATA[7:0]
        '[',
        field('high', $._expression),
        optional(seq(':', field('low', $._expression))), // Allow single index or range
        ']'
    ),

    // Component Instantiation (Restored Optional Parts)
     component_instantiation: $ => prec('instantiation', seq(
      field('name', $.identifier), // Instance name first
      ':', 
      field('type', $._type_name),  // Type name second
      optional(field('bus', $.bus_specifier)), // Optional bus specifier
      optional( // Optional parameters
        choice(
           field('parameters_paren', $.component_parameter_list_paren),
           field('parameters_curly', $.component_parameter_list_curly)
        )
      ),
      ';'
    )),

    // Keep parameter list rules defined, just not used by simplified instantiation
    component_parameter_list_paren: $ => seq(
      '(',
      // Explicitly allow physical literals in addition to general expressions
      optional(commaSep(choice($._expression, $.physical_literal))), 
      ')'
    ),

    component_parameter_list_curly: $ => seq(
      '{',
      // Restore optional commaSep to allow empty {}
      optional(commaSep($.component_parameter_assignment)),
      '}'
    ),

    component_parameter_assignment: $ => seq(
      field('name', $.identifier),
      '=', // Parameter assignments use '='
      field('value', $._expression) // Revert back to _expression
    ),

    // Interface Usage Declaration (inside interfaces {} block)
    interface_usage_declaration: $ => seq(
      field('name', $.identifier),
      ':',
      $.kw_interface,
      field('type', $._type_name),
      optional(field('arguments', $.interface_argument_list)), // Arguments passed to interface type
      // Optional pin mapping for components (Spec 3.4)
      optional(seq(
         $.kw_pins, ':', '{', // Example: pins: { MOSI: P1_0; ... }
         repeat($.interface_pin_mapping),
         '}'
      )),
      ';'
    ),

    interface_argument_list: $ => seq( // e.g. AxiBus(addr_width=32, data_width=64)
       '(',
       optional(commaSep($.interface_argument_assignment)),
       ')'
    ),

    interface_argument_assignment: $ => seq( // e.g. addr_width=32
      field('name', $.identifier),
      '=',
      field('value', $._expression)
    ),

    interface_pin_mapping: $ => seq( // e.g. MOSI: P1_0;
      field('interface_pin', $.identifier),
      ':',
      field('component_pin', $.identifier),
      ';'
    ),


    // Connection statement rule
    connection_statement: $ => seq(
      field('source', $._connection_endpoint),
      field('operator', choice('->', '<-', '<=>')), // Allow <- and <=>
      field('target', $._connection_endpoint),
      optional(seq('{', repeat($._property_assignment_no_semi), '}')), // Optional inline constraints
      ';'
    ),

    _connection_endpoint: $ => choice(
      $.identifier,       // Net name or GND/VCC implicit nets
      $.member_access,    // Component.Pin, Module.Port
      $.subscript_access  // Bus[index], Component[index].Pin
    ),

    // A constraint *statement* (assuming it applies to generated items or top level)
    constraint_statement: $ => seq(
      $.kw_constraint,
      // Target can be more complex: net, pin, component, group etc.
      // Using _expression for now, needs refinement based on spec examples (Sec 5)
      field('target', $._expression),
      '{',
      repeat($.property_assignment), // Uses property_assignment with : and ;
      '}'
      // No semicolon after constraint block in examples
    ),

    // === Expressions ===
    _expression: $ => choice(
      $.binary_expression,
      $.unary_expression,
      $.ternary_expression,
      $.range_expression,
      $.member_access, // Re-add here for general expressions
      $.subscript_access, // Re-add here for general expressions
      $.function_call_expression,
      $.parenthesized_expression,
      $.identifier,
      $.physical_literal,
      $.integer_literal,
      $.float_literal,
      $.boolean_literal,
      $.string_literal,
      $.char_literal,
      $.enum_value_literal
      // Add other expression forms as needed
    ),

    parenthesized_expression: $ => seq(
      '(',
      $._expression,
      ')'
    ),

    binary_expression: $ => choice(
      prec.left('logical_or', seq($._expression, '||', $._expression)),
      prec.left('logical_and', seq($._expression, '&&', $._expression)),
      prec.left('comparative', seq($._expression, choice('==', '!=', '<', '<=', '>', '>='), $._expression)),
      prec.left('additive', seq($._expression, choice('+', '-'), $._expression)),
      prec.left('multiplicative', seq($._expression, choice('*', '/'), $._expression))
      // Add bitwise operators etc. if needed
    ),

    unary_expression: $ => prec.left('unary', seq(
      choice('!', '-'), // Add other unary ops like +, ~ if needed
      $._expression
    )),

    ternary_expression: $ => prec.right('ternary', seq(
        $._expression, '?', $._expression, ':', $._expression
    )),

    range_expression: $ => prec.left('range_expr', seq(
      field('lower', $._expression),
      field('operator', token(choice('..', 'to', 'upto'))),
      field('upper', $._expression)
    )),

    member_access: $ => prec.left('member', seq(
      field('object', $._expression), // Allow expressions like (complex_obj).member
      '.',
      field('property', choice($.identifier, $.integer_literal)) // Allow identifier OR integer literal
    )),

    subscript_access: $ => prec.left('subscript', seq(
      field('object', $._expression), // Allow (complex_obj)[idx]
      field('index', $.bus_specifier) // Reuse bus_specifier for index access
    )),

    function_call_expression: $ => prec('call', seq(
        field('function', $._expression), // Allows obj.method()
        field('arguments', $.argument_list)
    )),

    argument_list: $ => seq(
        '(',
        optional(commaSep($.argument_assignment)),
        ')'
    ),
    argument_assignment: $ => choice( // Allow positional or named args
        $._expression, // Positional
        seq(field('name', $.identifier), '=', field('value', $._expression)) // Named
    ),

    // Wrapper for top-level expression statements ending in semicolon
    _top_level_expression_statement: $ => seq($._expression, ';'),


    // === Literals ===
    physical_literal: $ => seq(
        choice($.integer_literal, $.float_literal),
        $.identifier // Unit (e.g., V, kOhm, uF, MHz) - Relaxed for now
        // TODO: Define specific unit patterns if needed: /(V|A|Ohm|F|H|W|Hz|s|degC|pct|S|dB|bit|Bd|cd|lm|lx)/
    ),

    integer_literal: $ => /\d+/,
    // Require digit after decimal point to avoid conflict with '..' range op
    float_literal: $ => /\d+\.\d+(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?|\d+[eE][+-]?\d+/,
    boolean_literal: $ => choice('true', 'false'),
    string_literal: $ => /\"([^\\\"]|\\.)*\"/,
    char_literal: $ => /\'([^\\\']|\\.)*\'/,
    // Enum value literal like Type'Value
    enum_value_literal: $ => seq($.identifier, '\'', $.identifier),

    // === Keywords ===
    // Use alias helper `kw` for all keywords to avoid conflicts with identifier rule
    kw_board: $ => kw('board'),
    kw_end: $ => kw('end'),
    kw_module: $ => kw('module'),
    kw_component: $ => kw('component'),
    kw_property_set: $ => kw('property_set'),
    kw_typedef: $ => kw('typedef'),
    kw_interface: $ => kw('interface'),
    kw_net_class: $ => kw('net_class'),
    kw_via_style: $ => kw('via_style'),
    kw_library: $ => kw('library'),
    kw_use: $ => kw('use'),
    kw_generate: $ => kw('generate'),
    kw_constraint: $ => kw('constraint'),
    kw_parameters: $ => kw('parameters'),
    kw_ports: $ => kw('ports'),
    kw_components: $ => kw('components'),
    kw_connections: $ => kw('connections'),
    kw_layer_stackup: $ => kw('layer_stackup'),
    kw_default_design_rules: $ => kw('default_design_rules'),
    kw_pins: $ => kw('pins'),
    kw_interfaces: $ => kw('interfaces'),
    kw_for: $ => kw('for'),
    kw_loop: $ => kw('loop'),
    kw_all: $ => kw('all'),
    kw_in: $ => kw('in'),
    kw_out: $ => kw('out'),
    kw_inout: $ => kw('inout'),
    kw_signal: $ => kw('signal'),
    kw_power: $ => kw('power'),
    kw_ground: $ => kw('ground'),
    kw_pin: $ => kw('pin'),
    kw_port: $ => kw('port'),
    kw_time: $ => kw('time'),
    kw_boolean: $ => kw('boolean'),
    kw_string: $ => kw('string'),
    kw_char: $ => kw('char'),
    kw_physical: $ => kw('physical'),
    kw_to: $ => kw('to'),
    kw_upto: $ => kw('upto'),
    kw_not: $ => kw('not'),
    kw_and: $ => kw('and'),
    kw_or: $ => kw('or'),
    kw_xor: $ => kw('xor'),
    kw_group: $ => kw('group'),
    kw_layer: $ => kw('layer'),
    kw_param: $ => kw('param'),

    // === Identifier ===
    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    // === Comment ===
    comment: $ => token(choice(
      seq('//', /(\\(.|\r?\n)|[^\\\n])*/),
      seq(
        '/*',
        /[^*]*\*+([^/*][^*]*\*+)*/,
        '/'
      )
    )),

    _generate_range_expression: $ => choice(
      $.range_expression,
      $.identifier
    ),
  }
});