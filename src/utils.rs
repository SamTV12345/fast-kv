use napi::Error;
use regex::Regex;

pub fn update_regex(key: &str) -> Result<Regex, Error> {
  let mut not_key_regex_str = "^".to_string();
  not_key_regex_str.push_str(key);
  not_key_regex_str = not_key_regex_str.replace("*", ".*");
  not_key_regex_str.push('$');
  Regex::new(&not_key_regex_str)
    .map_err(|e| Error::new(napi::Status::GenericFailure, format!("{e:?}")))
}

fn simple_glob_to_regexp(s: &str) -> String {
  let mut escaped = String::new();
  for c in s.chars() {
    match c {
      '.' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
        escaped.push('\\');
        escaped.push(c);
      }
      '*' => {
        escaped.push_str(".*");
      }
      _ => escaped.push(c),
    }
  }
  escaped
}

pub fn create_find_regex(key: &str, not_key: Option<String>) -> String {
  let mut regex = format!("^(?={}$)", simple_glob_to_regexp(key));
  let not_key_regex = not_key.map(|nk| format!("(?!{}$)", simple_glob_to_regexp(&nk)));
  if let Some(not_regex) = not_key_regex {
    regex.push_str(&not_regex);
  }
  regex
}
