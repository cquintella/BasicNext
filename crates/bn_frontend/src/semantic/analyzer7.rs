#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn math_call(
        &mut self,
        name: &str,
        arguments: &[Expression],
        locals: &HashMap<String, Symbol>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let expected_count = match name {
            "MIN" | "MAX" | "ROUND" | "POW" | "ATAN2" | "HYPOT" | "TOTIMESTAMP" => 2,
            "FMA" => 3,
            _ => 1,
        };
        if matches!(name, "MIN" | "MAX") && arguments.len() == 1 {
            let ty = self.expression(&arguments[0], locals)?;
            if matches!(ty, Type::Vector { ref element, .. } if is_numeric(element))
                || matches!(ty, Type::Pointer { ref element, .. } if is_numeric(element))
            {
                return Ok(match ty {
                    Type::Vector { element, .. } | Type::Pointer { element, .. } => *element,
                    _ => unreachable!(),
                });
            }
            return Err(error(
                "TYPE_MISMATCH",
                "BNMath.MIN/MAX expects a numeric vector",
                span,
            ));
        }
        if !(matches!(name, "MIN" | "MAX") && arguments.len() == 1)
            && arguments.len() != expected_count
        {
            return Err(error(
                "TYPE_MISMATCH",
                format!("BNMath.{name} expects {expected_count} argument(s)"),
                span,
            ));
        }
        let types = arguments
            .iter()
            .map(|argument| self.expression(argument, locals))
            .collect::<Result<Vec<_>, _>>()?;
        if name == "VAL" {
            if types[0] != Type::String {
                return Err(error("TYPE_MISMATCH", "BNMath.VAL expects STRING", span));
            }
            return Ok(Type::Float(FloatType::Float64));
        }
        if matches!(
            name,
            "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "MODE" | "STDEV" | "VARIANCE" | "RANGE"
        ) {
            let valid = matches!(types[0], Type::Vector { ref element, .. } if is_numeric(element))
                || matches!(types[0], Type::Pointer { ref element, .. } if is_numeric(element));
            if !valid {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("BNMath.{name} expects a numeric vector"),
                    span,
                ));
            }
            return Ok(if name == "MODE" {
                Type::Alternative(vec![Type::Float(FloatType::Float64), Type::NotAvailable])
            } else {
                Type::Float(FloatType::Float64)
            });
        }
        if matches!(name, "TOHOUR" | "TOWEEKDAY") {
            if !matches!(types[0], Type::Integer(IntegerType::Int64)) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("BNMath.{name} expects TIMESTAMP"),
                    span,
                ));
            }
            return Ok(Type::Integer(IntegerType::Int32));
        }
        if matches!(name, "TODATE" | "TOTIME") {
            if !matches!(types[0], Type::Integer(IntegerType::Int64)) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("BNMath.{name} expects TIMESTAMP"),
                    span,
                ));
            }
            return Ok(Type::Named(
                if name == "TODATE" { "DATE" } else { "TIME" }.into(),
            ));
        }
        if name == "TOTIMESTAMP" {
            if types != [Type::Named("DATE".into()), Type::Named("TIME".into())] {
                return Err(error(
                    "TYPE_MISMATCH",
                    "BNMath.TOTIMESTAMP expects DATE and TIME",
                    span,
                ));
            }
            return Ok(Type::Integer(IntegerType::Int64));
        }
        if name == "ROUND" {
            if !is_float(&types[0]) || !is_integer(&types[1]) {
                return Err(error(
                    "TYPE_MISMATCH",
                    "BNMath.ROUND expects a floating value and an INTEGER digit count",
                    span,
                ));
            }
            return Ok(default_literal_type(types[0].clone()));
        }
        if matches!(name, "ABS" | "MIN" | "MAX" | "SIGN") {
            let mut result = types[0].clone();
            for ty in &types[1..] {
                result = numeric_result(&result, ty).ok_or_else(|| {
                    error(
                        "TYPE_MISMATCH",
                        format!("BNMath.{name} requires compatible numeric arguments"),
                        span,
                    )
                })?;
            }
            if !is_numeric(&result) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("BNMath.{name} requires numeric arguments"),
                    span,
                ));
            }
            return Ok(default_literal_type(result));
        }
        if types.iter().any(|ty| !is_float(ty)) {
            return Err(error(
                "TYPE_MISMATCH",
                format!("BNMath.{name} requires floating-point arguments"),
                span,
            ));
        }
        let result = types
            .into_iter()
            .reduce(|left, right| numeric_result(&left, &right).expect("floats are compatible"))
            .expect("BNMath functions have arguments");
        Ok(default_literal_type(result))
    }
    pub(crate) fn declare_global(
        &mut self,
        name: &str,
        ty: Type,
        constant: bool,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let symbol = self.symbol(ty, constant);
        if self.globals.insert(name.into(), symbol).is_some() {
            return Err(error(
                "DUPLICATE_NAME",
                format!("duplicate top-level declaration '{name}'"),
                span,
            ));
        }
        let record = self.globals.get(name).expect("inserted global").clone();
        self.record_symbol(name, &record, span);
        Ok(())
    }
    pub(crate) fn declare_local(
        &mut self,
        locals: &mut HashMap<String, Symbol>,
        name: &str,
        ty: Type,
        constant: bool,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let symbol = self.symbol(ty, constant);
        if locals.insert(name.into(), symbol).is_some() {
            return Err(error(
                "DUPLICATE_NAME",
                format!("duplicate binding '{name}' in the same scope"),
                span,
            ));
        }
        let record = locals.get(name).expect("inserted local").clone();
        self.record_symbol(name, &record, span);
        Ok(())
    }
    pub(crate) fn symbol(&mut self, ty: Type, constant: bool) -> Symbol {
        let symbol = Symbol {
            id: SymbolId(self.next_symbol),
            type_id: TypeId(self.next_type),
            declared_ty: ty.clone(),
            ty,
            constant,
        };
        self.next_symbol += 1;
        self.next_type += 1;
        symbol
    }
    pub(crate) fn record_symbol(&mut self, name: &str, symbol: &Symbol, span: Span) {
        self.symbols.push(ResolvedSymbol {
            id: symbol.id,
            type_id: symbol.type_id,
            name: name.into(),
            type_name: display(&symbol.ty),
            ty: symbol.ty.clone(),
            constant: symbol.constant,
            span,
        });
    }
    pub(crate) fn lookup<'a>(
        &'a self,
        name: &str,
        locals: &'a HashMap<String, Symbol>,
    ) -> Option<&'a Symbol> {
        locals.get(name).or_else(|| self.globals.get(name))
    }

    pub(crate) fn compatible(&self, expected: &Type, actual: &Type) -> bool {
        if compatible(expected, actual) {
            return true;
        }
        match (expected, actual) {
            (Type::Named(expected), Type::Named(actual))
                if self.is_subclass_of(actual, expected) =>
            {
                true
            }
            (Type::ImportedNamed { module, name }, Type::Named(class))
                if self.is_subclass_of(class, &format!("#{}.{name}", module.0)) =>
            {
                true
            }
            (
                Type::ImportedNamed {
                    module,
                    name: interface,
                },
                Type::Named(class),
            ) => self.implementations.get(class).is_some_and(|interfaces| {
                interfaces.iter().any(|implemented| {
                    implemented.split_once('.').is_some_and(|(alias, name)| {
                        name == interface && self.module_imports.get(alias) == Some(module)
                    })
                })
            }),
            (Type::Named(interface), Type::Named(class)) => self
                .implementations
                .get(class)
                .is_some_and(|interfaces| interfaces.contains(interface)),
            (
                Type::ImportedNamed {
                    module: interface_module,
                    name: interface,
                },
                Type::ImportedNamed {
                    module: class_module,
                    name: class,
                },
            ) if interface_module == class_module => self
                .imported_types
                .get(&(*class_module, class.clone()))
                .is_some_and(|info| info.interfaces.contains(interface)),
            (Type::Alternative(expected), actual) => expected
                .iter()
                .any(|expected| self.compatible(expected, actual)),
            (expected, Type::Alternative(actual)) => actual
                .iter()
                .all(|actual| self.compatible(expected, actual)),
            (
                Type::Vector {
                    element: expected,
                    dimensions: expected_dimensions,
                },
                Type::Vector {
                    element: actual,
                    dimensions: actual_dimensions,
                },
            ) => {
                (expected_dimensions == actual_dimensions
                    || (expected_dimensions.len() == 1 && expected_dimensions[0] == u64::MAX))
                    && self.compatible(expected, actual)
            }
            _ => false,
        }
    }

    pub(crate) fn is_subclass_of(&self, class: &str, ancestor: &str) -> bool {
        let mut current = class;
        while let Some(base) = self.base_classes.get(current) {
            if base == ancestor {
                return true;
            }
            current = base;
        }
        false
    }

    pub(crate) fn member_owner(&self, class: &str, name: &str) -> Option<String> {
        if self
            .declared_members
            .get(class)
            .is_some_and(|members| members.contains(name))
        {
            return Some(class.into());
        }
        self.base_classes
            .get(class)
            .and_then(|base| self.member_owner(base, name))
            .or_else(|| Some(class.into()))
    }

    pub(crate) fn deletable(&self, ty: &Type) -> bool {
        match ty {
            Type::Pointer { .. } | Type::Null => true,
            Type::Named(name) => {
                name == "FS.File"
                    || self.declaration_kinds.get(name) == Some(&DeclarationKind::Class)
            }
            Type::ImportedNamed { module, name } => self
                .imported_types
                .get(&(*module, name.clone()))
                .is_some_and(|info| info.kind == DeclarationKind::Class),
            Type::Alternative(alternatives) => alternatives
                .iter()
                .all(|alternative| self.deletable(alternative)),
            _ => false,
        }
    }

    pub(crate) fn member_mutable(&self, object: &Type, name: &str) -> bool {
        match object {
            Type::Named(owner) | Type::TypeName(owner) => self
                .members
                .get(owner)
                .and_then(|members| members.get(name))
                .is_some_and(|member| member.mutable),
            Type::ImportedNamed {
                module,
                name: owner,
            }
            | Type::ImportedTypeName {
                module,
                name: owner,
            } => self
                .imported_types
                .get(&(*module, owner.clone()))
                .and_then(|info| info.members.get(name))
                .is_some_and(|member| member.mutable),
            Type::Alternative(alternatives) => alternatives
                .iter()
                .filter(|alternative| !matches!(alternative, Type::Null | Type::NotAvailable))
                .all(|alternative| self.member_mutable(alternative, name)),
            _ => false,
        }
    }

    pub(crate) fn validate_type_reference(
        &self,
        reference: &TypeReference,
    ) -> Result<(), Diagnostic> {
        validate_type_reference(reference)?;
        if !self.allow_variable_vectors
            && reference.alternatives.iter().any(|atom| {
                atom.name != "POINTER"
                    && atom
                        .parts
                        .windows(2)
                        .any(|parts| parts == ["LeftBracket", "RightBracket"])
            })
        {
            return Err(error(
                "INVALID_VECTOR_TYPE",
                "variable-length vectors are reserved for the BNData library",
                reference.span,
            ));
        }
        for atom in &reference.alternatives {
            let ty = type_from_atom(atom);
            let name = match ty {
                Type::Named(name) => name,
                Type::Pointer { element, .. } => match *element {
                    Type::Named(name) => name,
                    _ => continue,
                },
                _ => continue,
            };
            if name == "VOID" {
                continue;
            }
            if matches!(name.as_str(), "DATE" | "TIME" | "TIMEZONE" | "Error")
                || self.declaration_kinds.contains_key(&name)
            {
                continue;
            }
            if let Some((alias, exported_name)) = name.split_once('.')
                && exported_name == "File"
                && self
                    .globals
                    .get(alias)
                    .is_some_and(|symbol| symbol.ty == Type::HostFileSystem)
            {
                continue;
            }
            if let Some((alias, _)) = name.split_once('.')
                && self
                    .globals
                    .get(alias)
                    .is_some_and(|symbol| symbol.ty == Type::HostNet)
            {
                continue;
            }
            if let Some((alias, exported_name)) = name.split_once('.')
                && let Some(Type::Module(module)) = self.globals.get(alias).map(|symbol| &symbol.ty)
                && self
                    .module_exports
                    .get(module)
                    .is_some_and(|exports| exports.contains_key(exported_name))
            {
                continue;
            }
            return Err(error(
                "UNKNOWN_TYPE",
                format!("type '{name}' is not declared or imported"),
                atom.span,
            ));
        }
        Ok(())
    }

    pub(crate) fn resolve_reference(&self, reference: &TypeReference) -> Type {
        self.resolve_type(type_from_reference(reference))
    }
}
