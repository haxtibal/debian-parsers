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

/// An error from the strict expression parsers.
///
/// [`LicenseExpr::parse`] never fails (malformed input degrades to verbatim
/// names); [`LicenseExpr::parse_strict`] and [`LicenseExpr::parse_spdx`]
/// report malformed input with this error instead, so callers can fall back
/// to treating the field as an opaque literal.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExprParseError {
    message: String,
    input: String,
}

impl ExprParseError {
    fn new(message: impl Into<String>, input: &str) -> Self {
        ExprParseError {
            message: message.into(),
            input: input.to_string(),
        }
    }

    /// The expression that failed to parse.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// What was wrong with it, without the expression itself.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ExprParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} in license expression {:?}", self.message, self.input)
    }
}

impl std::error::Error for ExprParseError {}

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

    /// Parse a license expression string, rejecting malformed input.
    ///
    /// Unlike [`parse`](Self::parse), which accepts anything (unparseable
    /// constructs survive verbatim inside names), this enforces the DEP-5
    /// short-name grammar: a license name is a single token, operators may
    /// not dangle, and a comma must introduce a lowered `and`/`or`. Callers
    /// use it to detect fields that are not really expressions and treat
    /// them as opaque literals instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::LicenseExpr;
    ///
    /// assert!(LicenseExpr::parse_strict("GPL-2+ or MIT").is_ok());
    /// assert!(LicenseExpr::parse_strict("Apache 2.0").is_err());
    /// assert!(LicenseExpr::parse_strict("MIT or").is_err());
    /// ```
    pub fn parse_strict(input: &str) -> Result<Self, ExprParseError> {
        let tokens = tokenize(input);
        StrictParser {
            input,
            tokens: &tokens,
            pos: 0,
        }
        .parse()
    }

    /// Parse an SPDX license expression.
    ///
    /// SPDX expressions use uppercase `AND`/`OR`/`WITH` operators (accepted
    /// case-insensitively here, as found in the wild) and parentheses for
    /// grouping; `WITH` attaches an exception identifier to a single license
    /// id. The resulting tree uses the same [`LicenseExpr`] vocabulary as the
    /// DEP-5 parsers, with leaf names kept verbatim; use
    /// [`map_names`](Self::map_names) to convert them to DEP-5 names.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::LicenseExpr;
    ///
    /// let expr = LicenseExpr::parse_spdx("(MIT AND ISC) OR GPL-2.0-only").unwrap();
    /// assert_eq!(expr, LicenseExpr::Or(vec![
    ///     LicenseExpr::And(vec![
    ///         LicenseExpr::Name("MIT".to_string()),
    ///         LicenseExpr::Name("ISC".to_string()),
    ///     ]),
    ///     LicenseExpr::Name("GPL-2.0-only".to_string()),
    /// ]));
    /// ```
    pub fn parse_spdx(input: &str) -> Result<Self, ExprParseError> {
        let tokens = spdx_tokenize(input);
        SpdxParser {
            input,
            tokens: &tokens,
            pos: 0,
        }
        .parse()
    }

    /// Render the expression as an unambiguous SPDX string.
    ///
    /// Compound operands are parenthesised, so the result round-trips
    /// through [`parse_spdx`](Self::parse_spdx) even for nestings DEP-5
    /// cannot express. Leaf names and exceptions are emitted verbatim; use
    /// [`map_names`](Self::map_names) first to convert DEP-5 names to SPDX
    /// identifiers.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::LicenseExpr;
    ///
    /// let expr = LicenseExpr::parse_spdx("(MIT AND ISC) OR GPL-2.0-only").unwrap();
    /// assert_eq!(expr.to_spdx_string(), "(MIT AND ISC) OR GPL-2.0-only");
    /// ```
    pub fn to_spdx_string(&self) -> String {
        fn operand(expr: &LicenseExpr) -> String {
            match expr {
                LicenseExpr::And(_) | LicenseExpr::Or(_) => {
                    format!("({})", expr.to_spdx_string())
                }
                _ => expr.to_spdx_string(),
            }
        }
        match self {
            LicenseExpr::Name(n) => n.clone(),
            LicenseExpr::WithException(n, e) => format!("{} WITH {}", n, e),
            LicenseExpr::And(exprs) => exprs.iter().map(operand).collect::<Vec<_>>().join(" AND "),
            LicenseExpr::Or(exprs) => exprs.iter().map(operand).collect::<Vec<_>>().join(" OR "),
        }
    }

    /// Rebuild the expression with every leaf name and exception converted.
    ///
    /// Superseded by [`map_leaves`](Self::map_leaves), which takes one closure
    /// over a whole leaf rather than two interchangeable ones. The two
    /// converters here have the same signature, so passing them in the wrong
    /// order compiles and silently swaps names for exceptions.
    #[deprecated(since = "0.1.55", note = "use LicenseExpr::map_leaves instead")]
    pub fn map_names(
        &self,
        convert_name: &dyn Fn(&str) -> String,
        convert_exception: &dyn Fn(&str) -> String,
    ) -> LicenseExpr {
        self.map_leaves(|leaf| match leaf {
            LicenseExpr::Name(n) => LicenseExpr::Name(convert_name(n)),
            LicenseExpr::WithException(n, e) => {
                LicenseExpr::WithException(convert_name(n), convert_exception(e))
            }
            other => other.clone(),
        })
    }

    /// Rebuild the expression with every leaf converted.
    ///
    /// Structural: the and/or/with shape is untouched, and `convert` is called
    /// on each [`Name`](LicenseExpr::Name) and
    /// [`WithException`](LicenseExpr::WithException) leaf in turn. This is how
    /// an expression moves between vocabularies, e.g. an SPDX-parsed tree into
    /// DEP-5 short names. Passing the leaf whole (rather than a name and an
    /// exception separately) means a conversion sees the license and its
    /// exception together, which matters when the pair maps to a single name.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::LicenseExpr;
    ///
    /// let expr = LicenseExpr::parse_spdx("MIT OR GPL-2.0-only").unwrap();
    /// let mapped = expr.map_leaves(|leaf| match leaf {
    ///     LicenseExpr::Name(name) if name == "MIT" => LicenseExpr::Name("Expat".to_string()),
    ///     other => other.clone(),
    /// });
    /// assert_eq!(mapped.to_string(), "Expat or GPL-2.0-only");
    /// ```
    pub fn map_leaves(&self, convert: impl Fn(&LicenseExpr) -> LicenseExpr) -> LicenseExpr {
        fn walk(expr: &LicenseExpr, convert: &impl Fn(&LicenseExpr) -> LicenseExpr) -> LicenseExpr {
            match expr {
                LicenseExpr::Name(_) | LicenseExpr::WithException(..) => convert(expr),
                LicenseExpr::And(exprs) => {
                    LicenseExpr::And(exprs.iter().map(|e| walk(e, convert)).collect())
                }
                LicenseExpr::Or(exprs) => {
                    LicenseExpr::Or(exprs.iter().map(|e| walk(e, convert)).collect())
                }
            }
        }
        walk(self, &convert)
    }

    /// The expression's leaves ([`Name`](LicenseExpr::Name) and
    /// [`WithException`](LicenseExpr::WithException) nodes), in order.
    ///
    /// Unlike [`license_names`](Self::license_names) this keeps the
    /// exception attached to its license, so a caller grouping by "license
    /// as licensed" (where `GPL-2 with an exception` is not `GPL-2`) can key
    /// off each leaf as a whole.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::LicenseExpr;
    ///
    /// let expr = LicenseExpr::parse("GPL-2+ with OpenSSL-exception or MIT");
    /// let leaves = expr.leaves();
    /// assert_eq!(leaves.len(), 2);
    /// assert_eq!(leaves[0].to_string(), "GPL-2+ with OpenSSL-exception");
    /// assert_eq!(leaves[1].to_string(), "MIT");
    /// ```
    pub fn leaves(&self) -> Vec<&LicenseExpr> {
        let mut out = Vec::new();
        fn collect<'a>(expr: &'a LicenseExpr, out: &mut Vec<&'a LicenseExpr>) {
            match expr {
                LicenseExpr::Name(_) | LicenseExpr::WithException(..) => out.push(expr),
                LicenseExpr::And(exprs) | LicenseExpr::Or(exprs) => {
                    for expr in exprs {
                        collect(expr, out);
                    }
                }
            }
        }
        collect(self, &mut out);
        out
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

impl std::str::FromStr for LicenseExpr {
    type Err = ExprParseError;

    /// Parse a DEP-5 license expression, rejecting malformed input.
    ///
    /// This is [`LicenseExpr::parse_strict`], not the lenient
    /// [`LicenseExpr::parse`]: a `FromStr` that never fails would make the
    /// `Result` a lie. Reading arbitrary copyright fields, where prose in a
    /// `License` field must survive rather than fail, wants `parse` instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::LicenseExpr;
    ///
    /// let expr: LicenseExpr = "GPL-2+ or MIT".parse().unwrap();
    /// assert_eq!(expr.license_names(), vec!["GPL-2+", "MIT"]);
    /// assert!("Apache 2.0".parse::<LicenseExpr>().is_err());
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LicenseExpr::parse_strict(s)
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

/// Strict recursive-descent parser for the DEP-5 short-name grammar
/// (copyright-format 1.0 sec 7.2). The comma is a precedence level looser
/// than `or`, applied as a left-associative fold:
///
/// ```text
/// comma-expr := or-expr ( "," ("and"|"or") or-expr )*
/// or-expr    := and-expr ( "or" and-expr )*
/// and-expr   := with-expr ( "and" with-expr )*
/// with-expr  := NAME ( "with" NAME+ )?
/// ```
struct StrictParser<'a> {
    input: &'a str,
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> StrictParser<'a> {
    fn err<T>(&self, message: &str) -> Result<T, ExprParseError> {
        Err(ExprParseError::new(message, self.input))
    }

    fn peek(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn parse(mut self) -> Result<LicenseExpr, ExprParseError> {
        if self.tokens.is_empty() {
            return self.err("empty expression");
        }
        let expr = self.comma_expr()?;
        if self.pos != self.tokens.len() {
            return self.err("trailing tokens");
        }
        Ok(expr)
    }

    fn comma_expr(&mut self) -> Result<LicenseExpr, ExprParseError> {
        let mut acc = self.or_expr()?;
        while self.peek() == Some(&TokenKind::Comma) {
            self.pos += 1;
            let op = match self.peek() {
                Some(TokenKind::And) => TokenKind::And,
                Some(TokenKind::Or) => TokenKind::Or,
                _ => return self.err("expected 'and' or 'or' after comma"),
            };
            self.pos += 1;
            let rhs = self.or_expr()?;
            // Splice same-operator operands so the tree matches what the
            // lenient parser builds for the equivalent expression.
            acc = match (op, acc) {
                (TokenKind::And, LicenseExpr::And(mut exprs)) => {
                    exprs.push(rhs);
                    LicenseExpr::And(exprs)
                }
                (TokenKind::And, acc) => LicenseExpr::And(vec![acc, rhs]),
                (_, LicenseExpr::Or(mut exprs)) => {
                    exprs.push(rhs);
                    LicenseExpr::Or(exprs)
                }
                (_, acc) => LicenseExpr::Or(vec![acc, rhs]),
            };
        }
        Ok(acc)
    }

    fn or_expr(&mut self) -> Result<LicenseExpr, ExprParseError> {
        let mut terms = vec![self.and_expr()?];
        while self.peek() == Some(&TokenKind::Or) {
            self.pos += 1;
            terms.push(self.and_expr()?);
        }
        Ok(if terms.len() == 1 {
            terms.pop().unwrap()
        } else {
            LicenseExpr::Or(terms)
        })
    }

    fn and_expr(&mut self) -> Result<LicenseExpr, ExprParseError> {
        let mut terms = vec![self.with_expr()?];
        while self.peek() == Some(&TokenKind::And) {
            self.pos += 1;
            terms.push(self.with_expr()?);
        }
        Ok(if terms.len() == 1 {
            terms.pop().unwrap()
        } else {
            LicenseExpr::And(terms)
        })
    }

    fn with_expr(&mut self) -> Result<LicenseExpr, ExprParseError> {
        let name = self.name()?;
        if self.peek() != Some(&TokenKind::With) {
            return Ok(LicenseExpr::Name(name));
        }
        self.pos += 1;
        // The exception phrase may span several words ("Autoconf exception").
        let start = match self.tokens.get(self.pos) {
            Some(Token {
                kind: TokenKind::Word,
                range,
            }) => range.start,
            _ => return self.err("expected exception after 'with'"),
        };
        let mut end = self.tokens[self.pos].range.end;
        self.pos += 1;
        while let Some(Token {
            kind: TokenKind::Word,
            range,
        }) = self.tokens.get(self.pos)
        {
            end = range.end;
            self.pos += 1;
        }
        Ok(LicenseExpr::WithException(
            name,
            self.input[start..end].to_string(),
        ))
    }

    fn name(&mut self) -> Result<String, ExprParseError> {
        match self.tokens.get(self.pos) {
            Some(Token {
                kind: TokenKind::Word,
                range,
            }) => {
                self.pos += 1;
                // A second consecutive word would be a multi-word name, which
                // the short-name grammar does not allow.
                if let Some(TokenKind::Word) = self.peek() {
                    return self.err("expected operator between license names");
                }
                Ok(self.input[range.clone()].to_string())
            }
            Some(_) => self.err("expected license name"),
            None => self.err("unexpected end of expression"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SpdxToken<'a> {
    Id(&'a str),
    And,
    Or,
    With,
    Open,
    Close,
}

fn spdx_tokenize(input: &str) -> Vec<SpdxToken<'_>> {
    fn classify(word: &str) -> SpdxToken<'_> {
        if word.eq_ignore_ascii_case("and") {
            SpdxToken::And
        } else if word.eq_ignore_ascii_case("or") {
            SpdxToken::Or
        } else if word.eq_ignore_ascii_case("with") {
            SpdxToken::With
        } else {
            SpdxToken::Id(word)
        }
    }
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in input.char_indices() {
        match c {
            '(' | ')' => {
                if let Some(s) = start.take() {
                    tokens.push(classify(&input[s..i]));
                }
                tokens.push(if c == '(' {
                    SpdxToken::Open
                } else {
                    SpdxToken::Close
                });
            }
            c if c.is_whitespace() => {
                if let Some(s) = start.take() {
                    tokens.push(classify(&input[s..i]));
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(i);
                }
            }
        }
    }
    if let Some(s) = start {
        tokens.push(classify(&input[s..]));
    }
    tokens
}

/// Recursive-descent parser for SPDX license expressions:
///
/// ```text
/// or-expr   := and-expr  ( "OR"  and-expr )*
/// and-expr  := with-expr ( "AND" with-expr )*
/// with-expr := atom ( "WITH" exception-id )?
/// atom      := license-id | "(" or-expr ")"
/// ```
struct SpdxParser<'a> {
    input: &'a str,
    tokens: &'a [SpdxToken<'a>],
    pos: usize,
}

impl<'a> SpdxParser<'a> {
    fn err<T>(&self, message: &str) -> Result<T, ExprParseError> {
        Err(ExprParseError::new(message, self.input))
    }

    fn parse(mut self) -> Result<LicenseExpr, ExprParseError> {
        if self.tokens.is_empty() {
            return self.err("empty expression");
        }
        let expr = self.or_expr()?;
        if self.pos != self.tokens.len() {
            return self.err("trailing tokens");
        }
        Ok(expr)
    }

    fn or_expr(&mut self) -> Result<LicenseExpr, ExprParseError> {
        let mut terms = vec![self.and_expr()?];
        while self.tokens.get(self.pos) == Some(&SpdxToken::Or) {
            self.pos += 1;
            terms.push(self.and_expr()?);
        }
        Ok(if terms.len() == 1 {
            terms.pop().unwrap()
        } else {
            LicenseExpr::Or(terms)
        })
    }

    fn and_expr(&mut self) -> Result<LicenseExpr, ExprParseError> {
        let mut terms = vec![self.with_expr()?];
        while self.tokens.get(self.pos) == Some(&SpdxToken::And) {
            self.pos += 1;
            terms.push(self.with_expr()?);
        }
        Ok(if terms.len() == 1 {
            terms.pop().unwrap()
        } else {
            LicenseExpr::And(terms)
        })
    }

    fn with_expr(&mut self) -> Result<LicenseExpr, ExprParseError> {
        let atom = self.atom()?;
        if self.tokens.get(self.pos) != Some(&SpdxToken::With) {
            return Ok(atom);
        }
        self.pos += 1;
        // SPDX: WITH applies to a license id only, not a parenthesised group.
        let LicenseExpr::Name(name) = atom else {
            return self.err("WITH must follow a license id");
        };
        match self.tokens.get(self.pos) {
            Some(SpdxToken::Id(exception)) => {
                self.pos += 1;
                Ok(LicenseExpr::WithException(name, exception.to_string()))
            }
            _ => self.err("expected exception id after WITH"),
        }
    }

    fn atom(&mut self) -> Result<LicenseExpr, ExprParseError> {
        match self.tokens.get(self.pos) {
            Some(SpdxToken::Id(id)) => {
                self.pos += 1;
                Ok(LicenseExpr::Name(id.to_string()))
            }
            Some(SpdxToken::Open) => {
                self.pos += 1;
                let expr = self.or_expr()?;
                if self.tokens.get(self.pos) != Some(&SpdxToken::Close) {
                    return self.err("expected ')'");
                }
                self.pos += 1;
                Ok(expr)
            }
            Some(_) => self.err("expected license id"),
            None => self.err("unexpected end of expression"),
        }
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
    fn test_parse_strict_valid() {
        for input in [
            "MIT",
            "GPL-2+ or MIT",
            "Apache-2.0 and BSD-3-clause",
            "GPL-2+ with OpenSSL-exception",
            "GPL-2+ with Autoconf exception",
            "A or B, and C",
            "A, and B or C, or D",
            "GPL-1+ or Artistic or Perl",
        ] {
            let strict = LicenseExpr::parse_strict(input)
                .unwrap_or_else(|e| panic!("strict parse failed for {input:?}: {e}"));
            assert_eq!(
                strict,
                LicenseExpr::parse(input),
                "tree mismatch for {input:?}"
            );
        }
    }

    #[test]
    fn test_parse_strict_rejects_malformed() {
        for input in [
            "",
            "   ",
            "Apache 2.0",
            "MIT or",
            "or MIT",
            "MIT or or GPL-2",
            "MIT with",
            "MIT, GPL-2",
            "A, B, and C",
            ",",
        ] {
            assert!(
                LicenseExpr::parse_strict(input).is_err(),
                "strict parse accepted {input:?}"
            );
        }
    }

    #[test]
    fn test_parse_spdx_simple() {
        assert_eq!(
            LicenseExpr::parse_spdx("MIT OR Apache-2.0").unwrap(),
            LicenseExpr::Or(vec![
                LicenseExpr::Name("MIT".into()),
                LicenseExpr::Name("Apache-2.0".into()),
            ])
        );
    }

    #[test]
    fn test_parse_spdx_case_insensitive_operators() {
        assert_eq!(
            LicenseExpr::parse_spdx("MIT or Apache-2.0").unwrap(),
            LicenseExpr::parse_spdx("MIT OR Apache-2.0").unwrap(),
        );
    }

    #[test]
    fn test_parse_spdx_with() {
        assert_eq!(
            LicenseExpr::parse_spdx("GPL-2.0-only WITH Classpath-exception-2.0").unwrap(),
            LicenseExpr::WithException("GPL-2.0-only".into(), "Classpath-exception-2.0".into())
        );
    }

    #[test]
    fn test_parse_spdx_parens_and_precedence() {
        // Parens override the AND-binds-tighter default.
        assert_eq!(
            LicenseExpr::parse_spdx("MIT AND (ISC OR GPL-2.0-only)").unwrap(),
            LicenseExpr::And(vec![
                LicenseExpr::Name("MIT".into()),
                LicenseExpr::Or(vec![
                    LicenseExpr::Name("ISC".into()),
                    LicenseExpr::Name("GPL-2.0-only".into()),
                ]),
            ])
        );
        assert_eq!(
            LicenseExpr::parse_spdx("MIT AND ISC OR GPL-2.0-only").unwrap(),
            LicenseExpr::Or(vec![
                LicenseExpr::And(vec![
                    LicenseExpr::Name("MIT".into()),
                    LicenseExpr::Name("ISC".into()),
                ]),
                LicenseExpr::Name("GPL-2.0-only".into()),
            ])
        );
    }

    #[test]
    fn test_parse_spdx_rejects_malformed() {
        for input in [
            "",
            "MIT OR",
            "(MIT",
            "MIT)",
            "MIT WITH",
            "(MIT AND ISC) WITH X",
        ] {
            assert!(
                LicenseExpr::parse_spdx(input).is_err(),
                "SPDX parse accepted {input:?}"
            );
        }
    }

    #[test]
    fn test_to_spdx_string_round_trip() {
        for input in [
            "MIT",
            "MIT OR Apache-2.0",
            "(MIT AND ISC) OR GPL-2.0-only",
            "GPL-2.0-only WITH Classpath-exception-2.0",
            "MIT AND (ISC OR GPL-2.0-only)",
        ] {
            let expr = LicenseExpr::parse_spdx(input).unwrap();
            assert_eq!(
                LicenseExpr::parse_spdx(&expr.to_spdx_string()).unwrap(),
                expr,
                "round trip failed for {input:?}"
            );
        }
    }

    #[allow(deprecated)]
    #[test]
    fn test_map_names() {
        let expr =
            LicenseExpr::parse_spdx("MIT OR GPL-2.0-only WITH Classpath-exception-2.0").unwrap();
        let mapped = expr.map_names(
            &|name| match name {
                "MIT" => "Expat".to_string(),
                "GPL-2.0-only" => "GPL-2".to_string(),
                other => other.to_string(),
            },
            &|exception| match exception {
                "Classpath-exception-2.0" => "ClassPath exception".to_string(),
                other => other.to_string(),
            },
        );
        assert_eq!(
            mapped.to_string(),
            "Expat or GPL-2 with ClassPath exception"
        );
    }

    #[test]
    fn test_map_leaves() {
        let expr =
            LicenseExpr::parse_spdx("MIT OR GPL-2.0-only WITH Classpath-exception-2.0").unwrap();
        let mapped = expr.map_leaves(|leaf| match leaf {
            LicenseExpr::Name(name) if name == "MIT" => LicenseExpr::Name("Expat".to_string()),
            LicenseExpr::WithException(name, exception)
                if name == "GPL-2.0-only" && exception == "Classpath-exception-2.0" =>
            {
                LicenseExpr::WithException("GPL-2".to_string(), "ClassPath exception".to_string())
            }
            other => other.clone(),
        });
        assert_eq!(
            mapped.to_string(),
            "Expat or GPL-2 with ClassPath exception"
        );
    }

    #[test]
    fn test_map_leaves_preserves_structure() {
        let expr = LicenseExpr::parse("A or B, and C");
        let mapped = expr.map_leaves(|leaf| leaf.clone());
        assert_eq!(mapped, expr);
    }

    #[test]
    fn test_map_leaves_sees_license_and_exception_together() {
        // A leaf whose license and exception together map to one DEP-5 name;
        // the deprecated map_names could not express this.
        let expr = LicenseExpr::parse_spdx("GPL-2.0-only WITH Font-exception-2.0").unwrap();
        let mapped = expr.map_leaves(|leaf| match leaf {
            LicenseExpr::WithException(name, exception)
                if name == "GPL-2.0-only" && exception == "Font-exception-2.0" =>
            {
                LicenseExpr::Name("GPL-2-with-font-exception".to_string())
            }
            other => other.clone(),
        });
        assert_eq!(
            mapped,
            LicenseExpr::Name("GPL-2-with-font-exception".into())
        );
    }

    #[test]
    fn test_from_str() {
        let expr: LicenseExpr = "GPL-2+ or MIT".parse().unwrap();
        assert_eq!(expr, LicenseExpr::parse_strict("GPL-2+ or MIT").unwrap());
        assert!("Apache 2.0".parse::<LicenseExpr>().is_err());
    }

    #[test]
    fn test_expr_parse_error_accessors() {
        let err = LicenseExpr::parse_strict("MIT or").unwrap_err();
        assert_eq!(err.input(), "MIT or");
        assert_eq!(err.message(), "unexpected end of expression");
        assert_eq!(
            err.to_string(),
            r#"unexpected end of expression in license expression "MIT or""#
        );
    }

    #[test]
    fn test_leaves() {
        let expr = LicenseExpr::parse("A and B with C exception or D");
        let leaves: Vec<String> = expr.leaves().iter().map(|l| l.to_string()).collect();
        assert_eq!(leaves, vec!["A", "B with C exception", "D"]);
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
