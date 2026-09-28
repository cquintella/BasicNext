# Module and LSP diagnostics
module-not-found = Module could not be loaded from {$path}.
    .title = Module not found
    .code = MODULE_NOT_FOUND
    .label = missing module
    .cause = no module file exists at that path on the module path
    .help = check the IMPORT name and the --module-path directories; the file name must match the module name
