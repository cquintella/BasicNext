# Parser diagnostics
parse-error = Expected {$expected}.
    .title = Syntax error
    .code = E0100
    .label = syntax error
    .cause = the source at this position does not continue the construct as its syntax requires
    .help = compare the line with the construct's syntax in the language reference; an earlier unclosed block or a misspelled keyword is a common origin
parse-rule = {$rule}.
    .title = Syntax error
    .code = E0101
    .label = syntax error
    .cause = the construct is complete but breaks a rule of the grammar
    .help = rewrite the construct as the rule in the message requires
