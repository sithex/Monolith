# VERIFICATION — launcher-core crate

Location: `island-tauri/staging/launcher-core/`.
Scope: pure core behind SPEC §0.5 "Design direction" (Apps bullet) — the Phase 7
app-grid launcher for the Hermes Island widget. The crate parses `.desktop`
file text into launcher entries (`parse_desktop_entry`), turns an entry's
`Exec` line into an argv (`build_argv`, freedesktop.org Desktop Entry Spec §7
copied exactly), and merges/searchs entries across XDG dirs (`Catalog`). The
Tauri side reads `.desktop` files and spawns processes; this crate only decides
**what** an entry is and **how** to invoke it. Platform-free: no IO, no
network, no threads, no clock, no process spawning, no `unsafe`, never panics
on any input. Dependencies: serde (derive) + serde_json only. Wire types are
camelCase via serde, no `#[serde(alias)]`.

## Gate commands — final state (all green)

```
$ nice -n 10 cargo generate-lockfile
(lockfile present; all later steps run with --locked)

$ nice -n 10 cargo fmt --check
(clean)
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking launcher-core v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/launcher-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s
exit=0

$ nice -n 10 cargo test --locked
   Compiling launcher-core v0.1.0 (.../launcher-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.19s
     Running unittests src/lib.rs (target/debug/deps/launcher_core-537d094182159118)

running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/catalog_tests.rs (target/debug/deps/catalog_tests-18ede5483bdf22fa)

running 16 tests
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/exec_tests.rs (target/debug/deps/exec_tests-2215598c8a9af819)

running 29 tests
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/fuzz_tests.rs (target/debug/deps/fuzz_tests-f3ae131bd7b6d647)

running 1 test
test fuzz_desktop_text_and_exec_never_panics ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/parse_tests.rs (target/debug/deps/parse_tests-04933e42795937cc)

running 25 tests
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests launcher_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

Total: 78 tests (7 unit + 16 catalog + 29 exec + 1 fuzz + 25 parse), all green
under `nice -n 10`.

## Mutation checks (all actually run on the current code)

Method: snapshot the pristine tree to scratch (`--exclude=target`), apply one
breakage, run `nice -n 10 cargo test --locked --no-fail-fast`, record failing
test names, restore pristine (touching restored sources so cargo rebuilds),
then `diff -r --exclude=target <pristine> .` verified empty for every
mutation. All six restored byte-identical, and the final full gate re-run on
the restored tree is green (above). Every mutation compiled (a non-compiling
mutation would not count). Each mutation fails *named* tests, not only the
fuzz test.

| # | Breakage applied | Failing tests observed |
|---|---|---|
| 1 | Locale candidate chain skips `lang@MODIFIER` (the `sr@latin` candidate dropped in `Locale::candidates`) | `parse::tests::locale_candidates_full_chain`, `parse::tests::locale_candidates_lang_at_modifier`, `parse_localized_fallback_reaches_lang_at_modifier`, `parse_localized_fallback_rs_latin_full_chain` |
| 2 | Unknown field code accepted: `%<alpha>` kept literally in the argv instead of `ExecError::UnknownFieldCode` | `exec_unknown_field_code_rejected` (also `fuzz_desktop_text_and_exec_never_panics` via the "no unexpanded % code" assertion) |
| 3 | `%c` split on whitespace into multiple arguments instead of one (the entry name with spaces no longer stays one arg) | `exec_percent_c_name_with_spaces_stays_one_arg`, `exec_percent_c_inline` |
| 4 | String-escape rule neutered (`apply_string_escapes` returns an empty string), so escaping is effectively removed from `build_argv` | `exec_literal_backslash_inside_quotes_needs_four`, `exec_string_escape_s_inside_quotes`, `exec_dollar_escaped_inside_quotes`, and the exec tests that expect a non-empty Ok argv |
| 5 | `Hidden` entries are skipped on insert, so they can no longer mask the same id from lower-priority dirs | `catalog_hidden_higher_priority_masks_lower` |
| 6 | `NotShowIn` intersection dropped from `visible_in` | `visible_not_show_in_hides`, `visible_both_lists` |

Extra probe (not counted above, no named test failed): swapping the escape step
to *after* unquoting (tokenize raw text, then escape per token) passed all 78
tests. That ordering difference only shows up when an escape (`\s`, `\t`,
`\n`, `\r`) appears *between unquoted arguments* — where spec §7's
"escape-before-quoting" turns it into an argument separator — and no test pins
that case. It is a test gap, not an equivalence; it is closed by the named tests in the Follow-up section below.

## Decisions

- **Only the first `[Desktop Entry]` group is processed.** A second
  `[Desktop Entry]` group and every other group (e.g. `[Desktop Action x]`)
  are ignored entirely.
- **Duplicate keys: first occurrence wins** (`HashMap::entry().or_insert`).
- **`%f`/`%u` with several files use only the first** (documented; the spec is
  quiet about multi-file semantics for the lowercase codes).
- **`%F`/`%U`/`%i` must be a whole argument.** A field code inside a
  double-quoted argument is rejected (`FieldCodeInQuotedArgument`) because the
  spec leaves it undefined and the prompt says to treat such cases as invalid.
- **A lone `%` not followed by a letter is literal** (falls through to the
  catch-all arm, like `%%`). Known single-letter codes are `%f %F %u %U %i %c
  %k %%`; `%d %D %n %N %v %m` are deprecated and removed; any other alphabetic
  code is `UnknownFieldCode`.
- **String escapes (\s \n \t \r \\) apply before the quoting rule** (§7).
  Pinned by named tests for both the quoted and the unquoted-separator cases
  (see Follow-up below).
- **Catalog precedence: earlier dir (lower rank) wins; a `Hidden` entry in a
  higher-priority dir masks the same id from lower dirs**, because it occupies
  the slot and the lower dir's entry never replaces it.
- **Booleans are exactly `true`/`false`**; anything else is treated as
  `false` and emits a `Warning`.
- **Locale resolution ignores the encoding part** (`de_DE.UTF-8` →
  `de_DE`) and tries, in order: `lang_COUNTRY@MODIFIER`, `lang_COUNTRY`,
  `lang@MODIFIER`, `lang`, then the unlocalized key. If no `Name` resolves at
  all (including with no locale), it is a `MissingName` error.
- **`Type` must be exactly `Application`**; any other value is a typed
  `NotApplication` error carrying the value.
- **NUL bytes are rejected** (`ContainsNul`) even though `&str` can carry
  them; input larger than 64 KiB is rejected (`InputTooLarge`) before parsing.

## Follow-up after controller audit

Two gaps from the controller audit were closed on the current tree.

### 1. Escape-before-quoting between unquoted arguments

Named tests now pin the §7 ordering for the exact case the earlier probe could
not distinguish:

- `exec_escape_before_quoting_unquoted_separator`: `Exec=app\sflag` expands to
  two args `["app", "flag"]` (`\s` becomes a space *before* quoting, and that
  space separates arguments).
- `exec_escape_before_quoting_quoted_stays_one_arg`: `Exec="app\sflag"` stays
  one arg `["app flag"]`.

The implementation already applied string escapes before tokenizing, so no code
change was needed. Re-running the swap-escape-order mutation (tokenize raw, then
escape per token) now FAILS the named test
`exec_escape_before_quoting_unquoted_separator` (1 failed); the quoted test
passes under the mutation, exactly as the spec-ordering distinction predicts.
The mutation was reverted and the tree diff-checked byte-identical.

### 2. `%i` with an empty Icon

Spec §7: `%i` "should not expand to any arguments if the Icon key is empty or
missing". `build_argv` previously treated `Some("")` as a present icon and
pushed `--icon ""`. Fixed in `expand_token`: an empty icon is treated like
`None` and pushes nothing.

Named tests added:

- `exec_percent_i_empty_icon_removed`: `ExecContext { icon: Some(""), .. }`
  → `%i` adds no arguments.
- `exec_percent_i_empty_icon_from_parse_removed`: an entry with `Icon=`
  (present, empty) parses to an empty icon, and feeding that into
  `ExecContext.icon` makes `%i` add no arguments.

Mutation: reverting the guard so `%i` pushes `--icon` for `Some("")` FAILS both
named tests (2 failed). The mutation was reverted and the tree diff-checked
byte-identical.

### Gate (final, restored tree)

```
$ nice -n 10 cargo fmt --check                          # clean
$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings   # clean
$ nice -n 10 cargo test --locked --no-fail-fast          # 82 tests, all green
```

Total now 82 tests (was 78): 7 unit + 16 catalog + 33 exec + 1 fuzz + 25 parse.
Both mutations fail *named*, non-fuzz tests.

## UNVERIFIED

- **Tauri actor wiring** — `.desktop` discovery across XDG dirs, handing the
  real dir rank order to `add_dir_entries`, and process spawning are outside
  this crate's scope.
- **`%f`/`%u` with >1 file** — pinned to first-file-only; spec §7 is not
  explicit about the multi-file behavior of the lowercase codes.
- **Locale semantics beyond the fixed candidate chain** — e.g. real glibc
  `LANG=C`/country-only fallbacks and modifier handling are not modeled; the
  crate only implements the exact chain in the prompt.
- **Non-ASCII argv semantics** — multibyte characters survive unquoting and
  expansion unchanged, but there is no normalization or collation; search
  case-folding is ASCII-oriented (`to_lowercase`).
- **Fuzz coverage** — the hand-rolled LCG fuzz test is deterministic and
  bounded, not a coverage-guided fuzzer; it asserts never-panics plus
  "every Ok argv is non-empty and contains no unexpanded `%` code".
