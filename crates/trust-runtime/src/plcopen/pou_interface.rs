fn detect_program_pous_used_as_types(root: roxmltree::Node<'_, '_>) -> HashSet<String> {
    let mut program_names = HashSet::new();
    for pou in root
        .descendants()
        .filter(|node| is_element_named_ci(*node, "pou"))
    {
        let Some(pou_name) = extract_pou_name(pou) else {
            continue;
        };
        let Some(raw_type) = attribute_ci(pou, "pouType").or_else(|| attribute_ci(pou, "type"))
        else {
            continue;
        };
        if PlcopenPouType::from_xml(&raw_type).is_some_and(|kind| kind == PlcopenPouType::Program) {
            program_names.insert(pou_name.to_ascii_lowercase());
        }
    }

    if program_names.is_empty() {
        return HashSet::new();
    }

    let mut promoted = HashSet::new();
    for derived in root
        .descendants()
        .filter(|node| is_element_named_ci(*node, "derived"))
    {
        let Some(name) = attribute_ci(derived, "name")
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if program_names.contains(&name) {
            promoted.insert(name);
        }
    }
    promoted
}

fn synthesize_import_pou_source(
    pou_node: roxmltree::Node<'_, '_>,
    pou_type: PlcopenPouType,
    pou_name: &str,
    st_body: Option<&str>,
    warnings: &mut Vec<String>,
    unsupported_diagnostics: &mut Vec<PlcopenUnsupportedDiagnostic>,
) -> Option<String> {
    let normalized_body = st_body.map(normalize_body_text).unwrap_or_default();
    let has_body = !normalized_body.trim().is_empty();
    if has_body && source_has_top_level_pou_declaration(&normalized_body, pou_type) {
        return Some(normalized_body);
    }

    let metadata = extract_pou_interface_metadata(pou_node, pou_type);
    if !has_body && !metadata.has_details() {
        return None;
    }

    let header = render_import_pou_header(
        pou_type,
        pou_name,
        &metadata,
        warnings,
        unsupported_diagnostics,
    )?;
    let mut synthesized = String::new();
    synthesized.push_str(&header);
    synthesized.push('\n');

    for section in &metadata.sections {
        if section.declarations.is_empty() {
            continue;
        }
        synthesized.push_str(section.keyword);
        synthesized.push('\n');
        for declaration in &section.declarations {
            synthesized.push_str(declaration);
            synthesized.push('\n');
        }
        synthesized.push_str("END_VAR\n");
    }

    if has_body {
        synthesized.push_str(normalized_body.trim_end());
        synthesized.push('\n');
        warnings.push(format!(
            "pou '{}' body omitted a top-level declaration wrapper; synthesized '{}'",
            pou_name,
            pou_type.declaration_keyword()
        ));
        unsupported_diagnostics.push(unsupported_diagnostic(
            "PLCO207",
            "info",
            "pou/body/ST",
            "POU ST body lacked declaration wrapper; importer synthesized one",
            Some(pou_name.to_string()),
            "Review synthesized declaration sections for vendor-specific details",
        ));
    } else {
        warnings.push(format!(
            "pou '{}' had missing/empty ST body; synthesized declaration shell from interface metadata",
            pou_name
        ));
        unsupported_diagnostics.push(unsupported_diagnostic(
            "PLCO208",
            "info",
            "pou/interface",
            "POU body missing or empty; importer synthesized a declaration shell",
            Some(pou_name.to_string()),
            "Manual body implementation may still be required after import",
        ));
    }

    if pou_type == PlcopenPouType::Function
        && !function_result_assignment_present(&synthesized, pou_name)
    {
        synthesized.push_str(&format!("{pou_name} := {pou_name};\n"));
        warnings.push(format!(
            "function '{}' lacked an explicit result assignment; inserted default self-assignment",
            pou_name
        ));
        unsupported_diagnostics.push(unsupported_diagnostic(
            "PLCO212",
            "info",
            "pou/body/ST",
            "Function body lacked explicit result assignment; importer inserted a default self-assignment",
            Some(pou_name.to_string()),
            "Review the inserted assignment and replace it with domain-specific return logic",
        ));
    }

    synthesized.push_str(pou_type.end_keyword());
    synthesized.push('\n');
    Some(synthesized)
}

fn source_has_top_level_pou_declaration(source: &str, pou_type: PlcopenPouType) -> bool {
    let parsed = parser::parse(source);
    parsed
        .syntax()
        .children()
        .any(|node| node_to_pou_type(&node).is_some_and(|candidate| candidate == pou_type))
}

fn extract_pou_interface_metadata(
    pou_node: roxmltree::Node<'_, '_>,
    _pou_type: PlcopenPouType,
) -> PouInterfaceMetadata {
    let mut metadata = PouInterfaceMetadata::default();

    if let Some(interface) = first_child_element_ci(pou_node, "interface") {
        metadata.function_return_type = first_child_element_ci(interface, "returnType")
            .and_then(parse_type_expression_container)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let section_defs = [
            ("inputVars", "VAR_INPUT"),
            ("outputVars", "VAR_OUTPUT"),
            ("inOutVars", "VAR_IN_OUT"),
            ("externalVars", "VAR_EXTERNAL"),
            ("localVars", "VAR"),
            ("tempVars", "VAR_TEMP"),
        ];
        for (xml_name, st_keyword) in section_defs {
            let mut declarations = Vec::new();
            for section in interface
                .children()
                .filter(|child| is_element_named_ci(*child, xml_name))
            {
                declarations.extend(parse_interface_var_declarations(section));
            }
            if !declarations.is_empty() {
                metadata.sections.push(InterfaceVarSection {
                    keyword: st_keyword,
                    declarations,
                });
            }
        }
    }

    metadata.header_hint = extract_interface_plaintext_header(pou_node);
    metadata
}

fn parse_interface_var_declarations(section: roxmltree::Node<'_, '_>) -> Vec<String> {
    let mut declarations = Vec::new();
    for variable in section
        .children()
        .filter(|child| is_element_named_ci(*child, "variable"))
    {
        let Some(name) = attribute_ci(variable, "name")
            .or_else(|| {
                variable
                    .children()
                    .find(|child| is_element_named_ci(*child, "name"))
                    .and_then(extract_text_content)
            })
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(type_expr) = first_child_element_ci(variable, "type")
            .and_then(parse_type_expression_container)
            .or_else(|| {
                first_child_element_ci(variable, "baseType")
                    .and_then(parse_type_expression_container)
            })
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let initializer = first_child_element_ci(variable, "initialValue")
            .and_then(parse_initial_value)
            .map_or_else(String::new, |value| format!(" := {value}"));
        declarations.push(format!("    {name} : {type_expr}{initializer};"));
    }
    declarations
}

fn render_import_pou_header(
    pou_type: PlcopenPouType,
    pou_name: &str,
    metadata: &PouInterfaceMetadata,
    warnings: &mut Vec<String>,
    unsupported_diagnostics: &mut Vec<PlcopenUnsupportedDiagnostic>,
) -> Option<String> {
    match pou_type {
        PlcopenPouType::Program => Some(format!("PROGRAM {pou_name}")),
        PlcopenPouType::FunctionBlock => Some(format!("FUNCTION_BLOCK {pou_name}")),
        PlcopenPouType::Function => {
            if let Some(return_type) = metadata.function_return_type.as_deref() {
                return Some(format!("FUNCTION {pou_name} : {}", return_type.trim()));
            }
            if let Some(return_type) = metadata
                .header_hint
                .as_deref()
                .and_then(parse_function_return_type_from_header)
            {
                return Some(format!("FUNCTION {pou_name} : {}", return_type.trim()));
            }
            warnings.push(format!(
                "function '{}' did not provide an importable return type; defaulting to INT",
                pou_name
            ));
            unsupported_diagnostics.push(unsupported_diagnostic(
                "PLCO211",
                "warning",
                "pou/interface/returnType",
                "Function return type missing in PLCopen interface metadata; defaulted to INT",
                Some(pou_name.to_string()),
                "Review the imported FUNCTION signature and adjust the return type manually",
            ));
            Some(format!("FUNCTION {pou_name} : INT"))
        }
    }
}

fn extract_interface_plaintext_header(pou_node: roxmltree::Node<'_, '_>) -> Option<String> {
    const POU_PREFIXES: [&str; 3] = ["PROGRAM ", "FUNCTION_BLOCK ", "FUNCTION "];
    for data in pou_node
        .descendants()
        .filter(|node| is_element_named_ci(*node, "data"))
    {
        let Some(name) = attribute_ci(data, "name") else {
            continue;
        };
        if !name.to_ascii_lowercase().contains("interfaceasplaintext") {
            continue;
        }
        let Some(text) = extract_text_content(data) else {
            continue;
        };
        for line in text.lines() {
            let trimmed = line.trim();
            let upper = trimmed.to_ascii_uppercase();
            if POU_PREFIXES.iter().any(|prefix| upper.starts_with(prefix)) {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn parse_function_return_type_from_header(header: &str) -> Option<String> {
    let (_, suffix) = header.split_once(':')?;
    let return_type = suffix.trim().trim_end_matches(';').trim().to_string();
    if return_type.is_empty() {
        None
    } else {
        Some(return_type)
    }
}

fn function_result_assignment_present(source: &str, function_name: &str) -> bool {
    let target = function_name.trim();
    if target.is_empty() {
        return false;
    }

    let mut in_block_comment = false;
    for line in source.lines() {
        let mut text = line.to_string();

        if in_block_comment {
            if let Some(end) = text.find("*)") {
                text = text[end + 2..].to_string();
                in_block_comment = false;
            } else {
                continue;
            }
        }

        while let Some(start) = text.find("(*") {
            if let Some(end_rel) = text[start + 2..].find("*)") {
                let end = start + 2 + end_rel;
                text.replace_range(start..end + 2, "");
            } else {
                text.truncate(start);
                in_block_comment = true;
                break;
            }
        }

        if let Some(comment) = text.find("//") {
            text.truncate(comment);
        }

        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((lhs, _rhs)) = trimmed.split_once(":=") else {
            continue;
        };
        if lhs.trim().eq_ignore_ascii_case(target) {
            return true;
        }
    }

    false
}

