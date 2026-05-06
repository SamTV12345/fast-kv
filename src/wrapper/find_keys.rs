use regex::Regex;

/// Mirrors `simpleGlobToRegExp` in `lib/AbstractDatabase.ts`.
///
/// Escapes regex metacharacters (`.+?^${}()|[]\\`) and replaces `*` with `.*`.
/// The result is anchored at both ends so it matches the whole input.
pub fn simple_glob_to_regex(s: &str) -> Regex {
    let mut out = String::with_capacity(s.len() + 4);
    out.push('^');
    for c in s.chars() {
        match c {
            '.' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '*' => out.push_str(".*"),
            _ => out.push(c),
        }
    }
    out.push('$');
    Regex::new(&out).expect("glob translation produced invalid regex")
}

/// Compiled key-matching pattern with optional negative match.
pub struct FindPattern {
    key: Regex,
    not_key: Option<Regex>,
}

impl FindPattern {
    pub fn matches(&self, candidate: &str) -> bool {
        if !self.key.is_match(candidate) {
            return false;
        }
        if let Some(nk) = &self.not_key {
            if nk.is_match(candidate) {
                return false;
            }
        }
        true
    }
}

pub fn compile_find_pattern(key: &str, not_key: Option<&str>) -> FindPattern {
    FindPattern {
        key: simple_glob_to_regex(key),
        not_key: not_key.map(simple_glob_to_regex),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_matches_anything() {
        let re = simple_glob_to_regex("foo*");
        assert!(re.is_match("foo"));
        assert!(re.is_match("foobar"));
        assert!(!re.is_match("xfoo"));
    }

    #[test]
    fn special_chars_are_escaped() {
        let re = simple_glob_to_regex("a.b+c");
        assert!(re.is_match("a.b+c"));
        assert!(!re.is_match("axbxc"));
    }

    #[test]
    fn not_key_excludes() {
        let p = compile_find_pattern("foo*", Some("foo:bar"));
        assert!(p.matches("foo:baz"));
        assert!(!p.matches("foo:bar"));
    }

    #[test]
    fn anchors_full_string() {
        let re = simple_glob_to_regex("abc");
        assert!(re.is_match("abc"));
        assert!(!re.is_match("xabcx"));
    }
}
