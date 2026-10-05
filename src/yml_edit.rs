//! Surgical, line-based editing of `rblxsync.yml`.
//!
//! serde_yaml cannot preserve comments or formatting, so when `run` creates a
//! new resource it can't round-trip the whole file without destroying the
//! user's hand-authored layout. Instead we do a minimal text insert: find the
//! one list item we just created and splice a single `id:` line (or one
//! environment's entry in its `ids` map) into it, leaving every other byte of
//! the file untouched.

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
    let (start, end, key_indent) = find_item(&lines, section, name)?;
    // Already has an id? Return unchanged rather than duplicating the key.
    // (Documented choice: never write a second `id:` into an item that
    // already has one.)
    if item_has_key(&lines, start, end, key_indent, "id") {
        return Some(yaml.to_string());
    }
    let mut out = owned(&lines);
    out.insert(start + 1, format!("{}id: {}", " ".repeat(key_indent), id));
    Some(rejoin(yaml, out))
}

/// Insert `<env>: <id>` into the `ids` map of the list item under `section`
/// whose `name` equals `name`, unless that environment already has an id
/// there. A missing map is added as `ids: { <env>: <id> }` under the item's
/// `id:` line (or its first line). Returns None when the item can't be found
/// or its `ids` value is in a shape this editor doesn't touch.
pub fn insert_env_id(yaml: &str, section: &str, name: &str, env: &str, id: u64) -> Option<String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let (start, end, key_indent) = find_item(&lines, section, name)?;
    let mut out = owned(&lines);

    let Some(ids_line) = key_line(&lines, start, end, key_indent, "ids") else {
        let after = key_line(&lines, start, end, key_indent, "id").unwrap_or(start);
        let line = format!("{}ids: {{ {}: {} }}", " ".repeat(key_indent), env, id);
        out.insert(after + 1, line);
        return Some(rejoin(yaml, out));
    };

    let line = lines[ids_line];
    let value_at = line.find("ids:")? + "ids:".len();
    let (value, comment) = split_comment(&line[value_at..]);
    let value = value.trim();

    if value.is_empty() {
        // Block map: its entries are the following lines indented deeper.
        let entries: Vec<&str> = lines[ids_line + 1..end]
            .iter()
            .take_while(|l| l.trim().is_empty() || indent_of(l) > key_indent)
            .copied()
            .collect();
        if entries.iter().any(|l| parse_key_value(l, env).is_some()) {
            return Some(yaml.to_string());
        }
        let child_indent = entries
            .iter()
            .find(|l| !l.trim().is_empty())
            .map(|l| indent_of(l))
            .unwrap_or(key_indent + 2);
        out.insert(
            ids_line + 1,
            format!("{}{}: {}", " ".repeat(child_indent), env, id),
        );
        return Some(rejoin(yaml, out));
    }

    // Flow map on one line: `ids: { dev: 1 }`.
    let inner = value.strip_prefix('{')?.strip_suffix('}')?.trim();
    if inner
        .split(',')
        .any(|pair| parse_key_value(pair, env).is_some())
    {
        return Some(yaml.to_string());
    }
    let pairs = if inner.is_empty() {
        format!("{}: {}", env, id)
    } else {
        format!("{}, {}: {}", inner, env, id)
    };
    out[ids_line] = format!("{} {{ {} }}{}", &line[..value_at], pairs, comment);
    Some(rejoin(yaml, out))
}

/// Locate the list item under top-level `section` whose `name:` equals `name`.
/// Returns `(first line, end line exclusive, indent of its keys)`.
fn find_item(lines: &[&str], section: &str, name: &str) -> Option<(usize, usize, usize)> {
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
            if item_name(lines, idx, item_end, key_indent).as_deref() == Some(name) {
                return Some((idx, item_end, key_indent));
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
    key_line(lines, item_start, item_end, key_indent, key).is_some()
}

/// The line holding the item's `key:`, inline on its `- ` line or on a sibling
/// key line at `key_indent`.
fn key_line(
    lines: &[&str],
    item_start: usize,
    item_end: usize,
    key_indent: usize,
    key: &str,
) -> Option<usize> {
    // Inline keys on the `- ` line (e.g. `- id: 5` or `- name: X` style).
    if let Some((_, after)) = split_list_item(lines[item_start]) {
        if parse_key_value(after, key).is_some() {
            return Some(item_start);
        }
    }
    // Sibling key lines.
    (item_start + 1..item_end).find(|&i| {
        let line = lines[i];
        indent_of(line) == key_indent
            && line
                .trim_start()
                .strip_prefix(key)
                .is_some_and(|rest| rest.starts_with(':'))
    })
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Split a value from its trailing ` # comment`, keeping the comment's
/// leading whitespace with it.
fn split_comment(value: &str) -> (&str, &str) {
    match value.find(" #") {
        Some(at) => value.split_at(at),
        None => (value, ""),
    }
}

fn owned(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| line.to_string()).collect()
}

/// Join edited lines back into a file, keeping the original trailing newline
/// (which `str::lines` drops).
fn rejoin(yaml: &str, lines: Vec<String>) -> String {
    let mut result = lines.join("\n");
    if yaml.ends_with('\n') {
        result.push('\n');
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_id_adds_ids_under_the_id_line() {
        let yaml = "\
badges:
  - name: First Win
    id: 7
    icon: win.png
  - name: Second
";
        assert_eq!(
            insert_env_id(yaml, "badges", "First Win", "dev", 9).unwrap(),
            "\
badges:
  - name: First Win
    id: 7
    ids: { dev: 9 }
    icon: win.png
  - name: Second
"
        );
        // Without an `id:` line it goes right under the item's first line.
        assert_eq!(
            insert_env_id(yaml, "badges", "Second", "dev", 3).unwrap(),
            "\
badges:
  - name: First Win
    id: 7
    icon: win.png
  - name: Second
    ids: { dev: 3 }
"
        );
    }

    #[test]
    fn env_id_extends_a_flow_map_and_keeps_its_comment() {
        let yaml = "\
places:
  - name: Lobby
    ids: { dev: 1 } # set by hand
";
        assert_eq!(
            insert_env_id(yaml, "places", "Lobby", "staging", 2).unwrap(),
            "\
places:
  - name: Lobby
    ids: { dev: 1, staging: 2 } # set by hand
"
        );
        assert_eq!(
            insert_env_id(yaml, "places", "Lobby", "dev", 5).unwrap(),
            yaml,
            "an environment that has an id keeps it"
        );
    }

    #[test]
    fn env_id_extends_a_block_map() {
        let yaml = "\
game_passes:
  - name: VIP
    ids:
        dev: 1
    price: 100
";
        assert_eq!(
            insert_env_id(yaml, "game_passes", "VIP", "staging", 2).unwrap(),
            "\
game_passes:
  - name: VIP
    ids:
        staging: 2
        dev: 1
    price: 100
"
        );
        assert_eq!(
            insert_env_id(yaml, "game_passes", "VIP", "dev", 5).unwrap(),
            yaml
        );
        assert_eq!(
            insert_env_id(yaml, "game_passes", "Missing", "dev", 5),
            None
        );
    }

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
