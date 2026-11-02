use crate::error::ExecError;

/// Context for expanding an `Exec` line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecContext {
    /// Translated entry name (used by `%c`).
    pub name: String,
    /// Icon name/path (used by `%i`); `None` when the entry has no icon.
    pub icon: Option<String>,
    /// Absolute path of the desktop file (used by `%k`); `None` when unknown.
    pub desktop_file: Option<String>,
    /// Files/URLs passed to the launcher. May be empty: then `%f`/`%F`/
    /// `%u`/`%U` are removed. `%f`/`%u` use only the first element.
    pub files: Vec<String>,
}

impl ExecContext {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Turn an `Exec` line into an argv, applying §7 exactly: string escapes
/// first, then quoting, then field-code expansion (once, no rescan).
pub fn build_argv(exec: &str, ctx: &ExecContext) -> Result<Vec<String>, ExecError> {
    let escaped = apply_string_escapes(exec);
    let tokens = tokenize(&escaped)?;
    let mut argv: Vec<String> = Vec::new();
    let mut file_codes = 0usize;
    for token in &tokens {
        argv.extend(expand_token(token, ctx, &mut file_codes)?);
    }
    if file_codes > 1 {
        return Err(ExecError::MultipleFileCodes);
    }
    if argv.is_empty() {
        return Err(ExecError::EmptyCommand);
    }
    if argv[0].contains('=') {
        return Err(ExecError::ExecutableContainsEquals);
    }
    Ok(argv)
}

/// The string-escape rule (`\s \n \t \r \\`), applied to the whole line
/// *before* the quoting rule (§7).
fn apply_string_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Split into arguments on unquoted whitespace. Double quotes group; inside
/// quotes only `"`, `` ` `, `$` and `\` are escaped with a preceding
/// backslash. Any `%` inside a quoted argument is undefined by spec and is
/// rejected. An unterminated quote is an error. Empty tokens are dropped.
fn tokenize(s: &str) -> Result<Vec<String>, ExecError> {
    let mut args: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => in_quotes = !in_quotes,
            '\\' if in_quotes => match chars.next() {
                Some('"') => cur.push('"'),
                Some('`') => cur.push('`'),
                Some('$') => cur.push('$'),
                Some('\\') => cur.push('\\'),
                Some(other) => {
                    cur.push('\\');
                    cur.push(other);
                }
                None => cur.push('\\'),
            },
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            '%' if in_quotes => return Err(ExecError::FieldCodeInQuotedArgument),
            c => cur.push(c),
        }
    }
    if in_quotes {
        return Err(ExecError::UnterminatedQuote);
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    Ok(args)
}

/// Expand one argument token. `%F`/`%U`/`%i` must be the whole argument;
/// `%f`/`%u`/`%c`/`%k` may be inline. `%%` is a literal `%`; deprecated
/// codes are removed; any other alphabetic code is unknown (error).
fn expand_token(
    token: &str,
    ctx: &ExecContext,
    file_codes: &mut usize,
) -> Result<Vec<String>, ExecError> {
    let chars: Vec<char> = token.chars().collect();
    let len = chars.len();
    let mut args: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    while i < len {
        let c = chars[i];
        if c != '%' {
            buf.push(c);
            i += 1;
            continue;
        }
        match chars.get(i + 1).copied() {
            Some('%') => {
                buf.push('%');
                i += 2;
            }
            Some('f') | Some('u') => {
                *file_codes += 1;
                if let Some(first) = ctx.files.first() {
                    buf.push_str(first);
                }
                i += 2;
            }
            Some('F') | Some('U') => {
                if !buf.is_empty() || i + 2 != len {
                    return Err(ExecError::FieldCodeNotStandalone(chars[i + 1]));
                }
                *file_codes += 1;
                args.extend(ctx.files.iter().cloned());
                i += 2;
            }
            Some('i') => {
                if !buf.is_empty() || i + 2 != len {
                    return Err(ExecError::FieldCodeNotStandalone('i'));
                }
                // `%i` expands to no arguments when the Icon key is empty or
                // missing (§7): treat `Some("")` as if it were `None`.
                if let Some(icon) = &ctx.icon {
                    if !icon.is_empty() {
                        args.push("--icon".to_string());
                        args.push(icon.clone());
                    }
                }
                i += 2;
            }
            Some('c') => {
                buf.push_str(&ctx.name);
                i += 2;
            }
            Some('k') => {
                if let Some(df) = &ctx.desktop_file {
                    buf.push_str(df);
                }
                i += 2;
            }
            Some('d') | Some('D') | Some('n') | Some('N') | Some('v') | Some('m') => {
                i += 2;
            }
            Some(o) if o.is_ascii_alphabetic() => {
                return Err(ExecError::UnknownFieldCode(o));
            }
            _ => {
                buf.push('%');
                i += 1;
            }
        }
    }
    if !buf.is_empty() {
        args.push(buf);
    }
    Ok(args)
}
