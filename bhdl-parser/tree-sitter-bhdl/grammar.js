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
    ['range_expr'],
    ['multiplicative'],
    ['additive'],
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
    [$._expression, $.pin_port_declaration] // Resolve generate block pin decl vs expression
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
      $.generate_block, // Allow generate at top level
      $._top_level_expression_statement, // Allow standalone expressions (like literals.txt)
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
      '}',
      optional(seq($.kw_end, $.kw_board, optional(field('end_name', $.identifier)))), // Make end optional
      ';'
    )),

    _board_item: $ => choice(
        $.parameters_block,
        $.ports_block,
        $.components_block,
        $.connections_block,
        $.layer_stackup_block,
        $.default_design_rules_block,
        $.constraint_statement, // Allow constraints directly in board
        $.generate_block,       // Allow generate directly in board
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
      optional(seq($.kw_end, $.kw_module, optional(field('end_name', $.identifier)))), // Make end optional
      ';'
    )),

     _module_item: $ => choice(
         $.parameters_block,
         $.ports_block,
         $.components_block,
         $.connections_block,
         $.generate_block, // Allow generate in module
         $.constraint_statement, // Allow constraints in module
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
      // Make body optional for simple declarations
      optional(seq(
         '{',
         repeat($._component_item),
         '}'
      )),
      optional(seq($.kw_end, $.kw_component, optional(field('end_name', $.identifier)))), // Make end optional
      ';'
    )),

    _component_item: $ => choice(
       $.parameters_block,
       $.pins_block,
       $.interfaces_block,
       $.generate_block, // Allow generate in component
       $.constraint_statement, // Allow constraints in component
       $.comment
    ),

    typedef_definition: $ => prec('definition', seq(
      $.kw_typedef,
      field('name', $.identifier),
      optional(seq('extends', field('parent', $.identifier))),
      '{',
      repeat($.property_assignment), // Uses :
      '}',
      optional(seq($.kw_end, $.kw_typedef)), // Make end optional
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
      optional(seq($.kw_end, $.kw_interface)), // Make end optional
      ';'
    )),

    _interface_item: $ => choice(
      $.parameters_block,
      $.pins_block, // Note: Spec uses 'pins' inside interface
      $.generate_block,
      $.comment
    ),

    net_class_definition: $ => prec('definition', seq(
      $.kw_net_class,
      field('name', $.identifier),
      '{',
      repeat($.property_assignment), // Uses :
      '}',
      optional(seq($.kw_end, $.kw_net_class)), // Make end optional
      ';'
    )),

    via_style_definition: $ => prec('definition', seq(
      $.kw_via_style,
      field('name', $.identifier),
      '{',
      repeat($.property_assignment), // Uses :
      '}',
      optional(seq($.kw_end, $.kw_via_style)), // Make end optional
      ';'
    )),

    // === Blocks within structures ===
    parameters_block: $ => seq(
      $.kw_parameters,
      '{',
      repeat(choice($.parameter_declaration, $.generate_block)), // Allow generate here
      '}'
    ),

    ports_block: $ => seq(
      $.kw_ports,
      '{',
      repeat(choice($.pin_port_declaration, $.generate_block)), // Allow generate here
      '}'
    ),

    pins_block: $ => seq(
      $.kw_pins,
      '{',
      repeat(choice($.pin_port_declaration, $.generate_block)), // Allow generate here
      '}'
    ),

    interfaces_block: $ => seq( // Added based on spec
      $.kw_interfaces,
      '{',
      repeat(choice($.interface_usage_declaration, $.generate_block)), // Allow generate here
      '}'
    ),

    components_block: $ => seq(
      $.kw_components,
      '{',
      repeat(choice($.component_instantiation, $.generate_block)), // Allow generate here
      '}'
    ),

    connections_block: $ => seq(
      $.kw_connections,
      '{',
      repeat(choice($.connection_statement, $.generate_block)), // Allow generate here
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

    // Define a simplified bound expression for generate loop ranges
    _simple_generate_bound: $ => choice(
      $.identifier,
      $.integer_literal
    ),

    // Define a simpler range specifically for generate loops using the simplified bounds
    _simple_generate_range: $ => choice(
      $.identifier, // Iterate over list
      seq(
        field('lower', $._simple_generate_bound),
        field('operator', token(choice('..', 'to', 'upto'))),
        field('upper', $._simple_generate_bound)
      )
    ),

    generate_for_statement: $ => seq(
      'for',
      field('variable', $.identifier),
      $.kw_in,
      field('range', $._simple_generate_range), // Use the new simplified range
      choice(
        seq(
          '{',
          field('body', repeat($._generate_body_item)),
          '}'
        ),
        seq(
          'loop',
          field('body', repeat($._generate_body_item)),
          $.kw_end, 'loop', ';'
        )
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


    pin_port_declaration: $ => seq(
        optional(choice('pin', 'port')), // Make keyword optional
        field('name', seq(field('base_name', $.identifier), optional(field('bus', $.bus_specifier)))),
        ':',
        choice(
          // Case 1: Ground (no direction, no subtype)
          $.kw_ground,
          // Case 2: Signal/Power (requires direction)
          seq(
            field('direction', $._pin_direction),
            // Make base_type optional, assume 'signal' if missing when direction is present
            optional(field('base_type', choice($.kw_signal, $.kw_power))),
            optional(seq('(', field('subtype', $._type_name), ')')), // Optional subtype in parens
          )
        ),
        // repeat($.attribute), // Attributes TBD
        ';'
    ),

    _pin_direction: $ => choice($.kw_in, $.kw_out, $.kw_inout),

    bus_specifier: $ => seq( // For things like DATA[7:0]
        '[',
        field('high', $._expression),
        optional(seq(':', field('low', $._expression))), // Allow single index or range
        ']'
    ),

    // Component Instantiation
     component_instantiation: $ => prec('instantiation', seq(
      field('type', $._type_name),
      field('name', seq(field('base_name', $.identifier), optional(field('bus', $.bus_specifier)))),
      // Parameters can use () or {} according to examples/needs clarification
      // For now, require one or the other
      choice(
         field('parameters_paren', $.component_parameter_list_paren),
         field('parameters_curly', $.component_parameter_list_curly)
      ),
      ';'
    )),

    component_parameter_list_paren: $ => seq(
      '(',
      optional(commaSep($.component_parameter_assignment)),
      ')'
    ),

    component_parameter_list_curly: $ => seq(
      '{',
      optional(commaSep($.component_parameter_assignment)),
      '}'
    ),

    component_parameter_assignment: $ => seq(
      field('name', $.identifier),
      '=', // Parameter assignments use '='
      field('value', $._expression)
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
      // Use token() to help lexer disambiguate from '.' in float literals
      field('operator', token(choice('..', 'to', 'upto'))),
      field('upper', $._expression)
    )),

    member_access: $ => prec.left('member', seq(
      field('object', $._expression), // Allow expressions like (complex_obj).member
      '.',
      field('property', $.identifier)
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
    float_literal: $ => /\d+\.\d*(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?|\d+[eE][+-]?\d+/,
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
    kw_library: $ => kw('library'), // Assuming 'library' might be used
    kw_use: $ => kw('use'),       // Assuming 'use' might be used
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
    kw_loop: $ => kw('loop'), // Added
    kw_all: $ => kw('all'), // Assuming 'all' might be used
    kw_in: $ => kw('in'),
    kw_out: $ => kw('out'),
    kw_inout: $ => kw('inout'),
    kw_signal: $ => kw('signal'),
    kw_power: $ => kw('power'),
    kw_ground: $ => kw('ground'), // Added
    kw_pin: $ => kw('pin'),     // Not technically a keyword in pin decl
    kw_port: $ => kw('port'),   // Not technically a keyword in port decl
    kw_time: $ => kw('time'),   // Example from spec, maybe type?
    kw_boolean: $ => kw('boolean'), // Maybe type?
    kw_string: $ => kw('string'),   // Maybe type?
    kw_char: $ => kw('char'),     // Maybe type?
    kw_physical: $ => kw('physical'), // Maybe type?

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
      $.range_expression, // Reference the main range expression rule
      $.identifier // Allow iteration over a list variable
    ),
  }
});