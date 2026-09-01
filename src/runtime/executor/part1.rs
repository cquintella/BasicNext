#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn function(&mut self, function: &Function, arguments: Vec<Value>) -> Result<Flow, Diagnostic> {
        if arguments.len() != function.parameters.len() {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                format!(
                    "FUNCTION {} expects {} argument(s), found {}",
                    function.name,
                    function.parameters.len(),
                    arguments.len()
                ),
                function.span,
            ));
        }
        let mut symbols = function
            .parameters
            .iter()
            .copied()
            .zip(arguments)
            .collect::<HashMap<_, _>>();
        let mut values = HashMap::new();
        let mut block = function.entry;
        loop {
            let current = find_block(function, block)?;
            for instruction in &current.instructions {
                if let Some(hook) = self.debug_hook.as_deref_mut() {
                    hook(&function.name, instruction.span());
                }
                if let Some(control) = self.debug_control.as_deref_mut()
                    && control(
                        &function.name,
                        instruction.span(),
                        &debug_variables(&symbols, &values),
                    ) == DebugDecision::Terminate
                {
                    return Err(runtime_error(
                        "DEBUG_TERMINATED",
                        "execution terminated by debugger",
                        instruction.span(),
                    ));
                }
                self.instruction(instruction, &mut symbols, &mut values)?;
                if let Some(code) = self.stop_code.take() {
                    return Ok(Flow::Stop(code));
                }
            }
            match &current.terminator {
                Terminator::Jump { target } => block = *target,
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    block = if boolean(value(&values, *condition, function.span)?, function.span)? {
                        *then_block
                    } else {
                        *else_block
                    };
                }
                Terminator::Return { value: result } => {
                    return Ok(Flow::Return(
                        result
                            .map(|result| value(&values, result, function.span).cloned())
                            .transpose()?,
                    ));
                }
                Terminator::Stop { code } => {
                    let code = integer(value(&values, *code, function.span)?, function.span)?.0;
                    return Ok(Flow::Stop(code));
                }
            }
        }
    }

    pub(crate) fn ensure_class(&mut self, class: &str, span: Span) -> Result<(), Diagnostic> {
        match self.class_init.get(class).copied() {
            Some(ClassInit::Ready) => return Ok(()),
            Some(ClassInit::Running) => {
                return Err(runtime_error(
                    "STATIC_INITIALIZATION_CYCLE",
                    format!("STATIC initialization of {class} reentered"),
                    span,
                ));
            }
            None => {}
        }
        self.class_init
            .insert(class.to_string(), ClassInit::Running);
        let init_name = format!("{class}.$init");
        if let Some(index) = self
            .module
            .functions
            .iter()
            .position(|function| function.name == init_name)
        {
            let function = &self.module.functions[index];
            match self.function(function, Vec::new())? {
                Flow::Stop(code) => self.stop_code = Some(code),
                Flow::Return(_) => {}
            }
        }
        self.class_init.insert(class.to_string(), ClassInit::Ready);
        Ok(())
    }

}
