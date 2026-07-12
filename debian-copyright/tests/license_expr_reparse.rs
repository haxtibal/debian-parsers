//! Exhaustively check that parsing the Display output of a parsed license
//! expression yields the same expression, over all short combinations of
//! names, operators, and stray commas.
//!
//! Expressions with empty names (from dangling operators, e.g. `or and`)
//! are excluded: an empty name renders as an empty string, so it cannot
//! survive a round trip.

use debian_copyright::LicenseExpr;

fn has_empty_name(expr: &LicenseExpr) -> bool {
    match expr {
        LicenseExpr::Name(n) => n.is_empty(),
        LicenseExpr::WithException(n, e) => n.is_empty() || e.is_empty(),
        LicenseExpr::And(exprs) | LicenseExpr::Or(exprs) => exprs.iter().any(has_empty_name),
    }
}

#[test]
fn reparse_is_stable() {
    let alphabet = ["A", "B2.0", "X Y", "and", "or", "with", ",", "Z,"];
    let mut inputs: Vec<String> = alphabet.iter().map(|t| t.to_string()).collect();
    let mut frontier = inputs.clone();
    for _ in 0..3 {
        let mut next = Vec::new();
        for prefix in &frontier {
            for tok in &alphabet {
                next.push(format!("{} {}", prefix, tok));
            }
        }
        inputs.extend(next.iter().cloned());
        frontier = next;
    }
    let mut checked = 0;
    for input in &inputs {
        let parsed = LicenseExpr::parse(input);
        if has_empty_name(&parsed) {
            continue;
        }
        let rendered = parsed.to_string();
        let reparsed = LicenseExpr::parse(&rendered);
        assert_eq!(
            reparsed, parsed,
            "not stable for input {input:?}: rendered as {rendered:?}"
        );
        checked += 1;
    }
    assert!(checked > 1000, "only {checked} inputs exercised");
}
