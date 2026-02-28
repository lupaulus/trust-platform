use super::*;

#[test]
pub(super) fn lsp_document_symbols_include_members() {
    let source = r#"
INTERFACE ICounter
    METHOD Next : DINT
    END_METHOD
    PROPERTY Value : DINT
        GET
        END_GET
    END_PROPERTY
END_INTERFACE

FUNCTION_BLOCK CounterFb IMPLEMENTS ICounter
VAR
    x : DINT;
END_VAR

METHOD PUBLIC Next : DINT
    x := x + 1;
    Next := x;
END_METHOD

PUBLIC PROPERTY Value : DINT
    GET
        Value := x;
    END_GET
END_PROPERTY
END_FUNCTION_BLOCK
"#;
    let state = ServerState::new();
    let uri = tower_lsp::lsp_types::Url::parse("file:///members.st").unwrap();
    state.open_document(uri.clone(), 1, source.to_string());

    let params = tower_lsp::lsp_types::DocumentSymbolParams {
        text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let response = document_symbol(&state, params).expect("document symbols");
    let symbols = match response {
        tower_lsp::lsp_types::DocumentSymbolResponse::Flat(symbols) => symbols,
        tower_lsp::lsp_types::DocumentSymbolResponse::Nested(_) => {
            panic!("expected flat document symbols")
        }
    };

    let has_next = symbols.iter().any(|symbol| symbol.name.contains("Next"));
    let has_value = symbols.iter().any(|symbol| symbol.name.contains("Value"));
    assert!(has_next, "expected Next in document symbols");
    assert!(has_value, "expected Value in document symbols");

    let has_next_in_fb = symbols.iter().any(|symbol| {
        symbol.name.contains("Next") && symbol.container_name.as_deref() == Some("CounterFb")
    });
    assert!(has_next_in_fb, "expected Next under CounterFb");
}

#[test]
pub(super) fn lsp_oop_access_diagnostics_include_explainer_and_hint() {
    let source = r#"
CLASS Foo
VAR PRIVATE
    secret : INT;
END_VAR
END_CLASS

PROGRAM Test
VAR
    f : Foo;
    x : INT;
END_VAR
    x := f.secret;
END_PROGRAM
"#;
    let state = ServerState::new();
    let uri = tower_lsp::lsp_types::Url::parse("file:///access.st").unwrap();
    state.open_document(uri.clone(), 1, source.to_string());

    let params = tower_lsp::lsp_types::DocumentDiagnosticParams {
        text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
        identifier: None,
        previous_result_id: None,
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let report = document_diagnostic(&state, params);
    let report = match report {
        tower_lsp::lsp_types::DocumentDiagnosticReportResult::Report(report) => report,
        _ => panic!("expected diagnostic report"),
    };
    let full = match report {
        tower_lsp::lsp_types::DocumentDiagnosticReport::Full(full) => full,
        _ => panic!("expected full diagnostic report"),
    };
    let diagnostics = full.full_document_diagnostic_report.items;
    let access_diag = diagnostics
        .iter()
        .find(|diag| diag.message.contains("cannot access PRIVATE member"))
        .expect("expected access violation diagnostic");
    let explain = access_diag
        .data
        .as_ref()
        .and_then(|value| value.as_object())
        .and_then(|map| map.get("explain"))
        .and_then(|value| value.get("iec"))
        .and_then(|value| value.as_str());
    assert!(
        explain.is_some_and(|iec| iec.contains("6.6.5")),
        "expected IEC 6.6.5 explainer"
    );
    let related = access_diag.related_information.as_ref();
    assert!(
        related.is_some_and(|items| items.iter().any(|item| item.message.contains("Hint:"))),
        "expected access hint related information"
    );
}

#[test]
pub(super) fn lsp_workspace_diagnostics_supports_unchanged_reports() {
    let source = r#"
PROGRAM Test
    VAR
        A__B : INT;
    END_VAR
END_PROGRAM
"#;
    let state = ServerState::new();
    let uri = tower_lsp::lsp_types::Url::parse("file:///workspace-diag.st").unwrap();
    state.open_document(uri.clone(), 1, source.to_string());

    let params = tower_lsp::lsp_types::WorkspaceDiagnosticParams {
        identifier: None,
        previous_result_ids: Vec::new(),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let report = workspace_diagnostic(&state, params);
    let report = match report {
        tower_lsp::lsp_types::WorkspaceDiagnosticReportResult::Report(report) => report,
        _ => panic!("expected workspace diagnostic report"),
    };
    let first_item = report
        .items
        .iter()
        .find(|item| match item {
            tower_lsp::lsp_types::WorkspaceDocumentDiagnosticReport::Full(full) => full.uri == uri,
            tower_lsp::lsp_types::WorkspaceDocumentDiagnosticReport::Unchanged(unchanged) => {
                unchanged.uri == uri
            }
        })
        .expect("expected workspace diagnostic item");
    let result_id = match first_item {
        tower_lsp::lsp_types::WorkspaceDocumentDiagnosticReport::Full(full) => full
            .full_document_diagnostic_report
            .result_id
            .clone()
            .expect("result id"),
        _ => panic!("expected full diagnostic report"),
    };

    let params = tower_lsp::lsp_types::WorkspaceDiagnosticParams {
        identifier: None,
        previous_result_ids: vec![tower_lsp::lsp_types::PreviousResultId {
            uri: uri.clone(),
            value: result_id,
        }],
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let report = workspace_diagnostic(&state, params);
    let report = match report {
        tower_lsp::lsp_types::WorkspaceDiagnosticReportResult::Report(report) => report,
        _ => panic!("expected workspace diagnostic report"),
    };
    let unchanged = report
        .items
        .iter()
        .find(|item| match item {
            tower_lsp::lsp_types::WorkspaceDocumentDiagnosticReport::Full(full) => full.uri == uri,
            tower_lsp::lsp_types::WorkspaceDocumentDiagnosticReport::Unchanged(unchanged) => {
                unchanged.uri == uri
            }
        })
        .expect("expected workspace diagnostic item");
    assert!(
        matches!(
            unchanged,
            tower_lsp::lsp_types::WorkspaceDocumentDiagnosticReport::Unchanged(_)
        ),
        "expected unchanged workspace diagnostic report"
    );
}

#[test]
pub(super) fn lsp_will_rename_files_updates_pou_name() {
    let source_decl = r#"
FUNCTION_BLOCK OldName
END_FUNCTION_BLOCK
"#;
    let source_ref = r#"
PROGRAM Main
    VAR
        fb : OldName;
    END_VAR
END_PROGRAM
"#;
    let state = ServerState::new();
    let decl_uri = tower_lsp::lsp_types::Url::parse("file:///OldName.st").unwrap();
    let ref_uri = tower_lsp::lsp_types::Url::parse("file:///Ref.st").unwrap();
    state.open_document(decl_uri.clone(), 1, source_decl.to_string());
    state.open_document(ref_uri.clone(), 1, source_ref.to_string());

    let params = tower_lsp::lsp_types::RenameFilesParams {
        files: vec![tower_lsp::lsp_types::FileRename {
            old_uri: decl_uri.to_string(),
            new_uri: "file:///NewName.st".to_string(),
        }],
    };
    let edit = will_rename_files(&state, params).expect("rename edits");
    let changes = edit.changes.expect("workspace edits");
    let decl_edits = changes.get(&decl_uri).expect("declaration edits");
    let ref_edits = changes.get(&ref_uri).expect("reference edits");
    assert!(decl_edits.iter().any(|edit| edit.new_text == "NewName"));
    assert!(ref_edits.iter().any(|edit| edit.new_text == "NewName"));
}
