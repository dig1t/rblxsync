//! Surgical, line-based editing of `rblxsync.yml`.
//!
//! serde_yaml cannot preserve comments or formatting, so when `run` creates a
//! new resource it can't round-trip the whole file without destroying the
//! user's hand-authored layout. Instead we do a minimal text insert: find the
//! one list item we just created and splice a single `id:` line into it,
//! leaving every other byte of the file untouched.

/// Insert `id: <id>` into the list item under top-level `section` whose `name`
/// equals `name`, if that item does not already have an `id`. Returns the new
/// YAML text on success, or None if the item couldn't be located (caller then
/// warns the user to run `import`).
///
/// ponytail: this handles standard block-style YAML (2-space indent, one item
/// per `- name:` line) as produced by serde_yaml and typical hand-authored
/// files. Flow/inline style (`game_passes: [{...}]`) is not supported and
/// returns None. That is the acceptable ceiling for this surgical editor.
pub fn insert_id(yaml: &str, section: &str, name: &str, id: u64) -> Option<String> {
    let lines: Vec<&str> = yaml.lines().collect();

    // 1. Locate the top-level section key (e.g. `game_passes:` at column 0).
    let section_start = lines
        .iter()
        .position(|line| is_section_header(line, section))?;

    // 2. Determine the bounds of the section's body: every subsequent line up to
    //    (but not including) the next top-level key (a non-blank line that is not
    //    indented). Comments and blank lines inside the section are part of it.
    let section_end = lines[section_start + 1..]
        .iter()
        .position(|line| is_top_level_content(line))
        .map(|offset| section_start + 1 + offset)
        .unwrap_or(lines.len());

    // 3. Scan the section body for list items (`- ` entries) and find the one
    //    whose `name:` value matches.
    let mut idx = section_start + 1;
    while idx < section_end {
        let line = lines[idx];

        if let Some((dash_indent, _after_dash)) = split_list_item(line) {
            // The first key on the `- ` line is a sibling key. Subsequent keys
            // are indented to `dash_indent + 2` (the `- ` is two characters).
            let key_indent = dash_indent + 2;

            // Find where this item ends: the next `- ` at the same dash indent,
            // or the end of the section.
            let item_end = lines[idx + 1..section_end]
                .iter()
                .position(|l| {
                    split_list_item(l)
                        .map(|(di, _)| di == dash_indent)
                        .unwrap_or(false)
                })
                .map(|offset| idx + 1 + offset)
                .unwrap_or(section_end);

            // Match on this item's `name:` value, whether it's inline on the
            // `- ` line or on a sibling key line.
            if item_name(&lines, idx, item_end, key_indent).as_deref() == Some(name) {
                // Already has an id? Return unchanged rather than duplicating
                // the key. (Documented choice: never write a second `id:` into
                // an item that already has one.)
                if item_has_key(&lines, idx, item_end, key_indent, "id") {
                    return Some(yaml.to_string());
                }
                return Some(splice_id_line(yaml, &lines, idx, key_indent, id));
            }

            idx = item_end;
        } else {
            idx += 1;
        }
    }

    None
}

/// True if `line` is exactly the top-level key `section:` at column 0.
fn is_section_header(line: &str, section: &str) -> bool {
    if line.starts_with(' ') || line.starts_with('\t') {
        return false;
    }
    match line.trim_end().strip_suffix(':') {
        Some(key) => key == section,
        None => false,
    }
}

/// True if a line begins un-indented, non-blank content (the start of the next
/// top-level key). Blank lines and comment-only lines do NOT terminate a
/// section.
fn is_top_level_content(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    let first = line.as_bytes()[0];
    // Indented lines belong to the current section.
    if first == b' ' || first == b'\t' {
        return false;
    }
    // A fully blank (whitespace-only) line does not terminate the section.
    if line.trim().is_empty() {
        return false;
    }
    // A top-level comment is ambiguous; treat it as still inside the section so
    // trailing comments stay attached. Only real keys terminate.
    !line.trim_start().starts_with('#')
}

/// If `line` is a list item (`<indent>- ...`), return (indent, text after `- `).
fn split_list_item(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    let after = rest.strip_prefix("- ")?;
    Some((indent, after))
}

/// Parse `key: value` from a fragment, returning the logical string value
/// (unquoting double/single quotes) if the key matches.
fn parse_key_value(fragment: &str, key: &str) -> Option<String> {
    let fragment = fragment.trim();
    let rest = fragment.strip_prefix(key)?;
    let rest = rest.strip_prefix(':')?;
    let value = rest.trim();
    Some(unquote(value))
}

/// Strip a single matching pair of surrounding quotes (double or single).
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

/// Does the item spanning `[item_start, item_end)` already contain `key:` either
/// inline on the `- ` line or on a sibling key line at `key_indent`?
fn item_has_key(
    lines: &[&str],
    item_start: usize,
    item_end: usize,
    key_indent: usize,
    key: &str,
) -> bool {
    // Inline keys on the `- ` line (e.g. `- id: 5` or `- name: X` style).
    if let Some((_, after)) = split_list_item(lines[item_start]) {
        if parse_key_value(after, key).is_some() {
            return true;
        }
    }
    // Sibling key lines.
    for line in &lines[item_start + 1..item_end] {
        let indent = line.len() - line.trim_start().len();
        if indent == key_indent {
            if let Some(stripped) = line.trim_start().strip_prefix(key) {
                if stripped.starts_with(':') {
                    return true;
                }
            }
        }
    }
    false
}

/// Read the `name:` value of the item spanning `[item_start, item_end)`, whether
/// the key is inline on the `- ` line or on a sibling key line at `key_indent`.
fn item_name(
    lines: &[&str],
    item_start: usize,
    item_end: usize,
    key_indent: usize,
) -> Option<String> {
    // Inline on the `- ` line.
    if let Some((_, after)) = split_list_item(lines[item_start]) {
        if let Some(value) = parse_key_value(after, "name") {
            return Some(value);
        }
    }
    // Sibling key lines at the item's key indent.
    for line in &lines[item_start + 1..item_end] {
        let indent = line.len() - line.trim_start().len();
        if indent == key_indent {
            if let Some(value) = parse_key_value(line, "name") {
                return Some(value);
            }
        }
    }
    None
}

/// Build the new YAML by inserting `id: <id>` immediately after the item's first
/// line (`item_start`), indented to `key_indent`. Preserves the original line
/// terminator style and trailing-newline presence.
fn splice_id_line(
    yaml: &str,
    lines: &[&str],
    item_start: usize,
    key_indent: usize,
    id: u64,
) -> String {
    let indent = " ".repeat(key_indent);
    let new_line = format!("{}id: {}", indent, id);

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 1);
    for (i, line) in lines.iter().enumerate() {
        out.push((*line).to_string());
        if i == item_start {
            out.push(new_line.clone());
        }
    }

    let mut result = out.join("\n");
    // `str::lines` drops a trailing newline; restore it if the original had one.
    if yaml.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_id_preserving_comments() {
        let yaml = "\
# My game config
game_passes:
  # the premium pass
  - name: VIP Pass
    price: 100
";
        let out = insert_id(yaml, "game_passes", "VIP Pass", 42).unwrap();
        assert_eq!(
            out,
            "\
# My game config
game_passes:
  # the premium pass
  - name: VIP Pass
    id: 42
    price: 100
"
        );
    }

    #[test]
    fn only_right_section_and_item_touched() {
        let yaml = "\
game_passes:
  - name: VIP Pass
    price: 100

developer_products:
  - name: Speed Boost
    price: 50

badges:
  - name: First Win
";
        let out = insert_id(yaml, "developer_products", "Speed Boost", 777).unwrap();
        assert_eq!(
            out,
            "\
game_passes:
  - name: VIP Pass
    price: 100

developer_products:
  - name: Speed Boost
    id: 777
    price: 50

badges:
  - name: First Win
"
        );
    }

    #[test]
    fn matches_double_quoted_name() {
        let yaml = "game_passes:\n  - name: \"VIP Pass\"\n    price: 100\n";
        let out = insert_id(yaml, "game_passes", "VIP Pass", 1).unwrap();
        assert_eq!(
            out,
            "game_passes:\n  - name: \"VIP Pass\"\n    id: 1\n    price: 100\n"
        );
    }

    #[test]
    fn matches_single_quoted_name() {
        let yaml = "game_passes:\n  - name: 'VIP Pass'\n    price: 100\n";
        let out = insert_id(yaml, "game_passes", "VIP Pass", 2).unwrap();
        assert_eq!(
            out,
            "game_passes:\n  - name: 'VIP Pass'\n    id: 2\n    price: 100\n"
        );
    }

    #[test]
    fn matches_unquoted_name() {
        let yaml = "game_passes:\n  - name: VIP Pass\n    price: 100\n";
        let out = insert_id(yaml, "game_passes", "VIP Pass", 3).unwrap();
        assert_eq!(
            out,
            "game_passes:\n  - name: VIP Pass\n    id: 3\n    price: 100\n"
        );
    }

    #[test]
    fn unchanged_when_id_already_present() {
        let yaml = "game_passes:\n  - name: VIP Pass\n    id: 99\n    price: 100\n";
        let out = insert_id(yaml, "game_passes", "VIP Pass", 5).unwrap();
        assert_eq!(out, yaml);
    }

    #[test]
    fn unchanged_when_id_inline_on_dash_line() {
        // id appears inline on the `- ` line; should still be detected.
        let yaml = "game_passes:\n  - id: 99\n    name: VIP Pass\n";
        let out = insert_id(yaml, "game_passes", "VIP Pass", 5).unwrap();
        assert_eq!(out, yaml);
    }

    #[test]
    fn name_not_found_returns_none() {
        let yaml = "game_passes:\n  - name: VIP Pass\n    price: 100\n";
        assert!(insert_id(yaml, "game_passes", "Nonexistent", 1).is_none());
    }

    #[test]
    fn section_not_found_returns_none() {
        let yaml = "game_passes:\n  - name: VIP Pass\n    price: 100\n";
        assert!(insert_id(yaml, "badges", "VIP Pass", 1).is_none());
    }

    #[test]
    fn two_items_same_price_only_matching_name_gets_id() {
        let yaml = "\
game_passes:
  - name: VIP Pass
    price: 100
  - name: Gold Pass
    price: 100
";
        let out = insert_id(yaml, "game_passes", "Gold Pass", 555).unwrap();
        assert_eq!(
            out,
            "\
game_passes:
  - name: VIP Pass
    price: 100
  - name: Gold Pass
    id: 555
    price: 100
"
        );
    }

    #[test]
    fn realistic_multisection_with_blanks_and_comments() {
        let yaml = "\
# rblxsync configuration
assets_dir: assets/icons/

universe:
  id: 123456789
  name: \"My Game\"

# Monetization
game_passes:
  - name: VIP Pass     # the good one
    price: 100
    is_for_sale: true

  - name: Starter Pack
    price: 25

badges:
  - name: First Win
    description: \"Win your first match\"
";
        let out = insert_id(yaml, "game_passes", "Starter Pack", 2024).unwrap();
        assert_eq!(
            out,
            "\
# rblxsync configuration
assets_dir: assets/icons/

universe:
  id: 123456789
  name: \"My Game\"

# Monetization
game_passes:
  - name: VIP Pass     # the good one
    price: 100
    is_for_sale: true

  - name: Starter Pack
    id: 2024
    price: 25

badges:
  - name: First Win
    description: \"Win your first match\"
"
        );
    }

    #[test]
    fn no_trailing_newline_preserved() {
        let yaml = "game_passes:\n  - name: VIP Pass\n    price: 100";
        let out = insert_id(yaml, "game_passes", "VIP Pass", 7).unwrap();
        assert_eq!(
            out,
            "game_passes:\n  - name: VIP Pass\n    id: 7\n    price: 100"
        );
    }

    #[test]
    fn name_in_other_section_not_matched() {
        let yaml = "\
game_passes:
  - name: Shared Name
    price: 100
badges:
  - name: Shared Name
";
        // Inserting into badges must touch the badges entry, not the game pass.
        let out = insert_id(yaml, "badges", "Shared Name", 88).unwrap();
        assert_eq!(
            out,
            "\
game_passes:
  - name: Shared Name
    price: 100
badges:
  - name: Shared Name
    id: 88
"
        );
    }
}
