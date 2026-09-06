// Archived historical repository-hygiene checks.
// These checks target the pre-0.4.4 archive layout and are not part of the
// active Rust quality gate. Re-enable after the archive policy is reconciled.

use std::{fs, path::Path};

#[test]
fn markdown_links_resolve() {
    let exceptions = [
        "AGENTS.md",
        "docs/language/0.1/0.1.md::../project/usage.md",
        "docs/language/0.1/0.1.md::diagnostics.md",
        "docs/language/0.1/0.1.md::../library/temporal.md",
        "docs/language/0.1/0.1.md::../library/math.md",
        "docs/language/0.1/0.1.md::../library/host.md",
    ];
    let mut failures = Vec::new();
    for root in ["docs", "ongoing", "done", "todo"] {
        walk_markdown(Path::new(root), &mut |path, text| {
            check_links(path, text, &exceptions, &mut failures);
        });
    }
    for name in [
        "README.md",
        "CONTRIBUTING.md",
        "PHILOSOPHY.md",
        "GOVERNANCE.md",
    ] {
        if Path::new(name).exists() {
            let text = read(name);
            check_links(Path::new(name), &text, &exceptions, &mut failures);
        }
    }
    assert!(
        failures.is_empty(),
        "broken Markdown links:\n{}",
        failures.join("\n")
    );
}

#[test]
fn done_tree_is_closed_and_todo_accepted_docs_are_pointers() {
    let mut failures = Vec::new();
    walk_markdown(Path::new("done"), &mut |path, text| {
        if text.contains("Status: Open") {
            failures.push(format!("{} still says Status: Open", path.display()));
        }
        if text.contains("- [ ]") {
            failures.push(format!("{} has an unchecked gate", path.display()));
        }
        if text.lines().any(|line| line.contains("TODO:")) {
            failures.push(format!("{} contains TODO:", path.display()));
        }
    });
    walk_markdown(Path::new("todo/proposals"), &mut |path, text| {
        if path.file_name().is_some_and(|name| name == "README.md") {
            return;
        }
        let accepted = text.contains("Accepted into") || text.contains("Accepted for");
        let pointer = text.contains("Historical proposal")
            || text.contains("This file is a pointer")
            || text.contains("the rest of this document remains proposed")
            || text.contains("The rest of this document remains proposed")
            || text.contains("Exploratory")
            || text.contains("unresolved");
        if accepted && !pointer {
            failures.push(format!(
                "{} is marked accepted under todo/ without remaining scope",
                path.display()
            ));
        }
    });
    assert!(
        failures.is_empty(),
        "workflow location failures:\n{}",
        failures.join("\n")
    );
}


