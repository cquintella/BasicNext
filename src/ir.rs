use crate::{
    ast::{
        Block as AstBlock, DeclarationKind, Expression, ExpressionKind, ForHeader, Item, Literal,
        Program, Statement, TypeReference,
    },
    diagnostic::Diagnostic,
    semantic::{SemanticModel, SymbolId, Type},
    source::Span,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValueId(pub u32);

#[derive(Debug)]
pub struct Module {
    pub source_name: Option<String>,
    pub functions: Vec<Function>,
}

#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<SymbolId>,
    pub return_type: Type,
    pub entry: BlockId,
    pub blocks: Vec<BasicBlock>,
    pub span: Span,
}

#[derive(Debug)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

#[derive(Debug)]
pub enum Instruction {
    Constant {
        destination: ValueId,
        value: Constant,
        ty: Type,
        span: Span,
    },
    Default {
        destination: ValueId,
        ty: Type,
        dimensions: Vec<usize>,
        span: Span,
    },
    Load {
        destination: ValueId,
        symbol: SymbolId,
        ty: Type,
        span: Span,
    },
    Store {
        symbol: SymbolId,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    Copy {
        destination: ValueId,
        source: ValueId,
        ty: Type,
        span: Span,
    },
    Unary {
        destination: ValueId,
        operator: String,
        operand: ValueId,
        ty: Type,
        span: Span,
    },
    Binary {
        destination: ValueId,
        operator: String,
        left: ValueId,
        right: ValueId,
        ty: Type,
        span: Span,
    },
    Cast {
        destination: ValueId,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    Call {
        destination: ValueId,
        callee: ValueId,
        arguments: Vec<ValueId>,
        ty: Type,
        span: Span,
    },
    Input {
        destination: ValueId,
        ty: Type,
        span: Span,
    },
    Vector {
        destination: ValueId,
        values: Vec<ValueId>,
        ty: Type,
        span: Span,
    },
    Index {
        destination: ValueId,
        object: ValueId,
        index: ValueId,
        ty: Type,
        span: Span,
    },
    Member {
        destination: ValueId,
        object: ValueId,
        name: String,
        ty: Type,
        span: Span,
    },
    SetIndex {
        symbol: SymbolId,
        indices: Vec<ValueId>,
        value: ValueId,
        ty: Type,
        span: Span,
    },
    Length {
        destination: ValueId,
        vector: ValueId,
        span: Span,
    },
    Print {
        values: Vec<ValueId>,
        span: Span,
    },
}

#[derive(Debug)]
pub enum Terminator {
    Jump {
        target: BlockId,
    },
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    },
    Return {
        value: Option<ValueId>,
    },
    Stop {
        code: ValueId,
    },
}

#[derive(Debug)]
pub enum Constant {
    Integer(String),
    Float(String),
    String(String),
    Boolean(bool),
    Null,
    NotAvailable,
    EndOfFile,
    Function(String),
    Type(String),
}

struct OpenBlock {
    instructions: Vec<Instruction>,
    terminator: Option<Terminator>,
}

struct LoopTargets {
    kind: &'static str,
    exit: BlockId,
    continue_at: BlockId,
}

struct Builder<'a> {
    model: &'a SemanticModel,
    blocks: Vec<OpenBlock>,
    current: BlockId,
    next_value: u32,
    loops: Vec<LoopTargets>,
}

/// Lowers a semantically valid program to typed BN control-flow IR.
///
/// # Errors
///
/// Returns a source-spanned diagnostic if the AST contains a construct that
/// is not part of the core IR yet or if semantic resolution data is missing.
pub fn lower(program: &Program, model: &SemanticModel) -> Result<Module, Diagnostic> {
    let mut functions = Vec::new();
    for item in &program.items {
        let Item::Declaration {
            kind: DeclarationKind::Function,
            name,
            signature: Some(signature),
            statements,
            span,
            ..
        } = item
        else {
            continue;
        };
        let mut builder = Builder::new(model);
        builder.statements(statements)?;
        if !builder.terminated() {
            builder.terminate(Terminator::Return { value: None });
        }
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| builder.symbol(parameter.span))
            .collect::<Result<Vec<_>, _>>()?;
        functions.push(Function {
            name: name.clone(),
            parameters,
            return_type: function_return_type(model, *span)?,
            entry: BlockId(0),
            blocks: builder.finish()?,
            span: *span,
        });
    }
    let module = Module {
        source_name: program.source_name.clone(),
        functions,
    };
    validate(&module)?;
    Ok(module)
}

/// Checks structural invariants required by every BN IR consumer.
///
/// # Errors
///
/// Returns `INVALID_IR` if an entry point or terminator references a block that
/// does not exist.
pub fn validate(module: &Module) -> Result<(), Diagnostic> {
    for function in &module.functions {
        let block_count = u32::try_from(function.blocks.len())
            .map_err(|_| ir_error("function has too many basic blocks", function.span))?;
        if function.entry.0 >= block_count {
            return Err(ir_error(
                "function entry block does not exist",
                function.span,
            ));
        }
        for (index, block) in function.blocks.iter().enumerate() {
            if block.id.0
                != u32::try_from(index)
                    .map_err(|_| ir_error("function has too many basic blocks", function.span))?
            {
                return Err(ir_error(
                    "basic block IDs must be dense and ordered",
                    function.span,
                ));
            }
            let targets: &[BlockId] = match &block.terminator {
                Terminator::Jump { target } => std::slice::from_ref(target),
                Terminator::Branch { then_block, .. } => std::slice::from_ref(then_block),
                Terminator::Return { .. } | Terminator::Stop { .. } => &[],
            };
            if targets.iter().any(|target| target.0 >= block_count)
                || matches!(&block.terminator, Terminator::Branch { else_block, .. } if else_block.0 >= block_count)
            {
                return Err(ir_error(
                    "terminator references a basic block that does not exist",
                    function.span,
                ));
            }
        }
    }
    Ok(())
}

impl<'a> Builder<'a> {
    fn new(model: &'a SemanticModel) -> Self {
        Self {
            model,
            blocks: vec![OpenBlock {
                instructions: Vec::new(),
                terminator: None,
            }],
            current: BlockId(0),
            next_value: 0,
            loops: Vec::new(),
        }
    }

    fn statements(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        for statement in statements {
            if self.terminated() {
                break;
            }
            self.statement(statement)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // IR cases mirror the statement AST.
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::Binding {
                initializer,
                type_ref,
                span,
                ..
            } => {
                let ty = type_at(self.model, *span)?;
                let value = if let Some(initializer) = initializer {
                    self.expression(initializer)?
                } else {
                    self.default_value(ty.clone(), type_ref, *span)?
                };
                self.emit(Instruction::Store {
                    symbol: self.symbol(*span)?,
                    value,
                    ty,
                    span: *span,
                });
            }
            Statement::Assignment {
                target,
                operator,
                value,
                span,
            } => {
                let (symbol, indices) = self.assignment_place(target)?;
                let mut result = self.expression(value)?;
                if operator != "Assign" {
                    let left = self.expression(target)?;
                    let destination = self.value();
                    self.emit(Instruction::Binary {
                        destination,
                        operator: assignment_operator(operator)?.into(),
                        left,
                        right: result,
                        ty: type_at(self.model, target.span)?,
                        span: *span,
                    });
                    result = destination;
                }
                if indices.is_empty() {
                    self.emit(Instruction::Store {
                        symbol,
                        value: result,
                        ty: type_at(self.model, target.span)?,
                        span: *span,
                    });
                } else {
                    self.emit(Instruction::SetIndex {
                        symbol,
                        indices,
                        value: result,
                        ty: type_at(self.model, target.span)?,
                        span: *span,
                    });
                }
            }
            Statement::Print { values, span } => {
                let values = values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?;
                self.emit(Instruction::Print {
                    values,
                    span: *span,
                });
            }
            Statement::Call { expression, .. } => {
                self.expression(expression)?;
            }
            Statement::Return { value, .. } => {
                let value = value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?;
                self.terminate(Terminator::Return { value });
            }
            Statement::Stop { code, .. } => {
                let code = self.expression(code)?;
                self.terminate(Terminator::Stop { code });
            }
            Statement::If {
                branches,
                otherwise,
                ..
            } => self.if_statement(branches, otherwise.as_ref())?,
            Statement::While {
                condition, body, ..
            } => self.while_statement(condition, body)?,
            Statement::Repeat {
                body, condition, ..
            } => self.repeat_statement(body, condition)?,
            Statement::For { header, body, span } => self.for_statement(header, body, *span)?,
            Statement::Control { kind, target, span } => {
                let targets = self
                    .loops
                    .iter()
                    .rev()
                    .find(|targets| targets.kind == target)
                    .ok_or_else(|| ir_error("loop target is missing", *span))?;
                let destination = if kind == "EXIT" {
                    targets.exit
                } else {
                    targets.continue_at
                };
                self.terminate(Terminator::Jump {
                    target: destination,
                });
            }
            Statement::Delete { span, .. } | Statement::MemberFunction { span, .. } => {
                return Err(ir_error(
                    "object and pointer lowering belongs to the extended runtime",
                    *span,
                ));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Expression alternatives mirror the syntax AST.
    fn expression(&mut self, expression: &Expression) -> Result<ValueId, Diagnostic> {
        let destination = self.value();
        let ty = type_at(self.model, expression.span)?;
        let instruction = match &expression.kind {
            ExpressionKind::Literal(literal) => Instruction::Constant {
                destination,
                value: constant(literal),
                ty,
                span: expression.span,
            },
            ExpressionKind::Name { name } => {
                if matches!(ty, Type::Function { .. }) {
                    Instruction::Constant {
                        destination,
                        value: Constant::Function(name.clone()),
                        ty,
                        span: expression.span,
                    }
                } else {
                    Instruction::Load {
                        destination,
                        symbol: self.expression_symbol(expression)?,
                        ty,
                        span: expression.span,
                    }
                }
            }
            ExpressionKind::Input => Instruction::Input {
                destination,
                ty,
                span: expression.span,
            },
            ExpressionKind::TypeTest { .. } => {
                return Err(ir_error(
                    "type tests are lowered only as the right operand of IS",
                    expression.span,
                ));
            }
            ExpressionKind::Unary { operator, operand } => Instruction::Unary {
                destination,
                operator: operator.clone(),
                operand: self.expression(operand)?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                if matches!(operator.as_str(), "AND" | "OR") && ty == Type::Boolean {
                    return self.short_circuit(destination, operator, left, right, expression.span);
                }
                let left = self.expression(left)?;
                let right = if operator == "IS" {
                    self.type_test(right)?
                } else {
                    self.expression(right)?
                };
                Instruction::Binary {
                    destination,
                    operator: operator.clone(),
                    left,
                    right,
                    ty,
                    span: expression.span,
                }
            }
            ExpressionKind::Call { callee, arguments } => Instruction::Call {
                destination,
                callee: self.expression(callee)?,
                arguments: arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Cast { value, .. } => Instruction::Cast {
                destination,
                value: self.expression(value)?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Vector { values } => Instruction::Vector {
                destination,
                values: values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Index { object, index } => Instruction::Index {
                destination,
                object: self.expression(object)?,
                index: self.expression(index)?,
                ty,
                span: expression.span,
            },
            ExpressionKind::Member { object, name } => {
                if let ExpressionKind::Name { name: namespace } = &object.kind
                    && matches!(namespace.as_str(), "Math" | "Float")
                {
                    Instruction::Constant {
                        destination,
                        value: Constant::Function(format!("{namespace}.{name}")),
                        ty,
                        span: expression.span,
                    }
                } else {
                    Instruction::Member {
                        destination,
                        object: self.expression(object)?,
                        name: name.clone(),
                        ty,
                        span: expression.span,
                    }
                }
            }
            ExpressionKind::New { .. } => {
                return Err(ir_error(
                    "object and pointer expressions belong to the extended runtime",
                    expression.span,
                ));
            }
        };
        self.emit(instruction);
        Ok(destination)
    }

    fn short_circuit(
        &mut self,
        destination: ValueId,
        operator: &str,
        left: &Expression,
        right: &Expression,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let left = self.expression(left)?;
        let right_block = self.block();
        let short_block = self.block();
        let join = self.block();
        let (then_block, else_block, short_value) = if operator == "AND" {
            (right_block, short_block, false)
        } else {
            (short_block, right_block, true)
        };
        self.terminate(Terminator::Branch {
            condition: left,
            then_block,
            else_block,
        });
        self.current = short_block;
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Boolean(short_value),
            ty: Type::Boolean,
            span,
        });
        self.terminate(Terminator::Jump { target: join });
        self.current = right_block;
        let right = self.expression(right)?;
        self.emit(Instruction::Copy {
            destination,
            source: right,
            ty: Type::Boolean,
            span,
        });
        self.terminate(Terminator::Jump { target: join });
        self.current = join;
        Ok(destination)
    }

    fn type_test(&mut self, expression: &Expression) -> Result<ValueId, Diagnostic> {
        let value = match &expression.kind {
            ExpressionKind::TypeTest { type_ref } => type_ref
                .alternatives
                .first()
                .map(|atom| {
                    std::iter::once(atom.name.as_str())
                        .chain(atom.parts.iter().map(String::as_str))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .ok_or_else(|| ir_error("invalid IS type test", expression.span))?,
            ExpressionKind::Name { name } | ExpressionKind::Literal(Literal::TypeName(name)) => {
                name.clone()
            }
            ExpressionKind::Literal(Literal::Null) => "NULL".into(),
            ExpressionKind::Literal(Literal::NotAvailable) => "NA".into(),
            ExpressionKind::Literal(Literal::EndOfFile) => "EOF".into(),
            ExpressionKind::Literal(Literal::Special(value)) => value.clone(),
            ExpressionKind::Unary { operator, operand } if operator == "Minus" => {
                let ExpressionKind::Literal(Literal::Special(value)) = &operand.kind else {
                    return Err(ir_error("invalid IS test", expression.span));
                };
                format!("-{value}")
            }
            _ => return Err(ir_error("invalid IS test", expression.span)),
        };
        let destination = self.value();
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Type(value.clone()),
            ty: Type::TypeName(value.clone()),
            span: expression.span,
        });
        Ok(destination)
    }

    fn if_statement(
        &mut self,
        branches: &[crate::ast::IfBranch],
        otherwise: Option<&AstBlock>,
    ) -> Result<(), Diagnostic> {
        let join = self.block();
        for branch in branches {
            let body = self.block();
            let next = self.block();
            let condition = self.expression(&branch.condition)?;
            self.terminate(Terminator::Branch {
                condition,
                then_block: body,
                else_block: next,
            });
            self.current = body;
            self.statements(&branch.body.statements)?;
            self.jump_if_open(join);
            self.current = next;
        }
        if let Some(otherwise) = otherwise {
            self.statements(&otherwise.statements)?;
        }
        self.jump_if_open(join);
        self.current = join;
        Ok(())
    }

    fn while_statement(
        &mut self,
        condition: &Expression,
        body: &AstBlock,
    ) -> Result<(), Diagnostic> {
        let condition_block = self.block();
        let body_block = self.block();
        let exit = self.block();
        self.terminate(Terminator::Jump {
            target: condition_block,
        });
        self.current = condition_block;
        let value = self.expression(condition)?;
        self.terminate(Terminator::Branch {
            condition: value,
            then_block: body_block,
            else_block: exit,
        });
        self.current = body_block;
        self.loops.push(LoopTargets {
            kind: "WHILE",
            exit,
            continue_at: condition_block,
        });
        self.statements(&body.statements)?;
        self.loops.pop();
        self.jump_if_open(condition_block);
        self.current = exit;
        Ok(())
    }

    fn repeat_statement(
        &mut self,
        body: &AstBlock,
        condition: &Expression,
    ) -> Result<(), Diagnostic> {
        let body_block = self.block();
        let condition_block = self.block();
        let exit = self.block();
        self.terminate(Terminator::Jump { target: body_block });
        self.current = body_block;
        self.loops.push(LoopTargets {
            kind: "REPEAT",
            exit,
            continue_at: condition_block,
        });
        self.statements(&body.statements)?;
        self.loops.pop();
        self.jump_if_open(condition_block);
        self.current = condition_block;
        let value = self.expression(condition)?;
        self.terminate(Terminator::Branch {
            condition: value,
            then_block: exit,
            else_block: body_block,
        });
        self.current = exit;
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Counted and FOR EACH lowering share loop target handling.
    fn for_statement(
        &mut self,
        header: &ForHeader,
        body: &AstBlock,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match header {
            ForHeader::Counted {
                type_ref,
                start,
                end,
                step,
                ..
            } => {
                let symbol = self.symbol(type_ref.span)?;
                let start = self.expression(start)?;
                let end = self.expression(end)?;
                let step = if let Some(step) = step {
                    self.expression(step)?
                } else {
                    self.integer_one(type_at(self.model, type_ref.span)?, span)
                };
                self.emit(Instruction::Store {
                    symbol,
                    value: start,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                let condition_block = self.block();
                let body_block = self.block();
                let next_block = self.block();
                let exit = self.block();
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = condition_block;
                let current = self.load(symbol, type_at(self.model, type_ref.span)?, span);
                let condition = self.value();
                let for_condition = self.function_constant("$for_condition", span);
                self.emit(Instruction::Call {
                    destination: condition,
                    callee: for_condition,
                    arguments: vec![current, end, step],
                    ty: Type::Boolean,
                    span,
                });
                self.terminate(Terminator::Branch {
                    condition,
                    then_block: body_block,
                    else_block: exit,
                });
                self.current = body_block;
                self.loops.push(LoopTargets {
                    kind: "FOR",
                    exit,
                    continue_at: next_block,
                });
                self.statements(&body.statements)?;
                self.loops.pop();
                self.jump_if_open(next_block);
                self.current = next_block;
                let current = self.load(symbol, type_at(self.model, type_ref.span)?, span);
                let next = self.value();
                self.emit(Instruction::Binary {
                    destination: next,
                    operator: "Plus".into(),
                    left: current,
                    right: step,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.emit(Instruction::Store {
                    symbol,
                    value: next,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = exit;
            }
            ForHeader::Each {
                type_ref, iterable, ..
            } => {
                let symbol = self.symbol(type_ref.span)?;
                let vector = self.expression(iterable)?;
                let index = self.value();
                self.emit(Instruction::Constant {
                    destination: index,
                    value: Constant::Integer("0".into()),
                    ty: Type::Integer(crate::semantic::IntegerType::Int32),
                    span,
                });
                let length = self.value();
                self.emit(Instruction::Length {
                    destination: length,
                    vector,
                    span,
                });
                let condition_block = self.block();
                let body_block = self.block();
                let next_block = self.block();
                let exit = self.block();
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = condition_block;
                let condition = self.value();
                self.emit(Instruction::Binary {
                    destination: condition,
                    operator: "Less".into(),
                    left: index,
                    right: length,
                    ty: Type::Boolean,
                    span,
                });
                self.terminate(Terminator::Branch {
                    condition,
                    then_block: body_block,
                    else_block: exit,
                });
                self.current = body_block;
                let element = self.value();
                self.emit(Instruction::Index {
                    destination: element,
                    object: vector,
                    index,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.emit(Instruction::Store {
                    symbol,
                    value: element,
                    ty: type_at(self.model, type_ref.span)?,
                    span,
                });
                self.loops.push(LoopTargets {
                    kind: "FOR",
                    exit,
                    continue_at: next_block,
                });
                self.statements(&body.statements)?;
                self.loops.pop();
                self.jump_if_open(next_block);
                self.current = next_block;
                let one =
                    self.integer_one(Type::Integer(crate::semantic::IntegerType::Int32), span);
                self.emit(Instruction::Binary {
                    destination: index,
                    operator: "Plus".into(),
                    left: index,
                    right: one,
                    ty: Type::Integer(crate::semantic::IntegerType::Int32),
                    span,
                });
                self.terminate(Terminator::Jump {
                    target: condition_block,
                });
                self.current = exit;
            }
        }
        Ok(())
    }

    fn integer_one(&mut self, ty: Type, span: Span) -> ValueId {
        let destination = self.value();
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Integer("1".into()),
            ty,
            span,
        });
        destination
    }

    fn default_value(
        &mut self,
        ty: Type,
        type_ref: &TypeReference,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        if matches!(
            ty,
            Type::Named(_)
                | Type::TypeName(_)
                | Type::ImportedNamed { .. }
                | Type::ImportedTypeName { .. }
                | Type::Pointer { .. }
                | Type::Function { .. }
        ) {
            return Err(ir_error(
                "object, pointer, and function defaults are unavailable",
                span,
            ));
        }
        let destination = self.value();
        let dimensions = type_ref
            .alternatives
            .first()
            .map(|atom| {
                atom.parts
                    .windows(3)
                    .filter(|parts| parts[0] == "LeftBracket" && parts[2] == "RightBracket")
                    .map(|parts| {
                        parts[1]
                            .parse::<usize>()
                            .map_err(|_| ir_error("invalid vector dimension", span))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        self.emit(Instruction::Default {
            destination,
            ty,
            dimensions,
            span,
        });
        Ok(destination)
    }

    fn function_constant(&mut self, name: &str, span: Span) -> ValueId {
        let destination = self.value();
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Function(name.into()),
            ty: Type::Unknown,
            span,
        });
        destination
    }

    fn load(&mut self, symbol: SymbolId, ty: Type, span: Span) -> ValueId {
        let destination = self.value();
        self.emit(Instruction::Load {
            destination,
            symbol,
            ty,
            span,
        });
        destination
    }

    fn expression_symbol(&self, expression: &Expression) -> Result<SymbolId, Diagnostic> {
        self.model
            .expression(expression.span)
            .and_then(|expression| expression.symbol_id)
            .ok_or_else(|| ir_error("expression has no resolved SymbolId", expression.span))
    }

    fn assignment_place(
        &mut self,
        expression: &Expression,
    ) -> Result<(SymbolId, Vec<ValueId>), Diagnostic> {
        let mut indices = Vec::new();
        let mut current = expression;
        while let ExpressionKind::Index { object, index } = &current.kind {
            indices.push(self.expression(index)?);
            current = object;
        }
        indices.reverse();
        if !matches!(current.kind, ExpressionKind::Name { .. }) {
            return Err(ir_error(
                "member assignment lowering belongs to the extended runtime",
                expression.span,
            ));
        }
        Ok((self.expression_symbol(current)?, indices))
    }

    fn symbol(&self, span: Span) -> Result<SymbolId, Diagnostic> {
        self.model
            .symbol_at(span)
            .map(|symbol| symbol.id)
            .ok_or_else(|| ir_error("declaration has no resolved SymbolId", span))
    }

    fn block(&mut self) -> BlockId {
        let id = BlockId(u32::try_from(self.blocks.len()).expect("IR block count fits u32"));
        self.blocks.push(OpenBlock {
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    fn value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    fn emit(&mut self, instruction: Instruction) {
        self.blocks[self.current.0 as usize]
            .instructions
            .push(instruction);
    }

    fn terminate(&mut self, terminator: Terminator) {
        self.blocks[self.current.0 as usize].terminator = Some(terminator);
    }

    fn terminated(&self) -> bool {
        self.blocks[self.current.0 as usize].terminator.is_some()
    }

    fn jump_if_open(&mut self, target: BlockId) {
        if !self.terminated() {
            self.terminate(Terminator::Jump { target });
        }
    }

    fn finish(self) -> Result<Vec<BasicBlock>, Diagnostic> {
        self.blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                Ok(BasicBlock {
                    id: BlockId(u32::try_from(index).expect("IR block count fits u32")),
                    instructions: block.instructions,
                    terminator: block.terminator.ok_or_else(|| {
                        ir_error("generated basic block has no terminator", default_span())
                    })?,
                })
            })
            .collect()
    }
}

fn type_at(model: &SemanticModel, span: Span) -> Result<Type, Diagnostic> {
    model
        .expression(span)
        .map(|expression| expression.ty.clone())
        .or_else(|| model.symbol_at(span).map(|symbol| symbol.ty.clone()))
        .ok_or_else(|| ir_error("type information is missing for IR lowering", span))
}

fn function_return_type(model: &SemanticModel, span: Span) -> Result<Type, Diagnostic> {
    let Some(Type::Function { return_type, .. }) = model.symbol_at(span).map(|symbol| &symbol.ty)
    else {
        return Err(ir_error("function type information is missing", span));
    };
    Ok((**return_type).clone())
}

fn constant(literal: &Literal) -> Constant {
    match literal {
        Literal::Integer(value) => Constant::Integer(value.clone()),
        Literal::Float(value) | Literal::Special(value) => Constant::Float(value.clone()),
        Literal::String(value) => Constant::String(value.clone()),
        Literal::TypeName(value) => Constant::Type(value.clone()),
        Literal::Boolean(value) => Constant::Boolean(*value),
        Literal::Null => Constant::Null,
        Literal::NotAvailable => Constant::NotAvailable,
        Literal::EndOfFile => Constant::EndOfFile,
    }
}

fn assignment_operator(operator: &str) -> Result<&'static str, Diagnostic> {
    match operator {
        "PlusAssign" => Ok("Plus"),
        "MinusAssign" => Ok("Minus"),
        "StarAssign" => Ok("Star"),
        "SlashAssign" => Ok("Slash"),
        "PercentAssign" => Ok("Percent"),
        "PowerAssign" => Ok("Power"),
        _ => Err(ir_error("unknown assignment operator", default_span())),
    }
}

fn ir_error(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code: "IR_LOWERING",
        message: message.into(),
        span,
    }
}

fn default_span() -> Span {
    Span {
        start: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}
