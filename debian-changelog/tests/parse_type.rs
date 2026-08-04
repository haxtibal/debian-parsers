use debian_changelog::{ChangeLog, Parse, Urgency};

#[test]
fn test_parse_clone() {
    let changelog_text = r#"test (1.0.0) unstable; urgency=low

  * Initial release.

 -- Test User <test@example.com>  Mon, 04 Sep 2023 18:13:45 -0500
"#;

    let parsed: Parse<ChangeLog> = ChangeLog::parse(changelog_text);
    let cloned = parsed.clone();

    // Verify that clone creates an equal object
    assert_eq!(parsed, cloned);

    // Verify they have the same content
    assert_eq!(parsed.green(), cloned.green());
    assert_eq!(parsed.errors(), cloned.errors());
}

#[test]
fn test_parse_partial_eq() {
    let changelog1 = r#"test (1.0.0) unstable; urgency=low

  * Initial release.

 -- Test User <test@example.com>  Mon, 04 Sep 2023 18:13:45 -0500
"#;

    let changelog2 = r#"test (2.0.0) unstable; urgency=low

  * New version.

 -- Test User <test@example.com>  Mon, 04 Sep 2023 18:13:45 -0500
"#;

    let parsed1 = ChangeLog::parse(changelog1);
    let parsed2 = ChangeLog::parse(changelog2);
    let parsed1_clone = parsed1.clone();

    // Same content should be equal
    assert_eq!(parsed1, parsed1_clone);

    // Different content should not be equal
    assert_ne!(parsed1, parsed2);
}

#[test]
fn test_parse_with_errors() {
    // Parse some invalid changelog
    let invalid_text = "this is not a valid changelog";
    let parsed = ChangeLog::parse(invalid_text);

    // Should have errors
    assert!(!parsed.ok());
    assert!(!parsed.errors().is_empty());

    // Clone should preserve errors
    let cloned = parsed.clone();
    assert_eq!(parsed.errors(), cloned.errors());
    assert_eq!(parsed, cloned);
}

#[test]
fn test_parse_errors_accessor() {
    let invalid_text = "INVALID";
    let parsed = ChangeLog::parse(invalid_text);

    // Access errors
    let errors = parsed.errors();
    assert!(!errors.is_empty());
    assert!(errors[0].contains("expected") || errors[0].contains("VERSION"));
}

#[test]
fn test_parse_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Parse<ChangeLog>>();
}

#[test]
fn test_parse_to_result_with_errors() {
    let invalid_text = "INVALID CHANGELOG";
    let parsed = ChangeLog::parse(invalid_text);

    // to_result should return Err when there are errors
    let result = parsed.to_result();
    assert!(result.is_err());

    match result {
        Err(_) => {
            // Expected error
        }
        Ok(_) => panic!("Expected error but got Ok"),
    }
}

#[test]
fn test_parse_to_mut_result_with_errors() {
    let invalid_text = "INVALID CHANGELOG";
    let parsed = ChangeLog::parse(invalid_text);

    // to_mut_result should return Err when there are errors
    let result = parsed.to_mut_result();
    assert!(result.is_err());

    match result {
        Err(_) => {
            // Expected error
        }
        Ok(_) => panic!("Expected error but got Ok"),
    }
}

#[test]
fn test_parse_tree_mut() {
    let changelog_text = r#"test (1.0.0) unstable; urgency=low

  * Initial release.

 -- Test User <test@example.com>  Mon, 04 Sep 2023 18:13:45 -0500
"#;

    let parsed = ChangeLog::parse(changelog_text);
    let tree = parsed.tree_mut();

    // Should be able to get a mutable tree
    assert_eq!(tree.iter().count(), 1);

    // Verify the content
    let entry = tree.iter().next().unwrap();
    assert_eq!(entry.package(), Some("test".to_string()));
    assert_eq!(entry.version().unwrap().to_string(), "1.0.0");
}

#[test]
fn test_parse_tree_with_errors_returns_partial_tree() {
    let invalid_text = "INVALID";
    let parsed = ChangeLog::parse(invalid_text);

    assert!(!parsed.errors().is_empty());
    // tree() should still return a (partial) tree without panicking
    let _tree = parsed.tree();
}

#[test]
fn test_parse_tree_mut_with_errors_returns_partial_tree() {
    let invalid_text = "INVALID";
    let parsed = ChangeLog::parse(invalid_text);

    assert!(!parsed.errors().is_empty());
    // tree_mut() should still return a (partial) tree without panicking
    let _tree = parsed.tree_mut();
}

#[test]
fn test_parse_equality_with_same_errors() {
    // Two parses of the same invalid input should be equal
    let invalid_text = "INVALID CHANGELOG";
    let parsed1 = ChangeLog::parse(invalid_text);
    let parsed2 = ChangeLog::parse(invalid_text);

    assert_eq!(parsed1, parsed2);
}

#[test]
fn test_parse_inequality_different_errors() {
    // Different invalid inputs should produce different Parse objects
    let invalid1 = "INVALID1";
    let invalid2 = "INVALID2 (different)";

    let parsed1 = ChangeLog::parse(invalid1);
    let parsed2 = ChangeLog::parse(invalid2);

    // They should not be equal because they have different green nodes
    assert_ne!(parsed1, parsed2);
}

#[test]
fn test_parse_empty_string() {
    let parsed = ChangeLog::parse("");
    assert!(parsed.errors().is_empty());
    let tree = parsed.tree();
    assert_eq!(tree.iter().count(), 0);
}

#[test]
fn test_parse_relaxed_non_panicking() {
    let cl = ChangeLog::parse_relaxed("INVALID");
    // "INVALID" might be parsed as an identifier for an entry
    let _ = cl.iter().count();
}

#[test]
fn test_invalid_version_no_panic() {
    // Test with an invalid version string that should not panic
    let changelog_text = r#"test (2.0.37+cvs.JCW_PRE2_2037-1) unstable; urgency=low

  * Initial release.

 -- Test User <test@example.com>  Mon, 04 Sep 2023 18:13:45 -0500
"#;

    let parsed = ChangeLog::parse(changelog_text);

    // If parsing fails, that's okay - just shouldn't panic
    if !parsed.ok() {
        // Expected to have errors with relaxed parsing
        assert!(!parsed.errors().is_empty());
    } else {
        // If it parses successfully, accessing the entry should also not panic
        if let Some(entry) = parsed.tree().iter().next() {
            // Accessing version should not panic - this is the critical test
            let version_result = entry.version();
            assert_eq!(version_result, None, "Invalid version should return None");

            // try_version should return Some(Err(...)) for invalid version strings
            let try_result = entry.try_version();
            match try_result {
                Some(Err(err)) => {
                    // Expected: version token exists but parsing failed
                    assert!(
                        err.to_string().contains("Invalid version string")
                            || err.to_string().contains("2.0.37+cvs.JCW_PRE2_2037-1"),
                        "Error should mention invalid version: {}",
                        err
                    );
                }
                Some(Ok(_)) => {
                    panic!("Expected parsing to fail for invalid version string");
                }
                None => {
                    panic!("Expected Some(Err(...)) because version token exists but is invalid");
                }
            }
        }
    }
}

/// A changelog header carrying an urgency annotation, as used by dpkg and
/// cpp-11. See https://github.com/jelmer/debian-parsers/issues/466.
const URGENCY_ANNOTATION: &str = "dpkg (1.4.0) unstable; urgency=low (HIGH for new source format)

  * Fix something.

 -- Ian Jackson <ijackson@nyx.cs.du.edu>  Thu, 12 Sep 1996 01:13:33 +0100
";

#[test]
fn test_urgency_annotation_strict_reports_single_error() {
    let parsed = ChangeLog::parse(URGENCY_ANNOTATION);

    assert_eq!(
        parsed.errors(),
        vec!["unexpected text after metadata value"]
    );

    let offset = parsed.errors_with_offsets()[0].1;
    let offset = usize::try_from(u32::from(offset)).unwrap();
    assert_eq!(
        &URGENCY_ANNOTATION[offset..offset + "(HIGH for new source format)".len()],
        "(HIGH for new source format)"
    );
}

#[test]
fn test_urgency_annotation_relaxed_keeps_entry_intact() {
    let cl = ChangeLog::parse_relaxed(URGENCY_ANNOTATION);

    assert_eq!(cl.iter().count(), 1);
    assert_eq!(cl.to_string(), URGENCY_ANNOTATION);

    let entry = cl.iter().next().unwrap();
    assert_eq!(entry.package(), Some("dpkg".to_string()));
    assert_eq!(entry.distributions(), Some(vec!["unstable".to_string()]));
    assert_eq!(entry.urgency(), Some(Urgency::Low));
    assert_eq!(
        entry.change_lines().collect::<Vec<_>>(),
        vec!["* Fix something.".to_string()]
    );
    assert_eq!(entry.maintainer(), Some("Ian Jackson".to_string()));
    assert_eq!(entry.email(), Some("ijackson@nyx.cs.du.edu".to_string()));
}

#[test]
fn test_unparsable_urgency_returns_none() {
    let text = "dpkg (1.4.0) unstable; urgency=bogus

  * Fix something.

 -- Ian Jackson <ijackson@nyx.cs.du.edu>  Thu, 12 Sep 1996 01:13:33 +0100
";
    let cl = ChangeLog::parse_relaxed(text);
    let entry = cl.iter().next().unwrap();
    assert_eq!(entry.urgency(), None);
}

/// Older dpkg entries annotate `priority` without parentheses; recovery should
/// not split these into a bogus second entry either.
#[test]
fn test_unparenthesized_annotation_keeps_entry_intact() {
    for text in [
        "dpkg (0.93.67) BETA; priority=LOW for C dpkg alpha testers, HIGH for others

  * Fix something.

 -- Ian Jackson <ijackson@nyx.cs.du.edu>  Thu, 12 Sep 1996 01:13:33 +0100
",
        "dpkg (0.93.42) BETA; priority=LOW; HIGH for dselect users

  * Fix something.

 -- Ian Jackson <ijackson@nyx.cs.du.edu>  Thu, 12 Sep 1996 01:13:33 +0100
",
    ] {
        let cl = ChangeLog::parse_relaxed(text);
        assert_eq!(cl.iter().count(), 1);
        assert_eq!(cl.to_string(), text);

        let entry = cl.iter().next().unwrap();
        assert_eq!(entry.package(), Some("dpkg".to_string()));
        assert_eq!(entry.distributions(), Some(vec!["BETA".to_string()]));
        assert_eq!(
            entry.change_lines().collect::<Vec<_>>(),
            vec!["* Fix something.".to_string()]
        );
        assert_eq!(entry.maintainer(), Some("Ian Jackson".to_string()));
    }
}
