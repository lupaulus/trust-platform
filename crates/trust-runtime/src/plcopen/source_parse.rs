fn load_sources(project_root: &Path, sources_root: &Path) -> anyhow::Result<Vec<LoadedSource>> {
    let mut paths = BTreeSet::new();
    for pattern in ["**/*.st", "**/*.ST", "**/*.pou", "**/*.POU"] {
        let glob_pattern = format!("{}/{}", sources_root.display(), pattern);
        for entry in glob::glob(&glob_pattern)
            .with_context(|| format!("invalid source glob '{}'", glob_pattern))?
        {
            paths.insert(entry?);
        }
    }

    let mut sources = Vec::with_capacity(paths.len());
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read source '{}'", path.display()))?;
        let relative = path
            .strip_prefix(project_root)
            .map_or_else(|_| path.clone(), Path::to_path_buf);
        sources.push(LoadedSource {
            path: relative,
            text,
        });
    }
    Ok(sources)
}

fn extract_pou_declarations(source: &LoadedSource) -> (Vec<PouDecl>, Vec<String>) {
    let mut declarations = Vec::new();
    let mut warnings = Vec::new();

    let parsed = parser::parse(&source.text);
    let syntax = parsed.syntax();

    for node in syntax.children() {
        let Some(pou_type) = node_to_pou_type(&node) else {
            if is_unsupported_top_level(&node) {
                let line = line_for_node(&source.text, &node);
                warnings.push(format!(
                    "{}:{} unsupported top-level node '{:?}' skipped for PLCopen ST-complete subset",
                    source.path.display(),
                    line,
                    node.kind()
                ));
            }
            continue;
        };

        let Some(name) = declaration_name(&node) else {
            continue;
        };

        if is_test_pou(&node) {
            let line = line_for_node(&source.text, &node);
            warnings.push(format!(
                "{}:{} test POU '{}' exported as standard '{}'",
                source.path.display(),
                line,
                name,
                pou_type.as_xml()
            ));
        }

        let line = line_for_node(&source.text, &node);
        declarations.push(PouDecl {
            name,
            pou_type,
            body: normalize_body_text(node.text().to_string()),
            source: source.path.display().to_string(),
            line,
        });
    }

    (declarations, warnings)
}

fn extract_global_var_declarations(source: &LoadedSource) -> (Vec<GlobalVarDecl>, Vec<String>) {
    let mut declarations = Vec::new();
    let mut warnings = Vec::new();
    let lines = source.text.lines().collect::<Vec<_>>();
    let struct_fields_by_type = parse_struct_type_declarations(&source.text);
    let mut index = 0usize;
    let mut block_index = 0usize;
    let base_name = source
        .path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "GlobalVars".to_string());

    while index < lines.len() {
        let upper = lines[index].trim().to_ascii_uppercase();
        if !upper.starts_with("VAR_GLOBAL") {
            index += 1;
            continue;
        }

        let mut block_start = index;
        while block_start > 0 {
            let prev = lines[block_start - 1].trim();
            if prev.is_empty() {
                break;
            }
            if prev.starts_with('{') && prev.ends_with('}') {
                block_start -= 1;
            } else {
                break;
            }
        }

        let Some(end_index) = (index..lines.len())
            .find(|line_index| lines[*line_index].trim().eq_ignore_ascii_case("END_VAR"))
        else {
            warnings.push(format!(
                "{}:{} unterminated VAR_GLOBAL block skipped during PLCopen export",
                source.path.display(),
                index + 1
            ));
            break;
        };

        let body = normalize_body_text(lines[block_start..=end_index].join("\n"));
        let mut variables = parse_global_var_entries_from_st_block(&body);
        let name = if block_index == 0 {
            base_name.clone()
        } else {
            format!("{base_name}_{}", block_index + 1)
        };
        if variables.len() == 1 {
            let wrapper = &variables[0];
            if wrapper.name.eq_ignore_ascii_case(&name) {
                let type_key = wrapper.type_expr.trim().to_ascii_lowercase();
                if let Some(struct_fields) = struct_fields_by_type.get(&type_key) {
                    variables = struct_fields.clone();
                }
            }
        }
        declarations.push(GlobalVarDecl {
            name,
            body,
            source: source.path.display().to_string(),
            source_path: source.path.clone(),
            line: block_start + 1,
            variables,
        });
        block_index += 1;
        index = end_index + 1;
    }

    (declarations, warnings)
}

fn parse_struct_type_declarations(source: &str) -> BTreeMap<String, Vec<GlobalVarVariableDecl>> {
    let mut map = BTreeMap::new();
    let lines = source.lines().collect::<Vec<_>>();
    let mut index = 0usize;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        let Some((lhs, rhs)) = trimmed.split_once(':') else {
            index += 1;
            continue;
        };
        if !rhs.trim().eq_ignore_ascii_case("STRUCT") {
            index += 1;
            continue;
        }

        let type_name = lhs.trim();
        if type_name.is_empty() {
            index += 1;
            continue;
        }

        index += 1;
        let mut field_lines = Vec::new();
        while index < lines.len() && !lines[index].trim().eq_ignore_ascii_case("END_STRUCT") {
            field_lines.push(lines[index]);
            index += 1;
        }
        if index >= lines.len() {
            break;
        }

        let mut pseudo_block = String::from("VAR_GLOBAL\n");
        for field in field_lines {
            pseudo_block.push_str(field);
            pseudo_block.push('\n');
        }
        pseudo_block.push_str("END_VAR\n");

        let fields = parse_global_var_entries_from_st_block(&pseudo_block);
        if !fields.is_empty() {
            map.insert(type_name.to_ascii_lowercase(), fields);
        }

        index += 1;
    }

    map
}

fn parse_global_var_entries_from_st_block(block: &str) -> Vec<GlobalVarVariableDecl> {
    let mut entries = Vec::new();
    let mut in_block = false;

    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            continue;
        }
        if trimmed.to_ascii_uppercase().starts_with("VAR_GLOBAL") {
            in_block = true;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END_VAR") {
            break;
        }
        if !in_block || trimmed.starts_with("//") || trimmed.starts_with("(*") {
            continue;
        }

        let declaration = trimmed.trim_end_matches(';').trim();
        let Some((lhs, rhs)) = declaration.split_once(':') else {
            continue;
        };
        let raw_name = lhs.trim();
        if raw_name.is_empty() {
            continue;
        }
        let var_name = raw_name
            .split_whitespace()
            .next()
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        if var_name.is_empty() {
            continue;
        }
        let (type_expr, initial_value) = match rhs.split_once(":=") {
            Some((type_part, initial_part)) => (
                type_part.trim().to_string(),
                Some(initial_part.trim().to_string()),
            ),
            None => (rhs.trim().to_string(), None),
        };
        if type_expr.is_empty() {
            continue;
        }
        entries.push(GlobalVarVariableDecl {
            name: var_name,
            type_expr,
            initial_value,
        });
    }

    entries
}

fn node_to_pou_type(node: &SyntaxNode) -> Option<PlcopenPouType> {
    match node.kind() {
        SyntaxKind::Program => Some(PlcopenPouType::Program),
        SyntaxKind::Function => Some(PlcopenPouType::Function),
        SyntaxKind::FunctionBlock => Some(PlcopenPouType::FunctionBlock),
        _ => None,
    }
}

fn is_unsupported_top_level(node: &SyntaxNode) -> bool {
    matches!(
        node.kind(),
        SyntaxKind::Class
            | SyntaxKind::Interface
            | SyntaxKind::Namespace
            | SyntaxKind::Configuration
            | SyntaxKind::TypeDecl
            | SyntaxKind::Action
    )
}

fn is_test_pou(node: &SyntaxNode) -> bool {
    first_non_trivia_token(node).is_some_and(|kind| {
        matches!(
            kind,
            SyntaxKind::KwTestProgram | SyntaxKind::KwTestFunctionBlock
        )
    })
}

fn first_non_trivia_token(node: &SyntaxNode) -> Option<SyntaxKind> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())
        .map(|token| token.kind())
}

fn declaration_name(node: &SyntaxNode) -> Option<String> {
    node.children()
        .find(|child| child.kind() == SyntaxKind::Name)
        .map(|name| name.text().to_string().trim().to_string())
        .filter(|text| !text.is_empty())
}

fn line_for_node(source: &str, node: &SyntaxNode) -> usize {
    let offset = node
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())
        .map(|token| usize::from(token.text_range().start()))
        .unwrap_or(0);
    source[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{timestamp}Z")
}

fn escape_xml_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
}

fn escape_cdata(value: &str) -> String {
    value.replace("]]>", "]]]]><![CDATA[>")
}

fn normalize_body_text(text: impl Into<String>) -> String {
    let mut normalized = text.into().replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

