//! Knowledge-unit chunking: split a markdown knowledge document into a tree of
//! self-contained Knowledge Units along its heading hierarchy, and parse the
//! YAML frontmatter that carries Context-Envelope identity.

use super::extract::classify_kind;

/// A single knowledge unit carved out of a markdown document.
///
/// Each unit is a self-contained section: its own heading, the body directly
/// under it, a context envelope (`heading_path`) tracing its ancestry from the
/// document root, and a link to its parent unit. Every unit is graded by a
/// lightweight linter (`evaluate_contract`) into accepted / degraded / quarantined
/// before it enters the retrieval index.
pub(super) struct DocUnit {
    pub(super) id: String,
    pub(super) parent_id: Option<String>,
    pub(super) title: String,
    pub(super) kind: String,
    pub(super) scope: String,
    pub(super) heading_path: String,
    pub(super) level: usize,
    pub(super) body: String,
    pub(super) chunk: String,
    pub(super) source_line: usize,
    pub(super) ord: usize,
    pub(super) has_children: bool,
}

/// Split a markdown document into a tree of Knowledge Units along its heading
/// hierarchy.
///
/// A single document root always exists (reusing a leading `# H1` when present,
/// otherwise synthesised from the file name) so preamble prose and orphan
/// sections still have a parent. Fenced code blocks are skipped so `#` comments
/// inside code are never mistaken for headings.
pub(super) fn split_into_units(content: &str, relative: &str, file_stem: &str) -> Vec<DocUnit> {
    let lines: Vec<&str> = content.lines().collect();

    // Strip a leading YAML frontmatter block (--- ... ---) so its metadata never
    // pollutes the root node or the search index. Lines are skipped, not removed,
    // so source line numbers stay accurate.
    let frontmatter_end = detect_frontmatter(&lines);
    let content_start = frontmatter_end.map(|end| end + 1).unwrap_or(0);

    let first_content = lines
        .iter()
        .enumerate()
        .skip(content_start)
        .find(|(_, line)| !line.trim().is_empty())
        .map(|(index, _)| index);
    let (root_title, root_line, skip_line) = match first_content {
        Some(pos) => match parse_heading(lines[pos]) {
            Some((1, title)) => (title, pos + 1, Some(pos)),
            _ => (file_stem.to_string(), content_start + 1, None),
        },
        None => (file_stem.to_string(), content_start + 1, None),
    };

    let mut units: Vec<DocUnit> = vec![DocUnit {
        id: format!("doc:{relative}"),
        parent_id: None,
        title: root_title.clone(),
        kind: "overview".into(),
        scope: "module".into(),
        heading_path: root_title,
        level: 0,
        body: String::new(),
        chunk: String::new(),
        source_line: root_line,
        ord: 0,
        has_children: false,
    }];

    let mut stack: Vec<usize> = vec![0];
    let mut current = 0usize;
    let mut ord = 1usize;
    let mut in_code_block = false;

    for (index, line) in lines.iter().enumerate() {
        if frontmatter_end.is_some_and(|end| index <= end) || Some(index) == skip_line {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
        }
        let heading = if in_code_block {
            None
        } else {
            parse_heading(line)
        };

        if let Some((level, title)) = heading {
            while let Some(&top) = stack.last() {
                if units[top].level >= level {
                    stack.pop();
                } else {
                    break;
                }
            }
            let parent_index = *stack.last().expect("root sentinel is always present");
            let parent_id = units[parent_index].id.clone();
            let mut titles: Vec<String> =
                stack.iter().map(|&idx| units[idx].title.clone()).collect();
            titles.push(title.clone());
            let kind = classify_kind(&title);
            // B5 layered granularity: scope by tree depth, not raw markdown level,
            // so it is robust whether a doc starts at H1 or H2. Depth 1 (direct
            // child of the doc root) is a major `section`; deeper nodes are fine
            // `subsection` detail. The root itself is `module` (promoted to
            // `system` later if the frontmatter declares one).
            let depth = stack.len(); // ancestors incl. root == tree depth of this node
            let scope = if depth <= 1 { "section" } else { "subsection" };
            units.push(DocUnit {
                id: format!("doc:{relative}#s{ord}"),
                parent_id: Some(parent_id),
                title,
                kind: kind.into(),
                scope: scope.into(),
                heading_path: titles.join(" > "),
                level,
                body: String::new(),
                chunk: format!("{line}\n"),
                source_line: index + 1,
                ord,
                has_children: false,
            });
            let new_index = units.len() - 1;
            units[parent_index].has_children = true;
            stack.push(new_index);
            current = new_index;
            ord += 1;
        } else {
            units[current].body.push_str(line);
            units[current].body.push('\n');
            units[current].chunk.push_str(line);
            units[current].chunk.push('\n');
        }
    }

    units
}

/// Detect a leading YAML frontmatter block and return the line index of its
/// closing `---`, if present. Only a `---` on the first non-empty line starts a
/// block.
fn detect_frontmatter(lines: &[&str]) -> Option<usize> {
    let first = lines.iter().position(|line| !line.trim().is_empty())?;
    if lines[first].trim() != "---" {
        return None;
    }
    lines[first + 1..]
        .iter()
        .position(|line| line.trim() == "---")
        .map(|relative| first + 1 + relative)
}

/// Parse an ATX markdown heading, returning `(level, title)`. Requires a space
/// after the `#` run (so `#tag` is not treated as a heading) and 1..=6 hashes.
pub(super) fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !(rest.starts_with(' ') || rest.starts_with('\t')) {
        return None;
    }
    let title = rest.trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some((hashes, title))
}

/// Frontmatter identity fields relevant to the Context Envelope. These declare a
/// document's tier on the scope-of-concern ladder (largest → smallest):
/// `architecture` (whole project) > `domain` (cross-module area) > `module`
/// (one code unit) > `feature` (one atomic thing, owned by a `module`).
/// `lesson` sits off the ladder: an error-sourced experience record that may
/// *link* to a `module:` without being owned by one (`domain:` links are rejected by lint).
#[derive(Debug, Default)]
pub(super) struct Frontmatter {
    pub(super) module: Option<String>,
    pub(super) domain: Option<String>,
    pub(super) feature: Option<String>,
    pub(super) architecture: Option<String>,
    pub(super) lesson: Option<String>,
    // Lesson v2 applicability contract (lesson documents only; lint warns
    // otherwise). `applies_when` / `excludes` are slug lists matched exactly
    // against a declared query context; `guard_strength` is one of
    // GUARD_STRENGTHS; `depends_on` is human-readable prose, never persisted
    // or matched — its review is a maintenance process, not a query.
    pub(super) applies_when: Vec<String>,
    pub(super) excludes: Vec<String>,
    pub(super) guard_strength: Option<String>,
    pub(super) depends_on: Option<String>,
}
/// Guard intervention strengths, strongest first: how much of the agent's
/// judgement a lesson's Guard pre-empts. `directive` = apply verbatim (a
/// deterministic check, no judgement); `scope` = confine the investigation;
/// `hint` = try first, then debug (the default when undeclared);
/// `reference` = background only.
pub(super) const GUARD_STRENGTHS: [&str; 4] = ["directive", "scope", "hint", "reference"];

/// A context slug is `[a-z0-9][a-z0-9-]*` — matching is exact equality, so
/// anything richer (spaces, case, punctuation) is an authoring error.
pub(super) fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && value.chars().next().is_some_and(|c| c != '-')
}

/// Split a stored comma-joined slug column back into entries.
pub(super) fn split_slugs(encoded: Option<&str>) -> Vec<String> {
    encoded
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parse a frontmatter list value: `[a, b]` or a bare `a, b`; entries are
/// lowercased and quote-stripped. Single values yield one entry.
fn parse_list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|entry| {
            entry
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_ascii_lowercase()
        })
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// Parse a leading YAML frontmatter block for the scope-ladder identity fields
/// (`architecture` / `domain` / `module` / `feature` / `lesson`; `system` is a
/// legacy alias for `domain`), plus the lesson applicability fields
/// (`applies-when` / `excludes` / `guard-strength` / `depends-on`). Only simple
/// `key: value` lines are read; lists are `[a, b]` or bare comma-joined. This
/// is deliberately not a full YAML parser.
pub(super) fn parse_frontmatter(content: &str) -> Frontmatter {
    let lines: Vec<&str> = content.lines().collect();
    let Some(end) = detect_frontmatter(&lines) else {
        return Frontmatter::default();
    };
    let first = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(0);
    let mut frontmatter = Frontmatter::default();
    for line in &lines[first + 1..end] {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().trim_matches('"').trim().to_string();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "module" => frontmatter.module = Some(value),
                // `system` kept as a backward-compatible alias for `domain`.
                "domain" | "system" => frontmatter.domain = Some(value),
                "feature" => frontmatter.feature = Some(value),
                "architecture" | "project" => frontmatter.architecture = Some(value),
                "lesson" => frontmatter.lesson = Some(value),
                "applies-when" => frontmatter.applies_when = parse_list(&value),
                "excludes" => frontmatter.excludes = parse_list(&value),
                "guard-strength" => {
                    frontmatter.guard_strength = Some(value.to_ascii_lowercase())
                }
                "depends-on" => frontmatter.depends_on = Some(value),
                _ => {}
            }
        }
    }
    frontmatter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_parses_lesson_with_optional_module_link() {
        let fm = parse_frontmatter("---\nlesson: gha-missing-token\nmodule: LyraGame/Weapons\n---\n# T\n");
        assert_eq!(fm.lesson.as_deref(), Some("gha-missing-token"));
        assert_eq!(fm.module.as_deref(), Some("LyraGame/Weapons"));
        assert!(fm.feature.is_none());

        let bare = parse_frontmatter("---\nlesson: cross-cutting\n---\n# T\n");
        assert_eq!(bare.lesson.as_deref(), Some("cross-cutting"));
        assert!(bare.module.is_none());
    }
    #[test]
    fn frontmatter_parses_lesson_applicability_fields() {
        let fm = parse_frontmatter(
            "---\nlesson: ubt-lock\napplies-when: [ubt-build, Editor-Running]\nexcludes: ci-build\nguard-strength: Directive\ndepends-on: the up wrapper flow\n---\n# T\n",
        );
        assert_eq!(fm.applies_when, vec!["ubt-build", "editor-running"]);
        assert_eq!(fm.excludes, vec!["ci-build"]);
        assert_eq!(fm.guard_strength.as_deref(), Some("directive"));
        assert_eq!(fm.depends_on.as_deref(), Some("the up wrapper flow"));

        let bare = parse_frontmatter("---\nlesson: x\n---\n# T\n");
        assert!(bare.applies_when.is_empty() && bare.excludes.is_empty());
        assert!(bare.guard_strength.is_none() && bare.depends_on.is_none());
    }

    #[test]
    fn slug_contract() {
        assert!(is_slug("ubt-build"));
        assert!(!is_slug("UbtBuild"));
        assert!(!is_slug("editor running"));
        assert!(!is_slug("-lead"));
        assert!(!is_slug(""));
        assert_eq!(
            split_slugs(Some("ubt-build,editor-running")),
            vec!["ubt-build", "editor-running"]
        );
        assert!(split_slugs(None).is_empty());
        assert!(split_slugs(Some("")).is_empty());
    }

    fn find<'a>(units: &'a [DocUnit], title: &str) -> &'a DocUnit {
        units
            .iter()
            .find(|u| u.title == title)
            .unwrap_or_else(|| panic!("unit {title} not found"))
    }

    #[test]
    fn parse_heading_accepts_atx_and_rejects_non_headings() {
        assert_eq!(parse_heading("## Data Flow"), Some((2, "Data Flow".into())));
        assert_eq!(
            parse_heading("###  Key Structs"),
            Some((3, "Key Structs".into()))
        );
        assert_eq!(parse_heading("#tag"), None); // no space after hashes
        assert_eq!(parse_heading("```"), None);
        assert_eq!(parse_heading("plain text"), None);
    }

    #[test]
    fn split_builds_tree_with_context_envelope() {
        let md = "# Root\n\nintro\n\n## A\n\nbody a\n\n### A1\n\nbody a1\n\n## B\n\nbody b\n";
        let units = split_into_units(md, "docs/x.md", "x");
        assert_eq!(units.len(), 4); // Root, A, A1, B

        let root = find(&units, "Root");
        assert_eq!(root.parent_id, None);
        assert_eq!(root.heading_path, "Root");

        let a = find(&units, "A");
        assert_eq!(a.parent_id.as_deref(), Some(root.id.as_str()));
        assert_eq!(a.heading_path, "Root > A");

        let a1 = find(&units, "A1");
        assert_eq!(a1.parent_id.as_deref(), Some(a.id.as_str()));
        assert_eq!(a1.heading_path, "Root > A > A1");

        let b = find(&units, "B");
        assert_eq!(b.parent_id.as_deref(), Some(root.id.as_str()));

        // B5 layered granularity: scope follows tree depth. Root is the module
        // overview; direct children (A, B) are sections; nested A1 is detail.
        assert_eq!(root.scope, "module");
        assert_eq!(a.scope, "section");
        assert_eq!(b.scope, "section");
        assert_eq!(a1.scope, "subsection");
    }

    #[test]
    fn split_ignores_headings_inside_code_fences() {
        let md = "# R\n\n```\n# not a heading\n```\n\n## Real\n\nx\n";
        let units = split_into_units(md, "d.md", "d");
        assert_eq!(units.len(), 2); // R (root) + Real
        assert!(units.iter().all(|u| u.title != "not a heading"));
    }

    #[test]
    fn frontmatter_is_stripped() {
        let md = "---\nmodule: X\ntags: [a, b]\nsource: auto\n---\n\n# Title\n\nreal body\n";
        let units = split_into_units(md, "d.md", "d");
        // YAML keys must not leak into any unit's body or chunk.
        assert!(units.iter().all(|u| !u.body.contains("module:")));
        assert!(units.iter().all(|u| !u.chunk.contains("tags:")));
        // The leading H1 (after frontmatter) becomes the root; real body kept.
        assert_eq!(units[0].title, "Title");
        assert!(units.iter().any(|u| u.body.contains("real body")));
    }

    #[test]
    fn splitting_preserves_content_lines() {
        let md = "# Root\n\nintro line\n\n## A\n\nbody a1\nbody a2\n\n### A1\n\ndeep\n";
        let units = split_into_units(md, "d.md", "d");
        let assembled: String = units.iter().map(|u| u.chunk.clone()).collect();
        assert!(assembled.contains("intro line"));
        assert!(assembled.contains("body a1"));
        assert!(assembled.contains("body a2"));
        assert!(assembled.contains("deep"));
    }
}
