use launcher_core::{build_argv, parse_desktop_entry, ExecContext, ExecError};

fn ctx() -> ExecContext {
    ExecContext::default()
}

fn ctx_with(files: &[&str]) -> ExecContext {
    ExecContext {
        files: files.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn exec_empty_command_rejected() {
    assert_eq!(build_argv("", &ctx()), Err(ExecError::EmptyCommand));
    assert_eq!(build_argv("   ", &ctx()), Err(ExecError::EmptyCommand));
}

#[test]
fn exec_basic_words() {
    assert_eq!(
        build_argv("demo --flag", &ctx()).unwrap(),
        vec!["demo", "--flag"]
    );
}

#[test]
fn exec_quoted_argument_with_space() {
    assert_eq!(
        build_argv("\"my app\" --flag", &ctx()).unwrap(),
        vec!["my app", "--flag"]
    );
}

#[test]
fn exec_string_escape_s_inside_quotes() {
    assert_eq!(build_argv("\"a\\sb\"", &ctx()).unwrap(), vec!["a b"]);
}

#[test]
fn exec_literal_backslash_inside_quotes_needs_four() {
    // File text `"a\\\\b"` -> after string escapes `"a\\b"` -> after quoting `a\b`.
    assert_eq!(build_argv("\"a\\\\\\\\b\"", &ctx()).unwrap(), vec!["a\\b"]);
}

#[test]
fn exec_dollar_escaped_inside_quotes() {
    // `sh -c "echo \$HOME"` -> argv ["sh", "-c", "echo $HOME"].
    assert_eq!(
        build_argv("sh -c \"echo \\$HOME\"", &ctx()).unwrap(),
        vec!["sh", "-c", "echo $HOME"]
    );
}

#[test]
fn exec_percent_percent_literal() {
    assert_eq!(
        build_argv("app 100%%", &ctx()).unwrap(),
        vec!["app", "100%"]
    );
}

#[test]
fn exec_percent_f_single_file_and_first_only() {
    let c = ctx_with(&["/a", "/b"]);
    assert_eq!(build_argv("app %f", &c).unwrap(), vec!["app", "/a"]);
    assert_eq!(build_argv("app %u", &c).unwrap(), vec!["app", "/a"]);
}

#[test]
fn exec_percent_f_empty_files_removed() {
    assert_eq!(build_argv("app %f", &ctx()).unwrap(), vec!["app"]);
    assert_eq!(build_argv("app %u", &ctx()).unwrap(), vec!["app"]);
    assert_eq!(build_argv("app %F", &ctx()).unwrap(), vec!["app"]);
    assert_eq!(build_argv("app %U", &ctx()).unwrap(), vec!["app"]);
}

#[test]
fn exec_percent_f_and_u_multiple() {
    let c = ctx_with(&["/a", "/b"]);
    assert_eq!(build_argv("app %F", &c).unwrap(), vec!["app", "/a", "/b"]);
    assert_eq!(build_argv("app %U", &c).unwrap(), vec!["app", "/a", "/b"]);
}

#[test]
fn exec_percent_u_inline_expands_first() {
    let c = ctx_with(&["/a", "/b"]);
    assert_eq!(
        build_argv("app --file=%u", &c).unwrap(),
        vec!["app", "--file=/a"]
    );
}

#[test]
fn exec_percent_i_with_icon() {
    let c = ExecContext {
        icon: Some("firefox".to_string()),
        ..Default::default()
    };
    assert_eq!(
        build_argv("app %i", &c).unwrap(),
        vec!["app", "--icon", "firefox"]
    );
}

#[test]
fn exec_percent_i_without_icon_removed() {
    assert_eq!(build_argv("app %i", &ctx()).unwrap(), vec!["app"]);
}

#[test]
fn exec_percent_c_name_with_spaces_stays_one_arg() {
    let c = ExecContext {
        name: "My App".to_string(),
        ..Default::default()
    };
    assert_eq!(build_argv("app %c", &c).unwrap(), vec!["app", "My App"]);
}

#[test]
fn exec_percent_c_inline() {
    let c = ExecContext {
        name: "My App".to_string(),
        ..Default::default()
    };
    assert_eq!(
        build_argv("app --title=%c", &c).unwrap(),
        vec!["app", "--title=My App"]
    );
}

#[test]
fn exec_expansion_once_no_rescan() {
    let c = ExecContext {
        name: "a%f".to_string(),
        ..Default::default()
    };
    // The %f inside the injected name is NOT rescanned.
    assert_eq!(build_argv("app %c", &c).unwrap(), vec!["app", "a%f"]);
}

#[test]
fn exec_percent_k_with_path() {
    let c = ExecContext {
        desktop_file: Some("/usr/share/applications/demo.desktop".to_string()),
        ..Default::default()
    };
    assert_eq!(
        build_argv("app %k", &c).unwrap(),
        vec!["app", "/usr/share/applications/demo.desktop"]
    );
}

#[test]
fn exec_percent_k_unknown_removed() {
    assert_eq!(build_argv("app %k", &ctx()).unwrap(), vec!["app"]);
}

#[test]
fn exec_deprecated_codes_removed() {
    for code in ["%d", "%D", "%n", "%N", "%v", "%m"] {
        let exec = format!("app {code} x");
        assert_eq!(
            build_argv(&exec, &ctx()).unwrap(),
            vec!["app", "x"],
            "{code}"
        );
    }
    assert_eq!(build_argv("app %d", &ctx()).unwrap(), vec!["app"]);
}

#[test]
fn exec_deprecated_only_is_empty_command() {
    assert_eq!(build_argv("%d", &ctx()), Err(ExecError::EmptyCommand));
}

#[test]
fn exec_unknown_field_code_rejected() {
    assert_eq!(
        build_argv("app %x", &ctx()),
        Err(ExecError::UnknownFieldCode('x'))
    );
    assert_eq!(
        build_argv("app %Z", &ctx()),
        Err(ExecError::UnknownFieldCode('Z'))
    );
}

#[test]
fn exec_multiple_file_codes_rejected() {
    assert_eq!(
        build_argv("app %f %u", &ctx_with(&["/a"])),
        Err(ExecError::MultipleFileCodes)
    );
    assert_eq!(
        build_argv("app %F %U", &ctx_with(&["/a"])),
        Err(ExecError::MultipleFileCodes)
    );
    assert_eq!(
        build_argv("app %f %f", &ctx_with(&["/a"])),
        Err(ExecError::MultipleFileCodes)
    );
}

#[test]
fn exec_percent_f_not_standalone_rejected() {
    assert_eq!(
        build_argv("app --file=%F", &ctx_with(&["/a"])),
        Err(ExecError::FieldCodeNotStandalone('F'))
    );
    assert_eq!(
        build_argv("app %Fx", &ctx_with(&["/a"])),
        Err(ExecError::FieldCodeNotStandalone('F'))
    );
}

#[test]
fn exec_percent_u_not_standalone_rejected() {
    assert_eq!(
        build_argv("app --url=%U", &ctx_with(&["/a"])),
        Err(ExecError::FieldCodeNotStandalone('U'))
    );
}

#[test]
fn exec_percent_i_not_standalone_rejected() {
    assert_eq!(
        build_argv("app x%i", &ctx()),
        Err(ExecError::FieldCodeNotStandalone('i'))
    );
    assert_eq!(
        build_argv("app %iX", &ctx()),
        Err(ExecError::FieldCodeNotStandalone('i'))
    );
}

#[test]
fn exec_field_code_inside_quotes_rejected() {
    assert_eq!(
        build_argv("app \"%F\"", &ctx_with(&["/a"])),
        Err(ExecError::FieldCodeInQuotedArgument)
    );
    assert_eq!(
        build_argv("app \"foo%f\"", &ctx_with(&["/a"])),
        Err(ExecError::FieldCodeInQuotedArgument)
    );
    assert_eq!(
        build_argv("app \"%%\"", &ctx()),
        Err(ExecError::FieldCodeInQuotedArgument)
    );
}

#[test]
fn exec_unterminated_quote_rejected() {
    assert_eq!(
        build_argv("app \"oops", &ctx()),
        Err(ExecError::UnterminatedQuote)
    );
}

#[test]
fn exec_executable_contains_equals_rejected() {
    assert_eq!(
        build_argv("a=b arg", &ctx()),
        Err(ExecError::ExecutableContainsEquals)
    );
    assert_eq!(
        build_argv("a=b", &ctx()),
        Err(ExecError::ExecutableContainsEquals)
    );
}

#[test]
fn exec_multibyte_arguments() {
    assert_eq!(
        build_argv("app 中文 --é", &ctx()).unwrap(),
        vec!["app", "中文", "--é"]
    );
}

#[test]
fn exec_escape_before_quoting_unquoted_separator() {
    // §7: string escapes apply BEFORE quoting. `app\sflag` -> `app flag`
    // after escaping, and the resulting space separates two arguments.
    assert_eq!(
        build_argv("app\\sflag", &ctx()).unwrap(),
        vec!["app", "flag"]
    );
}

#[test]
fn exec_escape_before_quoting_quoted_stays_one_arg() {
    // Same escape inside quotes: `"app\sflag"` -> `"app flag"` stays one arg.
    assert_eq!(
        build_argv("\"app\\sflag\"", &ctx()).unwrap(),
        vec!["app flag"]
    );
}

#[test]
fn exec_percent_i_empty_icon_removed() {
    // An empty Icon value behaves like a missing one: `%i` adds no arguments.
    let c = ExecContext {
        icon: Some(String::new()),
        ..Default::default()
    };
    assert_eq!(build_argv("app %i", &c).unwrap(), vec!["app"]);
}

#[test]
fn exec_percent_i_empty_icon_from_parse_removed() {
    // `Icon=` (present, empty) parses to an empty icon; `%i` adds nothing.
    let text = "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo %i\nIcon=\n";
    let (entry, _) = parse_desktop_entry(text, "d", None).unwrap();
    assert!(entry.icon.is_empty());
    let c = ExecContext {
        icon: Some(entry.icon.clone()),
        ..Default::default()
    };
    assert_eq!(build_argv(&entry.exec, &c).unwrap(), vec!["demo"]);
}
