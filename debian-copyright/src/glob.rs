/// An invalid DEP-5 glob pattern.
///
/// The only way a pattern can be invalid is a backslash that does not escape
/// `*`, `?` or `\`, including a backslash at the end of the pattern.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlobError {
    pattern: String,
    /// The escaped character, or `None` for a trailing backslash.
    escape: Option<char>,
}

impl GlobError {
    /// The pattern that failed to compile.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

impl std::fmt::Display for GlobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.escape {
            Some(c) => write!(
                f,
                "invalid escape sequence \\{} in glob pattern {:?}",
                c, self.pattern
            ),
            None => write!(f, "trailing backslash in glob pattern {:?}", self.pattern),
        }
    }
}

impl std::error::Error for GlobError {}

/// A compiled DEP-5 glob pattern that can efficiently match many paths.
///
/// The pattern uses the glob syntax defined by the DEP-5 specification:
/// `*` matches any sequence of characters, `?` matches a single character,
/// and backslash escapes `*`, `?` and `\`.
///
/// # Examples
///
/// ```
/// let pat = debian_copyright::GlobPattern::try_new("src/*.rs").unwrap();
/// assert!(pat.is_match("src/main.rs"));
/// assert!(!pat.is_match("lib/main.rs"));
/// ```
#[derive(Clone, Debug)]
pub struct GlobPattern {
    regex: regex::Regex,
    pattern: String,
}

impl GlobPattern {
    /// Compile a DEP-5 glob pattern.
    ///
    /// # Panics
    ///
    /// Panics if the pattern contains an invalid escape sequence. Patterns
    /// read from copyright files are arbitrary input, so prefer
    /// [`try_new`](Self::try_new).
    #[deprecated(since = "0.1.55", note = "use GlobPattern::try_new instead")]
    pub fn new(pattern: &str) -> Self {
        Self::try_new(pattern).unwrap()
    }

    /// Compile a DEP-5 glob pattern, reporting an invalid escape sequence.
    ///
    /// # Examples
    ///
    /// ```
    /// let pat = debian_copyright::GlobPattern::try_new(r"\*.txt").unwrap();
    /// assert!(pat.is_match("*.txt"));
    /// assert!(debian_copyright::GlobPattern::try_new(r"\x").is_err());
    /// ```
    pub fn try_new(pattern: &str) -> Result<Self, GlobError> {
        Ok(Self {
            regex: glob_to_regex_checked(pattern)?,
            pattern: pattern.to_string(),
        })
    }

    /// Check whether a path matches this pattern.
    pub fn is_match(&self, path: &str) -> bool {
        self.regex.is_match(path)
    }

    /// Check whether a path matches this pattern.
    ///
    /// Returns `false` for a path that is not valid UTF-8, since DEP-5
    /// patterns cannot name one.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// let pat = debian_copyright::GlobPattern::try_new("src/*.rs").unwrap();
    /// assert!(pat.is_match_path(Path::new("src/main.rs")));
    /// ```
    pub fn is_match_path(&self, path: &std::path::Path) -> bool {
        path.to_str().is_some_and(|path| self.is_match(path))
    }

    /// The glob pattern this matcher was compiled from.
    ///
    /// # Examples
    ///
    /// ```
    /// let pat = debian_copyright::GlobPattern::try_new("src/*.rs").unwrap();
    /// assert_eq!(pat.pattern(), "src/*.rs");
    /// ```
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

impl std::fmt::Display for GlobPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.pattern)
    }
}

impl std::str::FromStr for GlobPattern {
    type Err = GlobError;

    fn from_str(pattern: &str) -> Result<Self, Self::Err> {
        Self::try_new(pattern)
    }
}

/// Whether any of `patterns` matches `path`.
///
/// A pattern that is not a valid glob, and a path that is not valid UTF-8,
/// match nothing: copyright files and filesystems both hand us arbitrary
/// input, and a match check is no place to fail. Callers that need to know a
/// pattern is broken should compile it with [`GlobPattern::try_new`].
pub(crate) fn matches_any<P: AsRef<str>>(patterns: &[P], path: &std::path::Path) -> bool {
    let Some(path) = path.to_str() else {
        return false;
    };
    patterns.iter().any(|pattern| {
        GlobPattern::try_new(pattern.as_ref()).is_ok_and(|pattern| pattern.is_match(path))
    })
}

/// Decode a DEP-5 file pattern that names a single literal path.
///
/// A pattern is literal when it has no unescaped `*` or `?` wildcard. Escapes
/// (`\*`, `\?`, `\\`) are resolved to the characters they stand for, so the
/// returned string is the actual filename the pattern designates. Patterns that
/// contain a wildcard (and therefore match many paths) return `None`.
///
/// # Examples
///
/// ```
/// assert_eq!(debian_copyright::glob::literal_path("src/main.rs").as_deref(), Some("src/main.rs"));
/// assert_eq!(debian_copyright::glob::literal_path(r"\*.txt").as_deref(), Some("*.txt"));
/// assert_eq!(debian_copyright::glob::literal_path("src/*.rs"), None);
/// ```
///
/// A malformed trailing or invalid escape returns `None` rather than panicking,
/// since callers decode arbitrary file contents.
pub fn literal_path(pattern: &str) -> Option<String> {
    let mut out = String::with_capacity(pattern.len());
    let mut it = pattern.chars();
    while let Some(c) = it.next() {
        match c {
            '*' | '?' => return None,
            '\\' => match it.next() {
                Some(esc @ ('*' | '?' | '\\')) => out.push(esc),
                _ => return None,
            },
            c => out.push(c),
        }
    }
    Some(out)
}

/// Whether a DEP-5 file pattern contains a wildcard, i.e. can match more than
/// one path. The inverse of [`literal_path`] returning `Some`.
pub fn is_glob(pattern: &str) -> bool {
    literal_path(pattern).is_none()
}

/// Convert a glob pattern to a regular expression.
#[deprecated(since = "0.1.46", note = "use GlobPattern instead")]
pub fn glob_to_regex(glob: &str) -> regex::Regex {
    glob_to_regex_checked(glob).unwrap()
}

fn glob_to_regex_checked(glob: &str) -> Result<regex::Regex, GlobError> {
    let err = |escape| GlobError {
        pattern: glob.to_string(),
        escape,
    };

    let mut it = glob.chars();
    let mut r = "^".to_string();

    while let Some(c) = it.next() {
        match c {
            '*' => r.push_str(".*"),
            '?' => r.push('.'),
            '\\' => match it.next() {
                Some(esc @ ('?' | '*' | '\\')) => r.push_str(&regex::escape(&esc.to_string())),
                Some(x) => return Err(err(Some(x))),
                None => return Err(err(None)),
            },
            c => r.push_str(&regex::escape(&c.to_string())),
        }
    }

    r.push('$');

    Ok(regex::Regex::new(r.as_str()).unwrap())
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    #[test]
    fn test_simple() {
        let r = super::glob_to_regex("*.rs");
        assert!(r.is_match("foo.rs"));
        assert!(r.is_match("bar.rs"));
        assert!(!r.is_match("foo.rs.bak"));
        assert!(!r.is_match("foo"));
    }

    #[test]
    fn test_single_char() {
        let r = super::glob_to_regex("?.rs");
        assert!(r.is_match("a.rs"));
        assert!(r.is_match("b.rs"));
        assert!(!r.is_match("foo.rs"));
        assert!(!r.is_match("foo"));
    }

    #[test]
    fn test_escape() {
        let r = super::glob_to_regex(r"\?.rs");
        assert!(r.is_match("?.rs"));
        assert!(!r.is_match("a.rs"));
        assert!(!r.is_match("b.rs"));

        let r = super::glob_to_regex(r"\*.rs");
        assert!(r.is_match("*.rs"));
        assert!(!r.is_match("a.rs"));
        assert!(!r.is_match("b.rs"));

        let r = super::glob_to_regex(r"\\?.rs");
        assert!(r.is_match("\\a.rs"));
        assert!(r.is_match("\\b.rs"));
        assert!(!r.is_match("a.rs"));
    }

    #[should_panic]
    #[test]
    fn test_invalid_escape() {
        super::glob_to_regex(r"\x.rs");
    }

    #[should_panic]
    #[test]
    fn test_invalid_escape2() {
        super::glob_to_regex(r"\");
    }

    #[test]
    fn test_glob_pattern_wildcard() {
        let pat = super::GlobPattern::new("src/*.rs");
        assert!(pat.is_match("src/main.rs"));
        assert!(pat.is_match("src/lib.rs"));
        assert!(!pat.is_match("lib/main.rs"));
        assert!(!pat.is_match("src/main.rs.bak"));
    }

    #[test]
    fn test_glob_pattern_deep_wildcard() {
        let pat = super::GlobPattern::new("src/*");
        assert!(pat.is_match("src/foo"));
        assert!(pat.is_match("src/foo/bar.rs"));
        assert!(!pat.is_match("lib/foo"));
    }

    #[test]
    fn test_glob_pattern_question_mark() {
        let pat = super::GlobPattern::new("file?.txt");
        assert!(pat.is_match("file1.txt"));
        assert!(pat.is_match("fileA.txt"));
        assert!(!pat.is_match("file10.txt"));
        assert!(!pat.is_match("file.txt"));
    }

    #[test]
    fn test_glob_pattern_literal() {
        let pat = super::GlobPattern::new("LICENSE");
        assert!(pat.is_match("LICENSE"));
        assert!(!pat.is_match("LICENSE.md"));
        assert!(!pat.is_match("NOLICENSE"));
    }

    #[test]
    fn test_glob_pattern_escaped_star() {
        let pat = super::GlobPattern::new(r"\*.txt");
        assert!(pat.is_match("*.txt"));
        assert!(!pat.is_match("foo.txt"));
    }

    #[test]
    fn test_glob_pattern_escaped_question() {
        let pat = super::GlobPattern::new(r"\?.txt");
        assert!(pat.is_match("?.txt"));
        assert!(!pat.is_match("a.txt"));
    }

    #[test]
    fn test_glob_pattern_escaped_backslash() {
        let pat = super::GlobPattern::new(r"\\foo");
        assert!(pat.is_match(r"\foo"));
        assert!(!pat.is_match("foo"));
    }

    #[should_panic]
    #[test]
    fn test_glob_pattern_invalid_escape() {
        super::GlobPattern::new(r"\x");
    }

    #[should_panic]
    #[test]
    fn test_glob_pattern_trailing_backslash() {
        super::GlobPattern::new(r"\");
    }

    #[test]
    fn test_glob_pattern_regex_special_chars() {
        let pat = super::GlobPattern::new("file(1).txt");
        assert!(pat.is_match("file(1).txt"));
        assert!(!pat.is_match("file1.txt"));
    }

    #[test]
    fn test_try_new_valid() {
        let pat = super::GlobPattern::try_new(r"src/\*.rs").unwrap();
        assert!(pat.is_match("src/*.rs"));
        assert!(!pat.is_match("src/main.rs"));
    }

    #[test]
    fn test_try_new_invalid_escape() {
        let err = super::GlobPattern::try_new(r"foo\x").unwrap_err();
        assert_eq!(err.pattern(), r"foo\x");
        assert_eq!(
            err.to_string(),
            r#"invalid escape sequence \x in glob pattern "foo\\x""#
        );
    }

    #[test]
    fn test_try_new_trailing_backslash() {
        let err = super::GlobPattern::try_new(r"foo\").unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"trailing backslash in glob pattern "foo\\""#
        );
    }

    #[test]
    fn test_from_str() {
        let pat: super::GlobPattern = "src/*.rs".parse().unwrap();
        assert!(pat.is_match("src/main.rs"));
        assert!(r"\x".parse::<super::GlobPattern>().is_err());
    }

    #[test]
    fn test_display() {
        let pat = super::GlobPattern::try_new("src/*.rs").unwrap();
        assert_eq!(pat.to_string(), "src/*.rs");
    }

    #[test]
    fn test_is_match_path() {
        let pat = super::GlobPattern::try_new("src/*.rs").unwrap();
        assert!(pat.is_match_path(std::path::Path::new("src/main.rs")));
        assert!(!pat.is_match_path(std::path::Path::new("lib/main.rs")));
    }

    #[cfg(unix)]
    #[test]
    fn test_is_match_path_non_utf8() {
        use std::os::unix::ffi::OsStrExt;
        let path = std::path::Path::new(std::ffi::OsStr::from_bytes(b"src/\xff.rs"));
        let pat = super::GlobPattern::try_new("src/*").unwrap();
        assert!(!pat.is_match_path(path));
    }

    #[test]
    fn test_literal_path_plain() {
        assert_eq!(
            super::literal_path("src/main.rs").as_deref(),
            Some("src/main.rs")
        );
        assert_eq!(super::literal_path("LICENSE").as_deref(), Some("LICENSE"));
    }

    #[test]
    fn test_literal_path_unescapes() {
        assert_eq!(super::literal_path(r"\*.txt").as_deref(), Some("*.txt"));
        assert_eq!(super::literal_path(r"\?.txt").as_deref(), Some("?.txt"));
        assert_eq!(super::literal_path(r"a\\b").as_deref(), Some(r"a\b"));
    }

    #[test]
    fn test_literal_path_rejects_globs() {
        assert_eq!(super::literal_path("src/*.rs"), None);
        assert_eq!(super::literal_path("file?.txt"), None);
    }

    #[test]
    fn test_literal_path_rejects_invalid_escape() {
        // A trailing or invalid escape is not a valid literal; return None
        // rather than panicking on arbitrary input.
        assert_eq!(super::literal_path(r"foo\"), None);
        assert_eq!(super::literal_path(r"foo\x"), None);
    }

    #[test]
    fn test_is_glob() {
        assert!(super::is_glob("src/*.rs"));
        assert!(super::is_glob("file?.txt"));
        assert!(!super::is_glob("src/main.rs"));
        assert!(!super::is_glob(r"\*.txt"));
    }
}
