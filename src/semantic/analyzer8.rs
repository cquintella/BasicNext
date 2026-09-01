#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    pub(crate) fn resolve_type(&self, ty: Type) -> Type {
        match ty {
            Type::Named(name) => {
                if let Some((alias, exported_name)) = name.split_once('.')
                    && self
                        .globals
                        .get(alias)
                        .is_some_and(|symbol| symbol.ty == Type::HostNet)
                {
                    return Type::Named(format!("HOST.Net.{exported_name}"));
                }
                if let Some((alias, exported_name)) = name.split_once('.')
                    && exported_name == "File"
                    && self
                        .globals
                        .get(alias)
                        .is_some_and(|symbol| symbol.ty == Type::HostFileSystem)
                {
                    return Type::Named(name);
                }
                if let Some((alias, exported_name)) = name.split_once('.')
                    && let Some(Type::Module(module)) =
                        self.globals.get(alias).map(|symbol| &symbol.ty)
                    && matches!(
                        self.module_exports
                            .get(module)
                            .and_then(|exports| exports.get(exported_name)),
                        Some(Type::ImportedTypeName { .. })
                    )
                {
                    return Type::ImportedNamed {
                        module: *module,
                        name: exported_name.into(),
                    };
                }
                Type::Named(name)
            }
            Type::Alternative(types) => {
                Type::Alternative(types.into_iter().map(|ty| self.resolve_type(ty)).collect())
            }
            Type::Function {
                parameters,
                return_type,
            } => Type::Function {
                parameters: parameters
                    .into_iter()
                    .map(|ty| self.resolve_type(ty))
                    .collect(),
                return_type: Box::new(self.resolve_type(*return_type)),
            },
            Type::Vector {
                element,
                dimensions,
            } => Type::Vector {
                element: Box::new(self.resolve_type(*element)),
                dimensions,
            },
            Type::Pointer { element, length } => Type::Pointer {
                element: Box::new(self.resolve_type(*element)),
                length,
            },
            ty => ty,
        }
    }

    pub(crate) fn type_requires_initializer(&self, ty: &Type) -> bool {
        match ty {
            Type::Alternative(_) | Type::Pointer { .. } | Type::Function { .. } => true,
            Type::Named(name) => matches!(
                self.declaration_kinds.get(name),
                Some(DeclarationKind::Class | DeclarationKind::Interface)
            ),
            _ => false,
        }
    }

    pub(crate) fn sizeof_type(&self, ty: &Type, span: Span) -> Result<Type, Diagnostic> {
        if matches!(ty, Type::String)
            || matches!(ty, Type::Vector { element, .. } if self.sizeof_allowed(element))
        {
            if let Some(size) = self.byte_size(ty) {
                require_integer_fit(Some(size), span)?;
            }
            return Ok(Type::Integer(IntegerType::Int32));
        }
        match self.byte_size(ty) {
            Some(size) => {
                require_integer_fit(Some(size), span)?;
                Ok(Type::Integer(IntegerType::Int32))
            }
            None => Err(error(
                "TYPE_MISMATCH",
                "SIZEOF requires a value with a defined byte size",
                span,
            )),
        }
    }

    pub(crate) fn sizeof_allowed(&self, ty: &Type) -> bool {
        matches!(ty, Type::String)
            || static_size_of(ty).is_some()
            || self.byte_size(ty).is_some()
            || matches!(ty, Type::Vector { element, .. } if self.sizeof_allowed(element))
    }

    pub(crate) fn byte_size(&self, ty: &Type) -> Option<u64> {
        if let Some(size) = static_size_of(ty) {
            return Some(size);
        }
        match ty {
            Type::Named(name) => self.layouts.get(name).copied(),
            Type::Vector {
                element,
                dimensions,
            } => self
                .byte_size(element)
                .and_then(|element| dimension_product(dimensions)?.checked_mul(element)),
            _ => None,
        }
    }

    pub(crate) fn compute_layouts(&mut self) {
        let names: Vec<String> = self.declaration_kinds.keys().cloned().collect();
        for name in names {
            self.ensure_layout(&name);
        }
    }

    pub(crate) fn ensure_layout(&mut self, name: &str) -> Option<u64> {
        if let Some(size) = self.layouts.get(name) {
            return Some(*size);
        }
        let kind = *self.declaration_kinds.get(name)?;
        if !matches!(kind, DeclarationKind::Struct | DeclarationKind::Class) {
            return None;
        }
        let members = self.members.get(name)?.clone();
        let mut total = 0u64;
        for member in members.values() {
            if member.is_static || matches!(member.ty, Type::Function { .. }) {
                continue;
            }
            let size = self.field_static_size(&member.ty)?;
            total = total.checked_add(size)?;
        }
        self.layouts.insert(name.to_string(), total);
        Some(total)
    }

    pub(crate) fn field_static_size(&mut self, ty: &Type) -> Option<u64> {
        if let Some(size) = static_size_of(ty) {
            return Some(size);
        }
        match ty {
            Type::Named(name)
                if self.declaration_kinds.get(name) == Some(&DeclarationKind::Struct) =>
            {
                self.ensure_layout(name)
            }
            Type::Vector {
                element,
                dimensions,
            } => {
                let element = self.field_static_size(element)?;
                dimension_product(dimensions)?.checked_mul(element)
            }
            _ => None,
        }
    }
}

pub(crate) fn declaration_type(kind: DeclarationKind, name: &str) -> Type {
    match kind {
        DeclarationKind::Function => Type::Unknown,
        _ => Type::TypeName(name.into()),
    }
}
pub(crate) fn function_type(signature: &FunctionSignature) -> Type {
    Type::Function {
        parameters: signature
            .parameters
            .iter()
            .map(|parameter| type_from_reference(&parameter.type_ref))
            .collect(),
        return_type: Box::new(type_from_reference(&signature.return_type)),
    }
}
pub(crate) fn type_from_reference(reference: &TypeReference) -> Type {
    let alternatives = reference
        .alternatives
        .iter()
        .map(type_from_atom)
        .collect::<Vec<_>>();
    match alternatives.as_slice() {
        [] => Type::Unknown,
        [ty] => ty.clone(),
        _ => Type::Alternative(alternatives),
    }
}
pub(crate) fn type_from_atom(atom: &crate::ast::TypeAtom) -> Type {
    if atom.name == "FUNCTION" {
        return function_type_from_parts(&atom.parts);
    }
    if atom.name == "POINTER" {
        let element = pointer_element_type(&atom.parts);
        let length = pointer_length(&atom.parts);
        return Type::Pointer {
            element: Box::new(element),
            length,
        };
    }
    let name = qualified_type_name(atom);
    let base = match name.as_str() {
        "BOOLEAN" => Type::Boolean,
        "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16" | "UINT32" | "UINT64"
        | "INTEGER" | "TIMESTAMP" => {
            Type::Integer(integer_type(atom.name.as_str()).expect("numeric type"))
        }
        "FLOAT32" | "FLOAT64" | "FLOAT" => {
            Type::Float(float_type(atom.name.as_str()).expect("float type"))
        }
        "STRING" => Type::String,
        "NULL" => Type::Null,
        "NA" => Type::NotAvailable,
        "EOF" => Type::EndOfFile,
        "POINTER" => Type::Unknown,
        "SYSTEM" => Type::System,
        name => Type::Named(name.into()),
    };
    let dimensions = atom
        .parts
        .windows(3)
        .filter(|parts| parts[0] == "LeftBracket" && parts[2] == "RightBracket")
        .filter_map(|parts| parse_integer(&parts[1]).and_then(|value| u64::try_from(value).ok()))
        .collect::<Vec<_>>();
    let has_vector_brackets = atom.parts.iter().any(|part| part == "LeftBracket");
    if dimensions.is_empty() && !has_vector_brackets {
        base
    } else {
        Type::Vector {
            element: Box::new(base),
            dimensions: if dimensions.is_empty() {
                vec![u64::MAX]
            } else {
                dimensions
            },
        }
    }
}

pub(crate) fn function_type_from_parts(parts: &[String]) -> Type {
    let Some(close) = matching_right_paren(parts) else {
        return Type::Unknown;
    };
    let parameter_tokens = &parts[1..close];
    let mut parameters = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for index in 0..=parameter_tokens.len() {
        let separator =
            index == parameter_tokens.len() || (parameter_tokens[index] == "Comma" && depth == 0);
        if separator {
            if start < index {
                parameters.push(type_from_tokens(&parameter_tokens[start..index]));
            }
            start = index + 1;
            continue;
        }
        match parameter_tokens[index].as_str() {
            "LeftParen" | "LeftBracket" => depth += 1,
            "RightParen" | "RightBracket" => depth -= 1,
            _ => {}
        }
    }
    let return_type = parts
        .get(close + 1)
        .filter(|part| part.as_str() == "AS")
        .map_or(Type::Unknown, |_| type_from_tokens(&parts[close + 2..]));
    Type::Function {
        parameters,
        return_type: Box::new(return_type),
    }
}

pub(crate) fn matching_right_paren(parts: &[String]) -> Option<usize> {
    if parts.first()?.as_str() != "LeftParen" {
        return None;
    }
    let mut depth = 0;
    for (index, part) in parts.iter().enumerate() {
        match part.as_str() {
            "LeftParen" => depth += 1,
            "RightParen" => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn type_from_tokens(tokens: &[String]) -> Type {
    if tokens.len() >= 3 && tokens[1] == "AS" {
        return type_from_tokens(&tokens[2..]);
    }
    let mut alternatives = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for index in 0..=tokens.len() {
        let separator = index == tokens.len() || (tokens[index] == "OR" && depth == 0);
        if separator {
            if start < index {
                alternatives.push(type_from_atom(&crate::ast::TypeAtom {
                    name: tokens[start].clone(),
                    parts: tokens[start + 1..index].to_vec(),
                    dimensions: Vec::new(),
                    span: default_span(),
                }));
            }
            start = index + 1;
            continue;
        }
        match tokens[index].as_str() {
            "LeftParen" | "LeftBracket" => depth += 1,
            "RightParen" | "RightBracket" => depth -= 1,
            _ => {}
        }
    }
    match alternatives.as_slice() {
        [ty] => ty.clone(),
        _ => Type::Alternative(alternatives),
    }
}
