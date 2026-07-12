//! License expression parsing for DEP-5 copyright files.
//!
//! License expressions combine license names with `or`, `and`, and `with` operators.
//! `and` binds tighter than `or`. A comma before an operator lowers its precedence,
//! e.g. `A or B, and C` means `(A or B) and C`.
//!
//! The `with` keyword attaches an exception to the preceding license name
//! (e.g. `GPL-2+ with OpenSSL-exception`).
//!
//! License names between operators are taken verbatim, including any spaces,
//! so malformed names such as `Apache 2.0` survive a parse/display round trip
//! rather than being truncated at the first space.

/// A parsed license expression from a DEP-5 copyright file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LicenseExpr {
    /// A single license name, e.g. `MIT`.
    Name(String),

    /// A license with an exception, e.g. `GPL-2+ with OpenSSL-exception`.
    WithException(String, String),

    /// All of these licenses apply simultaneously.
    And(Vec<LicenseExpr>),

    /// Any one of these licenses may be chosen.
    Or(Vec<LicenseExpr>),
}

impl LicenseExpr {
    /// Parse a license expression string.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::LicenseExpr;
    ///
    /// let expr = LicenseExpr::parse("GPL-2+ or MIT");
    /// assert_eq!(expr, LicenseExpr::Or(vec![
    ///     LicenseExpr::Name("GPL-2+".to_string()),
    ///     LicenseExpr::Name("MIT".to_string()),
    /// ]));
    ///
    /// let expr = LicenseExpr::parse("GPL-2+ with OpenSSL-exception");
    /// assert_eq!(expr, LicenseExpr::WithException(
    ///     "GPL-2+".to_string(),
    ///     "OpenSSL-exception".to_string(),
    /// ));
    /// ```
    pub fn parse(input: &str) -> Self {
        let tokens = tokenize(input);
        if tokens.is_empty() {
            return LicenseExpr::Name(String::new());
        }
        parse_expr(input, &tokens)
    }

    /// Returns the individual license names contained in this expression.
    ///
    /// For `WithException` variants, only the license name is returned,
    /// not the exception name.
    pub fn license_names(&self) -> Vec<&str> {
        let mut names = Vec::new();
        self.collect_names(&mut names);
        names
    }

    /// Locate each license name in `input` along with its byte range.
    ///
    /// Returns each license name paired with the half-open byte range it
    /// occupies in `input`. Exception words after `with` are skipped, matching
    /// [`license_names`](Self::license_names). Unlike `license_names`, no
    /// entry is emitted for expressions that contain no name token, since
    /// there is no meaningful range to report.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::LicenseExpr;
    ///
    /// let input = "GPL-2+ or MIT";
    /// assert_eq!(
    ///     LicenseExpr::name_ranges(input),
    ///     vec![("GPL-2+", 0..6), ("MIT", 10..13)],
    /// );
    ///
    /// let input = "GPL-2+ with OpenSSL-exception or MIT";
    /// assert_eq!(
    ///     LicenseExpr::name_ranges(input),
    ///     vec![("GPL-2+", 0..6), ("MIT", 33..36)],
    /// );
    /// ```
    pub fn name_ranges(input: &str) -> Vec<(&str, std::ops::Range<usize>)> {
        let tokens = tokenize(input);
        let mut out = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i].kind {
                TokenKind::Word => {
                    let last = word_run_end(&tokens, i);
                    let range = tokens[i].range.start..tokens[last].range.end;
                    out.push((&input[range.clone()], range));
                    i = last + 1;
                    if matches!(tokens.get(i).map(|t| &t.kind), Some(TokenKind::With)) {
                        i += 1;
                        if matches!(tokens.get(i).map(|t| &t.kind), Some(TokenKind::Word)) {
                            i = word_run_end(&tokens, i) + 1;
                        }
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        out
    }

    /// Whether this expression contains a top-level `or`, in which case a
    /// parent operator must be written in its comma-lowered form (`, and` /
    /// `, or`) to keep the intended grouping.
    fn needs_comma_join(&self) -> bool {
        match self {
            LicenseExpr::Or(_) => true,
            LicenseExpr::And(exprs) => exprs.iter().any(|e| e.needs_comma_join()),
            LicenseExpr::Name(_) | LicenseExpr::WithException(..) => false,
        }
    }

    fn collect_names<'a>(&'a self, names: &mut Vec<&'a str>) {
        match self {
            LicenseExpr::Name(n) => names.push(n),
            LicenseExpr::WithException(n, _) => names.push(n),
            LicenseExpr::And(exprs) | LicenseExpr::Or(exprs) => {
                for expr in exprs {
                    expr.collect_names(names);
                }
            }
        }
    }
}

impl std::fmt::Display for LicenseExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseExpr::Name(n) => f.write_str(n),
            LicenseExpr::WithException(n, e) => write!(f, "{} with {}", n, e),
            LicenseExpr::And(exprs) => {
                // `and` normally binds tighter than `or`, so an operand with
                // a top-level `or` forces the comma-lowered form: `A or B, and C`.
                let sep = if exprs.iter().any(|e| e.needs_comma_join()) {
                    ", and "
                } else {
                    " and "
                };
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        f.write_str(sep)?;
                    }
                    write!(f, "{}", expr)?;
                }
                Ok(())
            }
            LicenseExpr::Or(exprs) => {
                // An operand that itself uses the comma-lowered form needs
                // this `or` comma-lowered too: `A, and B or C, or D`.
                let sep = if exprs.iter().any(|e| e.needs_comma_join()) {
                    ", or "
                } else {
                    " or "
                };
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        f.write_str(sep)?;
                    }
                    write!(f, "{}", expr)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
enum TokenKind {
    Word,
    Or,
    And,
    With,
    Comma,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    range: std::ops::Range<usize>,
}

/// Given that `tokens[start]` is a `Word`, return the index of the last token
/// of the name run it begins: consecutive `Word` tokens, where a comma joins
/// the run only when another word follows it. This mirrors how `parse` groups
/// words into names (a comma directly before `and`/`or` is an operator,
/// anywhere else it is preserved as part of the surrounding name).
fn word_run_end(tokens: &[Token], start: usize) -> usize {
    let mut i = start;
    loop {
        let next = i + 1;
        match tokens.get(next).map(|t| &t.kind) {
            Some(TokenKind::Word) => i = next,
            Some(TokenKind::Comma)
                if matches!(tokens.get(next + 1).map(|t| &t.kind), Some(TokenKind::Word)) =>
            {
                i = next + 1;
            }
            _ => break,
        }
    }
    i
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        let Some(offset) = input[pos..].find(|c: char| !c.is_whitespace()) else {
            break;
        };
        let start = pos + offset;
        pos = match input[start..].find(char::is_whitespace) {
            Some(offset) => start + offset,
            None => input.len(),
        };
        let mut end = pos;
        let mut trailing_comma = false;
        if end > start && input.as_bytes()[end - 1] == b',' {
            trailing_comma = true;
            end -= 1;
        }
        if end > start {
            let word = &input[start..end];
            let kind = if word.eq_ignore_ascii_case("or") {
                TokenKind::Or
            } else if word.eq_ignore_ascii_case("and") {
                TokenKind::And
            } else if word.eq_ignore_ascii_case("with") {
                TokenKind::With
            } else {
                TokenKind::Word
            };
            tokens.push(Token {
                kind,
                range: start..end,
            });
        }
        if trailing_comma {
            tokens.push(Token {
                kind: TokenKind::Comma,
                range: end..end + 1,
            });
        }
    }
    tokens
}

/// Consume consecutive `Word` tokens starting at `*pos` and return the range
/// of `input` they span, or `None` if the token at `*pos` is not a `Word`.
/// The span preserves whatever separated the words in the input.
fn take_words(tokens: &[Token], pos: &mut usize) -> Option<std::ops::Range<usize>> {
    let start = match tokens.get(*pos) {
        Some(Token {
            kind: TokenKind::Word,
            range,
        }) => range.start,
        _ => return None,
    };
    let mut end = tokens[*pos].range.end;
    *pos += 1;
    while let Some(Token {
        kind: TokenKind::Word,
        range,
    }) = tokens.get(*pos)
    {
        end = range.end;
        *pos += 1;
    }
    Some(start..end)
}

/// Parse a single license term: a name optionally followed by `with <exception>`.
/// A name consumes all words until the next `or`, `and`, `with`, comma, or end,
/// so names containing spaces (as found in malformed files) survive parsing.
fn parse_term(input: &str, tokens: &[Token], pos: &mut usize) -> LicenseExpr {
    let name = match take_words(tokens, pos) {
        Some(range) => input[range].to_string(),
        None => return LicenseExpr::Name(String::new()),
    };

    if matches!(tokens.get(*pos).map(|t| &t.kind), Some(TokenKind::With)) {
        *pos += 1;
        let exception = match take_words(tokens, pos) {
            Some(range) => input[range].to_string(),
            None => String::new(),
        };
        LicenseExpr::WithException(name, exception)
    } else {
        LicenseExpr::Name(name)
    }
}

/// Parse a token stream into a `LicenseExpr`.
///
/// Handles comma-lowered precedence by splitting on `, and` / `, or` first,
/// then parsing each segment with normal precedence (`and` > `or`).
fn parse_expr(input: &str, tokens: &[Token]) -> LicenseExpr {
    // Split into segments at comma boundaries (comma + operator = low precedence).
    let mut segments: Vec<(Vec<Token>, Option<TokenKind>)> = Vec::new();
    let mut current: Vec<Token> = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind == TokenKind::Comma {
            if i + 1 < tokens.len() && matches!(tokens[i + 1].kind, TokenKind::Or | TokenKind::And)
            {
                let op = tokens[i + 1].kind.clone();
                segments.push((std::mem::take(&mut current), Some(op)));
                i += 2;
            } else {
                i += 1;
            }
        } else {
            current.push(tokens[i].clone());
            i += 1;
        }
    }
    if !current.is_empty() {
        segments.push((current, None));
    }

    // A lone comma produces tokens but no segments.
    if segments.is_empty() {
        return LicenseExpr::Name(String::new());
    }

    if segments.len() == 1 {
        return parse_segment(input, &segments[0].0);
    }

    // Group segments by their joining low-precedence operator.
    // Low-precedence `and` binds tighter than low-precedence `or`.
    // Nested same-operator expressions are spliced into their parent so
    // `A, and B and C` and `A and B and C` parse to the same flat tree.
    fn push_and_operand(group: &mut Vec<LicenseExpr>, expr: LicenseExpr) {
        match expr {
            LicenseExpr::And(exprs) => group.extend(exprs),
            other => group.push(other),
        }
    }

    let mut and_groups: Vec<Vec<LicenseExpr>> = vec![Vec::new()];
    push_and_operand(&mut and_groups[0], parse_segment(input, &segments[0].0));

    for i in 1..segments.len() {
        let preceding_op = segments[i - 1].1.as_ref().unwrap_or(&TokenKind::Or);
        if !matches!(preceding_op, TokenKind::And) {
            and_groups.push(Vec::new());
        }
        push_and_operand(
            and_groups.last_mut().unwrap(),
            parse_segment(input, &segments[i].0),
        );
    }

    let mut or_operands = Vec::new();
    for group in and_groups {
        let expr = if group.len() == 1 {
            group.into_iter().next().unwrap()
        } else {
            LicenseExpr::And(group)
        };
        match expr {
            LicenseExpr::Or(exprs) => or_operands.extend(exprs),
            other => or_operands.push(other),
        }
    }

    if or_operands.len() == 1 {
        or_operands.into_iter().next().unwrap()
    } else {
        LicenseExpr::Or(or_operands)
    }
}

/// Parse a segment (no comma-lowered operators) with normal precedence: `and` > `or`.
fn parse_segment(input: &str, tokens: &[Token]) -> LicenseExpr {
    // Split on `or` (lower precedence), then each part on `and`.
    let mut or_groups: Vec<Vec<Token>> = vec![Vec::new()];
    for tok in tokens {
        if tok.kind == TokenKind::Or {
            or_groups.push(Vec::new());
        } else {
            or_groups.last_mut().unwrap().push(tok.clone());
        }
    }

    let or_exprs: Vec<LicenseExpr> = or_groups
        .into_iter()
        .map(|group| {
            let mut and_groups: Vec<Vec<Token>> = vec![Vec::new()];
            for tok in &group {
                if tok.kind == TokenKind::And {
                    and_groups.push(Vec::new());
                } else {
                    and_groups.last_mut().unwrap().push(tok.clone());
                }
            }

            let and_exprs: Vec<LicenseExpr> = and_groups
                .into_iter()
                .map(|toks| {
                    let mut pos = 0;
                    parse_term(input, &toks, &mut pos)
                })
                .collect();

            if and_exprs.len() == 1 {
                and_exprs.into_iter().next().unwrap()
            } else {
                LicenseExpr::And(and_exprs)
            }
        })
        .collect();

    if or_exprs.len() == 1 {
        or_exprs.into_iter().next().unwrap()
    } else {
        LicenseExpr::Or(or_exprs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_name() {
        assert_eq!(LicenseExpr::parse("MIT"), LicenseExpr::Name("MIT".into()));
    }

    #[test]
    fn test_or() {
        assert_eq!(
            LicenseExpr::parse("GPL-2+ or MIT"),
            LicenseExpr::Or(vec![
                LicenseExpr::Name("GPL-2+".into()),
                LicenseExpr::Name("MIT".into()),
            ])
        );
    }

    #[test]
    fn test_and() {
        assert_eq!(
            LicenseExpr::parse("Apache-2.0 and BSD-3-clause"),
            LicenseExpr::And(vec![
                LicenseExpr::Name("Apache-2.0".into()),
                LicenseExpr::Name("BSD-3-clause".into()),
            ])
        );
    }

    #[test]
    fn test_with_exception() {
        assert_eq!(
            LicenseExpr::parse("GPL-2+ with OpenSSL-exception"),
            LicenseExpr::WithException("GPL-2+".into(), "OpenSSL-exception".into())
        );
    }

    #[test]
    fn test_with_multi_word_exception() {
        assert_eq!(
            LicenseExpr::parse("GPL-2+ with Autoconf exception"),
            LicenseExpr::WithException("GPL-2+".into(), "Autoconf exception".into())
        );
    }

    #[test]
    fn test_with_exception_then_or() {
        assert_eq!(
            LicenseExpr::parse("GPL-2+ with OpenSSL-exception or MIT"),
            LicenseExpr::Or(vec![
                LicenseExpr::WithException("GPL-2+".into(), "OpenSSL-exception".into()),
                LicenseExpr::Name("MIT".into()),
            ])
        );
    }

    #[test]
    fn test_and_binds_tighter_than_or() {
        // A or B and C → A or (B and C)
        assert_eq!(
            LicenseExpr::parse("A or B and C"),
            LicenseExpr::Or(vec![
                LicenseExpr::Name("A".into()),
                LicenseExpr::And(vec![
                    LicenseExpr::Name("B".into()),
                    LicenseExpr::Name("C".into()),
                ]),
            ])
        );
    }

    #[test]
    fn test_comma_lowers_precedence() {
        // A or B, and C → (A or B) and C
        assert_eq!(
            LicenseExpr::parse("A or B, and C"),
            LicenseExpr::And(vec![
                LicenseExpr::Or(vec![
                    LicenseExpr::Name("A".into()),
                    LicenseExpr::Name("B".into()),
                ]),
                LicenseExpr::Name("C".into()),
            ])
        );
    }

    #[test]
    fn test_case_insensitive_operators() {
        assert_eq!(
            LicenseExpr::parse("GPL-2+ OR MIT"),
            LicenseExpr::Or(vec![
                LicenseExpr::Name("GPL-2+".into()),
                LicenseExpr::Name("MIT".into()),
            ])
        );
    }

    #[test]
    fn test_license_names() {
        let expr = LicenseExpr::parse("GPL-2+ or MIT and BSD-3-clause");
        assert_eq!(expr.license_names(), vec!["GPL-2+", "MIT", "BSD-3-clause"]);
    }

    #[test]
    fn test_license_names_with_exception() {
        let expr = LicenseExpr::parse("GPL-2+ with OpenSSL-exception or MIT");
        assert_eq!(expr.license_names(), vec!["GPL-2+", "MIT"]);
    }

    #[test]
    fn test_display_round_trip_simple() {
        let input = "GPL-2+ or MIT";
        let expr = LicenseExpr::parse(input);
        assert_eq!(expr.to_string(), input);
    }

    #[test]
    fn test_display_with_exception() {
        let input = "GPL-2+ with OpenSSL-exception";
        let expr = LicenseExpr::parse(input);
        assert_eq!(expr.to_string(), input);
    }

    #[test]
    fn test_name_ranges_simple() {
        let input = "MIT";
        assert_eq!(LicenseExpr::name_ranges(input), vec![("MIT", 0..3)]);
    }

    #[test]
    fn test_name_ranges_or() {
        let input = "GPL-2+ or MIT";
        assert_eq!(
            LicenseExpr::name_ranges(input),
            vec![("GPL-2+", 0..6), ("MIT", 10..13)],
        );
    }

    #[test]
    fn test_name_ranges_and() {
        let input = "Apache-2.0 and BSD-3-clause";
        assert_eq!(
            LicenseExpr::name_ranges(input),
            vec![("Apache-2.0", 0..10), ("BSD-3-clause", 15..27)],
        );
    }

    #[test]
    fn test_name_ranges_with_exception() {
        let input = "GPL-2+ with OpenSSL-exception or MIT";
        assert_eq!(
            LicenseExpr::name_ranges(input),
            vec![("GPL-2+", 0..6), ("MIT", 33..36)],
        );
    }

    #[test]
    fn test_name_ranges_multi_word_exception() {
        let input = "GPL-2+ with Autoconf exception or MIT";
        assert_eq!(
            LicenseExpr::name_ranges(input),
            vec![("GPL-2+", 0..6), ("MIT", 34..37)],
        );
    }

    #[test]
    fn test_name_ranges_comma_lowered() {
        let input = "A or B, and C";
        assert_eq!(
            LicenseExpr::name_ranges(input),
            vec![("A", 0..1), ("B", 5..6), ("C", 12..13)],
        );
    }

    #[test]
    fn test_name_ranges_empty() {
        assert_eq!(LicenseExpr::name_ranges(""), Vec::<(&str, _)>::new());
        assert_eq!(LicenseExpr::name_ranges("   "), Vec::<(&str, _)>::new());
    }

    #[test]
    fn test_name_ranges_matches_license_names() {
        let cases = [
            "GPL-2+ or MIT",
            "Apache-2.0 and BSD-3-clause",
            "GPL-2+ with OpenSSL-exception or MIT",
            "A or B, and C",
            "GPL-1+ or Artistic or Perl",
        ];
        for input in cases {
            let from_expr: Vec<String> = LicenseExpr::parse(input)
                .license_names()
                .into_iter()
                .map(str::to_owned)
                .collect();
            let from_ranges: Vec<String> = LicenseExpr::name_ranges(input)
                .into_iter()
                .map(|(n, _)| n.to_owned())
                .collect();
            assert_eq!(from_ranges, from_expr, "mismatch for input {input:?}");
        }
    }

    #[test]
    fn test_multi_word_name() {
        assert_eq!(
            LicenseExpr::parse("Apache 2.0"),
            LicenseExpr::Name("Apache 2.0".into())
        );
    }

    #[test]
    fn test_multi_word_name_in_or() {
        assert_eq!(
            LicenseExpr::parse("Creative Commons Attribution 3.0 or MIT"),
            LicenseExpr::Or(vec![
                LicenseExpr::Name("Creative Commons Attribution 3.0".into()),
                LicenseExpr::Name("MIT".into()),
            ])
        );
    }

    #[test]
    fn test_multi_word_name_with_exception() {
        assert_eq!(
            LicenseExpr::parse("Apache 2.0 with LLVM exception"),
            LicenseExpr::WithException("Apache 2.0".into(), "LLVM exception".into())
        );
    }

    #[test]
    fn test_name_ranges_multi_word_name() {
        let input = "Apache 2.0 or MIT";
        assert_eq!(
            LicenseExpr::name_ranges(input),
            vec![("Apache 2.0", 0..10), ("MIT", 14..17)],
        );
    }

    #[test]
    fn test_non_ascii_name() {
        assert_eq!(
            LicenseExpr::parse("Ràndom"),
            LicenseExpr::Name("Ràndom".into())
        );
    }

    #[test]
    fn test_display_comma_lowered_round_trip() {
        let input = "A or B, and C";
        let expr = LicenseExpr::parse(input);
        assert_eq!(expr.to_string(), input);
        assert_eq!(LicenseExpr::parse(&expr.to_string()), expr);
    }

    #[test]
    fn test_display_comma_lowered_or_round_trip() {
        let input = "A, and B or C, or D";
        let expr = LicenseExpr::parse(input);
        assert_eq!(expr.to_string(), input);
        assert_eq!(LicenseExpr::parse(&expr.to_string()), expr);
    }

    #[test]
    fn test_stray_comma_preserved_in_name() {
        assert_eq!(
            LicenseExpr::parse("A, B, and C"),
            LicenseExpr::And(vec![
                LicenseExpr::Name("A, B".into()),
                LicenseExpr::Name("C".into()),
            ])
        );
    }

    #[test]
    fn test_lone_comma() {
        assert_eq!(LicenseExpr::parse(","), LicenseExpr::Name(String::new()));
    }

    #[test]
    fn test_three_way_or() {
        assert_eq!(
            LicenseExpr::parse("GPL-1+ or Artistic or Perl"),
            LicenseExpr::Or(vec![
                LicenseExpr::Name("GPL-1+".into()),
                LicenseExpr::Name("Artistic".into()),
                LicenseExpr::Name("Perl".into()),
            ])
        );
    }
}
