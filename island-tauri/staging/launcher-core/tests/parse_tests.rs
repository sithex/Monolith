use launcher_core::{parse_desktop_entry, ParseError};

fn sample(extra: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName=Demo\nExec=demo --flag\n{extra}")
}

#[test]
fn parse_minimal_entry() {
    let text = sample("");
    let (entry, warnings) = parse_desktop_entry(&text, "demo.desktop", None).unwrap();
    assert_eq!(entry.desktop_id, "demo.desktop");
    assert_eq!(entry.name, "Demo");
    assert_eq!(entry.exec, "demo --flag");
    assert!(entry.generic_name.is_empty());
    assert!(entry.comment.is_empty());
    assert!(entry.icon.is_empty());
    assert!(!entry.terminal);
    assert!(!entry.no_display);
    assert!(!entry.hidden);
    assert!(entry.only_show_in.is_empty());
    assert!(entry.not_show_in.is_empty());
    assert!(entry.categories.is_empty());
    assert!(entry.keywords.is_empty());
    assert!(warnings.is_empty());
}

#[test]
fn parse_requires_desktop_entry_group() {
    assert_eq!(
        parse_desktop_entry("[Desktop Action x]\nName=Demo\nExec=x\n", "d", None),
        Err(ParseError::NoDesktopEntryGroup)
    );
    assert_eq!(
        parse_desktop_entry("Name=Demo\nExec=x\n", "d", None),
        Err(ParseError::NoDesktopEntryGroup)
    );
}

#[test]
fn parse_type_missing_is_error() {
    let text = "[Desktop Entry]\nName=Demo\nExec=demo\n";
    assert_eq!(
        parse_desktop_entry(text, "d", None),
        Err(ParseError::MissingType)
    );
}

#[test]
fn parse_type_not_application_is_typed_error() {
    let text = "[Desktop Entry]\nType=Link\nName=Demo\nExec=demo\n";
    assert_eq!(
        parse_desktop_entry(text, "d", None),
        Err(ParseError::NotApplication("Link".to_string()))
    );
}

#[test]
fn parse_missing_name_is_error() {
    let text = "[Desktop Entry]\nType=Application\nExec=demo\n";
    assert_eq!(
        parse_desktop_entry(text, "d", None),
        Err(ParseError::MissingName)
    );
}

#[test]
fn parse_missing_exec_is_error() {
    let text = "[Desktop Entry]\nType=Application\nName=Demo\n";
    assert_eq!(
        parse_desktop_entry(text, "d", None),
        Err(ParseError::MissingExec)
    );
}

#[test]
fn parse_over_64kib_rejected() {
    let mut text = sample("");
    text.push_str(&"x".repeat(64 * 1024 - text.len() + 1));
    assert!(text.len() > 64 * 1024);
    assert_eq!(
        parse_desktop_entry(&text, "d", None),
        Err(ParseError::InputTooLarge)
    );
}

#[test]
fn parse_exactly_64kib_accepted() {
    let mut text = sample("");
    let pad = 64 * 1024 - text.len();
    text.push_str(&"#".repeat(pad));
    assert_eq!(text.len(), 64 * 1024);
    assert!(parse_desktop_entry(&text, "d", None).is_ok());
}

#[test]
fn parse_nul_rejected() {
    let text = "[Desktop Entry]\nType=Application\nName=A\0B\nExec=x\n";
    assert_eq!(
        parse_desktop_entry(text, "d", None),
        Err(ParseError::ContainsNul)
    );
}

#[test]
fn parse_duplicate_key_first_wins() {
    let text = "[Desktop Entry]\nType=Application\nName=First\nName=Second\nExec=run\n";
    let (entry, _) = parse_desktop_entry(text, "d", None).unwrap();
    assert_eq!(entry.name, "First");
}

#[test]
fn parse_keys_outside_first_desktop_entry_ignored() {
    let text = [
        "Name=Before\nType=Application\nExec=bad\n",
        "[Desktop Action play]\nName=Bad\nType=Application\nExec=bad\n",
        "[Desktop Entry]\nType=Application\nName=Good\nExec=good\n",
        "[Desktop Action x]\nName=Bad2\n",
    ]
    .concat();
    let (entry, _) = parse_desktop_entry(&text, "d", None).unwrap();
    assert_eq!(entry.name, "Good");
    assert_eq!(entry.exec, "good");
}

#[test]
fn parse_second_desktop_entry_group_ignored() {
    let text = [
        "[Desktop Entry]\nType=Application\nName=First\nExec=first\n",
        "[Desktop Entry]\nType=Application\nName=Second\nExec=second\n",
    ]
    .concat();
    let (entry, _) = parse_desktop_entry(&text, "d", None).unwrap();
    assert_eq!(entry.name, "First");
}

#[test]
fn parse_comments_and_blank_lines_ignored() {
    let text = "# comment\n\n[Desktop Entry]\n# inside\nType=Application\nName=Demo\nExec=demo\n";
    let (entry, _) = parse_desktop_entry(text, "d", None).unwrap();
    assert_eq!(entry.name, "Demo");
}

#[test]
fn parse_spaces_around_equals_ignored() {
    let text = "[Desktop Entry]\nType = Application\nName = Demo\nExec = demo\n";
    let (entry, _) = parse_desktop_entry(text, "d", None).unwrap();
    assert_eq!(entry.exec, "demo");
}

#[test]
fn parse_string_escape_s_in_name() {
    let text = "[Desktop Entry]\nType=Application\nName=Demo\\sLauncher\nExec=demo\n";
    let (entry, _) = parse_desktop_entry(text, "d", None).unwrap();
    assert_eq!(entry.name, "Demo Launcher");
}

#[test]
fn parse_boolean_non_true_is_false_with_warning() {
    let text = "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo\nTerminal=True\nNoDisplay=1\nHidden=maybe\n";
    let (entry, warnings) = parse_desktop_entry(text, "d", None).unwrap();
    assert!(!entry.terminal);
    assert!(!entry.no_display);
    assert!(!entry.hidden);
    assert_eq!(warnings.len(), 3);
    assert!(warnings.iter().all(|w| w.message.contains("boolean")));
}

#[test]
fn parse_boolean_true_false_valid() {
    let text = "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo\nTerminal=true\nNoDisplay=false\nHidden=true\n";
    let (entry, warnings) = parse_desktop_entry(text, "d", None).unwrap();
    assert!(entry.terminal);
    assert!(!entry.no_display);
    assert!(entry.hidden);
    assert!(warnings.is_empty());
}

#[test]
fn parse_list_semicolon_escape() {
    let text = [
        "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo\n",
        "OnlyShowIn=a;b\\;c;\nNotShowIn=x;\nCategories=cat1;cat2\nKeywords=k1;k2;\n",
    ]
    .concat();
    let (entry, _) = parse_desktop_entry(&text, "d", None).unwrap();
    assert_eq!(entry.only_show_in, vec!["a".to_string(), "b;c".to_string()]);
    assert_eq!(entry.not_show_in, vec!["x".to_string()]);
    assert_eq!(
        entry.categories,
        vec!["cat1".to_string(), "cat2".to_string()]
    );
    assert_eq!(entry.keywords, vec!["k1".to_string(), "k2".to_string()]);
}

#[test]
fn parse_localized_fallback_rs_latin_full_chain() {
    let text = [
        "[Desktop Entry]\nType=Application\nExec=demo\n",
        "Name[en]=English\nName[sr_RS@latin]=RsLatin\nName[sr_RS]=Rs\n",
        "Name[sr@latin]=LangMod\nName[sr]=Lang\nName=Fallback\n",
    ]
    .concat();
    let cases = [
        (Some("sr_RS@latin"), "RsLatin"),
        (Some("sr_RS"), "Rs"),
        (Some("sr@latin"), "LangMod"),
        (Some("sr"), "Lang"),
        (Some("en"), "English"),
        (None, "Fallback"),
    ];
    for (locale, expected) in cases {
        let (entry, _) = parse_desktop_entry(&text, "d", locale).unwrap();
        assert_eq!(entry.name, expected, "locale {locale:?}");
    }
}

#[test]
fn parse_localized_fallback_reaches_lang_at_modifier() {
    let text = "[Desktop Entry]\nType=Application\nExec=demo\nName[sr@latin]=Mod\n";
    let (entry, _) = parse_desktop_entry(text, "d", Some("sr_RS@latin")).unwrap();
    assert_eq!(entry.name, "Mod");
}

#[test]
fn parse_localized_fallback_de_de_utf8() {
    let text = [
        "[Desktop Entry]\nType=Application\nExec=demo\n",
        "Name[de_DE]=DeDe\nName[de]=De\nName=Fallback\n",
    ]
    .concat();
    let (e1, _) = parse_desktop_entry(&text, "d", Some("de_DE.UTF-8")).unwrap();
    assert_eq!(e1.name, "DeDe");
    let (e2, _) = parse_desktop_entry(&text, "d", Some("de_DE")).unwrap();
    assert_eq!(e2.name, "DeDe");
    let (e3, _) = parse_desktop_entry(&text, "d", Some("de")).unwrap();
    assert_eq!(e3.name, "De");
    let (e4, _) = parse_desktop_entry(&text, "d", Some("pt")).unwrap();
    assert_eq!(e4.name, "Fallback");
}

#[test]
fn parse_localized_fallback_pt() {
    let text = "[Desktop Entry]\nType=Application\nExec=demo\nName[pt]=Portugues\nName=Fallback\n";
    let (entry, _) = parse_desktop_entry(text, "d", Some("pt")).unwrap();
    assert_eq!(entry.name, "Portugues");
}

#[test]
fn parse_localized_generic_name_comment_icon_keywords() {
    let text = [
        "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo\n",
        "GenericName[fr]=Générique\nComment[fr]=Un commentaire\nIcon[fr]=icone\nKeywords[fr]=mot;clef;\n",
    ]
    .concat();
    let (entry, _) = parse_desktop_entry(&text, "d", Some("fr")).unwrap();
    assert_eq!(entry.generic_name, "Générique");
    assert_eq!(entry.comment, "Un commentaire");
    assert_eq!(entry.icon, "icone");
    assert_eq!(entry.keywords, vec!["mot".to_string(), "clef".to_string()]);
    let (entry2, _) = parse_desktop_entry(&text, "d", None).unwrap();
    assert!(entry2.generic_name.is_empty());
    assert!(entry2.comment.is_empty());
    assert!(entry2.icon.is_empty());
    assert!(entry2.keywords.is_empty());
}

#[test]
fn parse_type_line_with_equals_inside_value() {
    let text = "[Desktop Entry]\nType=Application\nName=Demo\nExec=env A=B demo\n";
    let (entry, _) = parse_desktop_entry(text, "d", None).unwrap();
    assert_eq!(entry.exec, "env A=B demo");
}

#[test]
fn parse_localized_name_only_no_unlocalized_without_locale_errors() {
    let text = "[Desktop Entry]\nType=Application\nName[sr]=Српски\nExec=demo\n";
    assert_eq!(
        parse_desktop_entry(text, "d", None),
        Err(ParseError::MissingName)
    );
    let (entry, _) = parse_desktop_entry(text, "d", Some("sr")).unwrap();
    assert_eq!(entry.name, "Српски");
}
