use napi::Error;
use regex::Regex;

pub fn update_regex(key: &str) -> Result<Regex, Error> {
    let mut not_key_regex_str = "^".to_string();
    not_key_regex_str.push_str(&key);
    not_key_regex_str = not_key_regex_str.replace("*", ".*");
    not_key_regex_str.push('$');
    Regex::new(&not_key_regex_str)
        .map_err(|e| Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
}