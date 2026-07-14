//! A library for parsing and manipulating debian/copyright files that
//! use the DEP-5 format.
//!
//! # Examples
//!
//! ```rust
//!
//! use debian_copyright::Copyright;
//! use std::path::Path;
//!
//! let text = r#"Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
//! Upstream-Author: John Doe <john@example>
//! Upstream-Name: example
//! Source: https://example.com/example
//!
//! Files: *
//! License: GPL-3+
//! Copyright: 2019 John Doe
//!
//! Files: debian/*
//! License: GPL-3+
//! Copyright: 2019 Jane Packager
//!
//! License: GPL-3+
//!  This program is free software: you can redistribute it and/or modify
//!  it under the terms of the GNU General Public License as published by
//!  the Free Software Foundation, either version 3 of the License, or
//!  (at your option) any later version.
//! "#;
//!
//! let c = text.parse::<Copyright>().unwrap();
//! let license = c.find_license_for_file(Path::new("debian/foo")).unwrap();
//! assert_eq!(license.name(), Some("GPL-3+"));
//! ```

use crate::License;
use crate::CURRENT_FORMAT;
use deb822_fast::{Deb822, FromDeb822, FromDeb822Paragraph, ToDeb822, ToDeb822Paragraph};
use std::path::Path;

// File patterns are whitespace-separated and may share a line (patterns
// cannot contain whitespace; the escape syntax only covers *, ? and \).
fn deserialize_file_list(text: &str) -> Result<Vec<String>, String> {
    Ok(text.split_whitespace().map(|x| x.to_string()).collect())
}

fn serialize_file_list(files: &[String]) -> String {
    files.join("\n")
}

/// A header paragraph.
#[derive(FromDeb822, ToDeb822, Clone, PartialEq, Eq, Debug)]
pub struct Header {
    #[deb822(field = "Format")]
    /// The format of the file.
    format: String,

    #[deb822(field = "Files-Excluded", deserialize_with = deserialize_file_list, serialize_with = serialize_file_list)]
    /// Files that are excluded from the copyright information, and should be excluded from the package.
    files_excluded: Option<Vec<String>>,

    #[deb822(field = "Source")]
    /// The source of the package.
    source: Option<String>,

    #[deb822(field = "Upstream-Contact")]
    /// Contact information for the upstream author.
    upstream_contact: Option<String>,
}

impl Default for Header {
    fn default() -> Self {
        Header {
            format: CURRENT_FORMAT.to_string(),
            files_excluded: None,
            source: None,
            upstream_contact: None,
        }
    }
}

impl std::fmt::Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let para: deb822_fast::Paragraph = self.to_paragraph();
        write!(f, "{}", para)?;
        Ok(())
    }
}

/// A copyright file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Copyright {
    /// The header paragraph.
    pub header: Header,

    /// Files paragraphs.
    pub files: Vec<FilesParagraph>,

    /// License paragraphs.
    pub licenses: Vec<LicenseParagraph>,
}

impl std::str::FromStr for Copyright {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with("Format:") {
            return Err("Not machine readable".to_string());
        }

        let deb822: Deb822 = s.parse().map_err(|e: deb822_fast::Error| e.to_string())?;

        let mut paragraphs = deb822.iter();

        let first_para = if let Some(para) = paragraphs.next() {
            para
        } else {
            return Err("No paragraphs".to_string());
        };

        let header: Header = Header::from_paragraph(first_para)?;

        let mut files_paras = vec![];
        let mut license_paras = vec![];

        for para in paragraphs {
            if para.get("Files").is_some() {
                files_paras.push(FilesParagraph::from_paragraph(para)?);
            } else if para.get("License").is_some() {
                license_paras.push(LicenseParagraph::from_paragraph(para)?);
            } else {
                return Err("Paragraph is neither License nor Files".to_string());
            }
        }

        Ok(Copyright {
            header,
            files: files_paras,
            licenses: license_paras,
        })
    }
}

/// A paragraph describing a license.
#[derive(FromDeb822, ToDeb822, Clone, PartialEq, Eq, Debug)]
pub struct LicenseParagraph {
    /// The license text.
    #[deb822(field = "License")]
    license: License,

    /// A comment.
    #[deb822(field = "Comment")]
    comment: Option<String>,
}

impl std::fmt::Display for LicenseParagraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let para: deb822_fast::Paragraph = self.to_paragraph();
        f.write_str(&para.to_string())
    }
}

fn deserialize_copyrights(text: &str) -> Result<Vec<String>, String> {
    Ok(text.split('\n').map(ToString::to_string).collect())
}

fn serialize_copyrights(copyrights: &[String]) -> String {
    copyrights.join("\n")
}

/// A paragraph describing a set of files.
#[derive(FromDeb822, ToDeb822, Clone, PartialEq, Eq, Debug)]
pub struct FilesParagraph {
    #[deb822(field="Files", deserialize_with = deserialize_file_list, serialize_with = serialize_file_list)]
    files: Vec<String>,
    #[deb822(field = "License")]
    license: License,
    #[deb822(field="Copyright", deserialize_with = deserialize_copyrights, serialize_with = serialize_copyrights)]
    copyright: Vec<String>,
    #[deb822(field = "Comment")]
    comment: Option<String>,
}

impl FilesParagraph {
    /// Check if the given filename matches one of the file patterns in this paragraph.
    ///
    /// A pattern that is not a valid glob matches nothing; use
    /// [`try_compiled_patterns`](Self::try_compiled_patterns) to detect one.
    pub fn matches(&self, filename: &std::path::Path) -> bool {
        crate::glob::matches_any(&self.files, filename)
    }

    /// The paragraph's Files patterns, compiled for matching.
    ///
    /// # Panics
    ///
    /// Panics if a pattern is not a valid glob. Copyright files are arbitrary
    /// input, so prefer [`try_compiled_patterns`](Self::try_compiled_patterns).
    #[deprecated(since = "0.1.55", note = "use try_compiled_patterns instead")]
    pub fn compiled_patterns(&self) -> Vec<crate::GlobPattern> {
        self.try_compiled_patterns().unwrap()
    }

    /// The paragraph's Files patterns, compiled for matching.
    ///
    /// Compiling a pattern is not free, so callers that match many paths
    /// against the same paragraph should call this once and reuse the result
    /// rather than calling [`matches`](Self::matches) per path. Each
    /// [`crate::GlobPattern`] reports the pattern it was compiled from, so a
    /// caller can tell which Files entry claimed a path.
    ///
    /// Returns an error if a pattern is not a valid glob.
    pub fn try_compiled_patterns(&self) -> Result<Vec<crate::GlobPattern>, crate::glob::GlobError> {
        self.files
            .iter()
            .map(|f| crate::GlobPattern::try_new(f))
            .collect()
    }
}

impl std::fmt::Display for FilesParagraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let para: deb822_fast::Paragraph = self.to_paragraph();
        f.write_str(&para.to_string())?;
        Ok(())
    }
}

impl Default for Copyright {
    fn default() -> Self {
        Self::new()
    }
}

impl Copyright {
    /// Create a new empty `Copyright` object.
    pub fn new() -> Self {
        Self {
            header: Header::default(),
            licenses: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Find the files paragraph that matches the given path.
    ///
    /// Returns `None` if no matching files paragraph is found.
    ///
    /// Every lookup recompiles the file patterns; to look up many paths
    /// against the same `Copyright`, build a [`FileMatcher`] once with
    /// [`matcher`](Self::matcher) instead.
    ///
    /// # Arguments
    /// * `path` - The path to the file to find the license for.
    pub fn find_files(&self, path: &std::path::Path) -> Option<&FilesParagraph> {
        self.files.iter().rfind(|f| f.matches(path))
    }

    /// Compile the file patterns once, for looking up many paths.
    ///
    /// Returns an error if any Files paragraph has a pattern that is not a
    /// valid glob. [`find_files`](Self::find_files) instead treats such a
    /// pattern as matching nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::Copyright;
    /// use std::path::Path;
    ///
    /// let text = "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n\
    ///             \n\
    ///             Files: *\n\
    ///             License: GPL-3+\n\
    ///             Copyright: 2019 John Doe\n";
    /// let copyright: Copyright = text.parse().unwrap();
    /// let matcher = copyright.matcher().unwrap();
    /// for path in ["src/main.rs", "README"] {
    ///     assert!(matcher.find_files(Path::new(path)).is_some());
    /// }
    /// ```
    pub fn matcher(&self) -> Result<FileMatcher<'_>, crate::glob::GlobError> {
        let paragraphs = self
            .files
            .iter()
            .map(|p| Ok((p, p.try_compiled_patterns()?)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(FileMatcher { paragraphs })
    }

    /// Returns the license for the given file.
    pub fn find_license_for_file(&self, filename: &Path) -> Option<&License> {
        let files = self.find_files(filename)?;
        if files.license.text().is_some() {
            return Some(&files.license);
        }
        self.find_license_by_name(files.license.name()?)
    }

    /// Find a license by name.
    ///
    /// Returns `None` if no license with the given name is found.
    ///
    /// # Arguments
    /// * `name` - The name of the license to find.
    pub fn find_license_by_name(&self, name: &str) -> Option<&License> {
        self.licenses
            .iter()
            .find(|p| p.license.name() == Some(name))
            .map(|p| &p.license)
    }
}

/// A [`Copyright`]'s file patterns, compiled once for looking up many paths.
///
/// Built with [`Copyright::matcher`]. Unlike [`Copyright::find_files`], which
/// recompiles every pattern on every call, this compiles each pattern once and
/// reuses it, and reports which pattern claimed a path.
pub struct FileMatcher<'a> {
    paragraphs: Vec<(&'a FilesParagraph, Vec<crate::GlobPattern>)>,
}

impl<'a> FileMatcher<'a> {
    /// Find the files paragraph that matches the given path.
    ///
    /// Consistent with the specification, this returns the last paragraph that
    /// matches, which is the most specific one.
    pub fn find_files(&self, path: &Path) -> Option<&'a FilesParagraph> {
        self.find_match(path).map(|(paragraph, _)| paragraph)
    }

    /// Find the files paragraph that matches the given path, along with the
    /// pattern that claimed it.
    ///
    /// # Examples
    ///
    /// ```
    /// use debian_copyright::Copyright;
    /// use std::path::Path;
    ///
    /// let text = "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n\
    ///             \n\
    ///             Files: *\n\
    ///             License: GPL-3+\n\
    ///             Copyright: 2019 John Doe\n\
    ///             \n\
    ///             Files: debian/*\n\
    ///             License: GPL-3+\n\
    ///             Copyright: 2019 Jane Packager\n";
    /// let copyright: Copyright = text.parse().unwrap();
    /// let matcher = copyright.matcher().unwrap();
    /// let (_, pattern) = matcher.find_match(Path::new("debian/rules")).unwrap();
    /// assert_eq!(pattern.pattern(), "debian/*");
    /// ```
    pub fn find_match(&self, path: &Path) -> Option<(&'a FilesParagraph, &crate::GlobPattern)> {
        let path = path.to_str()?;
        self.paragraphs.iter().rev().find_map(|(paragraph, globs)| {
            let glob = globs.iter().find(|glob| glob.is_match(path))?;
            Some((*paragraph, glob))
        })
    }
}

impl std::fmt::Display for Copyright {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.header)?;
        for files in &self.files {
            writeln!(f)?;
            write!(f, "{}", files)?;
        }
        for license in &self.licenses {
            writeln!(f)?;
            write!(f, "{}", license)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_not_machine_readable() {
        let s = r#"
This copyright file is not machine readable.
"#;
        let ret = s.parse::<super::Copyright>();
        assert!(ret.is_err());
        assert_eq!(ret.unwrap_err(), "Not machine readable".to_string());
    }

    #[test]
    fn test_new() {
        let n = super::Copyright::new();
        assert_eq!(
            n.to_string().as_str(),
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n"
        );
    }

    #[test]
    fn test_parse() {
        let s = r#"Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: foo
Upstream-Contact: Joe Bloggs <joe@example.com>
Source: https://example.com/foo

Files: *
Copyright:
  2020 Joe Bloggs <joe@example.com>
License: GPL-3+

Files: debian/*
Comment: Debian packaging is licensed under the GPL-3+.
Copyright: 2023 Jelmer Vernooij
License: GPL-3+

License: GPL-3+
 This program is free software: you can redistribute it and/or modify
 it under the terms of the GNU General Public License as published by
 the Free Software Foundation, either version 3 of the License, or
 (at your option) any later version.
"#;
        let copyright = s.parse::<super::Copyright>().expect("failed to parse");

        assert_eq!(
            "https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/",
            copyright.header.format
        );
        assert_eq!(
            "Joe Bloggs <joe@example.com>",
            copyright.header.upstream_contact.as_ref().unwrap()
        );
        assert_eq!(
            "https://example.com/foo",
            copyright.header.source.as_ref().unwrap()
        );

        let files = &copyright.files;
        assert_eq!(2, files.len());
        assert_eq!("*", files[0].files.join(" "));
        assert_eq!("debian/*", files[1].files.join(" "));
        assert_eq!(
            "Debian packaging is licensed under the GPL-3+.",
            files[1].comment.as_ref().unwrap()
        );
        assert_eq!(vec!["2023 Jelmer Vernooij".to_string()], files[1].copyright);
        assert_eq!("GPL-3+", files[1].license.name().unwrap());
        assert_eq!(files[1].license.text(), None);

        let licenses = &copyright.licenses;
        assert_eq!(1, licenses.len());
        assert_eq!("GPL-3+", licenses[0].license.name().unwrap());
        assert_eq!(
            "This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.",
            licenses[0].license.text().unwrap()
        );

        let upstream_files = copyright.find_files(std::path::Path::new("foo.c")).unwrap();
        assert_eq!(vec!["*"], upstream_files.files);

        let debian_files = copyright
            .find_files(std::path::Path::new("debian/foo.c"))
            .unwrap();
        assert_eq!(vec!["debian/*"], debian_files.files);

        let gpl = copyright.find_license_by_name("GPL-3+");
        assert!(gpl.is_some());

        let gpl = copyright.find_license_for_file(std::path::Path::new("debian/foo.c"));
        assert_eq!(gpl.unwrap().name().unwrap(), "GPL-3+");
    }

    #[test]
    fn test_matches_invalid_pattern() {
        // An invalid escape in Files used to panic; it now matches nothing.
        let s = r#"Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/

Files: src\x
License: GPL-3+
Copyright: 2019 John Doe
"#;
        let copyright = s.parse::<super::Copyright>().expect("failed to parse");
        assert!(!copyright.files[0].matches(std::path::Path::new(r"src\x")));
        assert_eq!(
            copyright.files[0]
                .try_compiled_patterns()
                .unwrap_err()
                .pattern(),
            r"src\x",
        );
        assert!(copyright
            .find_files(std::path::Path::new(r"src\x"))
            .is_none());
        assert!(copyright.matcher().is_err());
    }

    #[test]
    fn test_matcher() {
        let s = r#"Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/

Files: *
License: GPL-3+
Copyright: 2019 John Doe

Files: debian/*
License: GPL-2+
Copyright: 2019 Jane Packager
"#;
        let copyright = s.parse::<super::Copyright>().expect("failed to parse");
        let matcher = copyright.matcher().unwrap();

        let (paragraph, pattern) = matcher
            .find_match(std::path::Path::new("debian/rules"))
            .unwrap();
        assert_eq!(pattern.pattern(), "debian/*");
        assert_eq!(paragraph.license.name(), Some("GPL-2+"));

        let (paragraph, pattern) = matcher
            .find_match(std::path::Path::new("src/foo.c"))
            .unwrap();
        assert_eq!(pattern.pattern(), "*");
        assert_eq!(paragraph.license.name(), Some("GPL-3+"));
    }

    #[test]
    fn test_matcher_agrees_with_find_files() {
        let s = r#"Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/

Files: *
License: GPL-3+
Copyright: 2019 John Doe

Files: debian/*
License: GPL-2+
Copyright: 2019 Jane Packager
"#;
        let copyright = s.parse::<super::Copyright>().expect("failed to parse");
        let matcher = copyright.matcher().unwrap();
        for path in ["src/foo.c", "debian/rules", "debian/patches/series"] {
            let path = std::path::Path::new(path);
            assert_eq!(matcher.find_files(path), copyright.find_files(path));
        }
    }
}
