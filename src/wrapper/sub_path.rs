use crate::error::{Result, UeberError};
use serde_json::{Map, Value};

/// Walk `path` through `value` and return a clone of the leaf, or `None` if any
/// segment is missing or applies to a non-traversable type.
///
/// An empty `path` returns a clone of the root.
pub fn get_sub(value: &Value, path: &[String]) -> Option<Value> {
    let mut cur = value;
    for segment in path {
        cur = match cur {
            Value::Object(m) => m.get(segment)?,
            Value::Array(a) => {
                let idx: usize = segment.parse().ok()?;
                a.get(idx)?
            }
            _ => return None,
        };
    }
    Some(cur.clone())
}

/// Walk `path` through `root`, creating intermediate objects as needed, and write
/// `new_value` at the leaf.
///
/// An empty `path` replaces the root. If a non-leaf segment lands on a non-object
/// (e.g. a string or number), returns `UeberError::SetSubOnNonObject` matching
/// the TS error string.
pub fn set_sub(root: &mut Value, path: &[String], new_value: Value) -> Result<()> {
    if path.is_empty() {
        *root = new_value;
        return Ok(());
    }
    if root.is_null() {
        *root = Value::Object(Map::new());
    }
    let mut cur = root;
    for (i, segment) in path.iter().enumerate() {
        let is_last = i == path.len() - 1;
        if !cur.is_object() {
            // Render the offending value as a bare string when possible (matches TS).
            let value_repr = match cur {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            return Err(UeberError::SetSubOnNonObject {
                prop: segment.clone(),
                value: value_repr,
            });
        }
        let map = cur.as_object_mut().expect("guarded by the is_object check above");
        if is_last {
            map.insert(segment.clone(), new_value);
            return Ok(());
        }
        cur = map
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn get_sub_walks_path() {
        let v = json!({"a": {"b": [10, 20]}});
        assert_eq!(
            get_sub(&v, &["a".into(), "b".into(), "1".into()]),
            Some(json!(20))
        );
    }

    #[test]
    fn get_sub_missing_returns_none() {
        let v = json!({"a": 1});
        assert_eq!(get_sub(&v, &["x".into()]), None);
    }

    #[test]
    fn get_sub_empty_path_returns_root() {
        let v = json!({"a": 1});
        assert_eq!(get_sub(&v, &[]), Some(v.clone()));
    }

    #[test]
    fn set_sub_creates_intermediate_objects() {
        let mut v = serde_json::Value::Null;
        set_sub(&mut v, &["a".into(), "b".into()], json!(7)).unwrap();
        assert_eq!(v, json!({"a": {"b": 7}}));
    }

    #[test]
    fn set_sub_replaces_existing_leaf() {
        let mut v = json!({"a": {"b": 1}});
        set_sub(&mut v, &["a".into(), "b".into()], json!(99)).unwrap();
        assert_eq!(v, json!({"a": {"b": 99}}));
    }

    #[test]
    fn set_sub_empty_path_replaces_root() {
        let mut v = json!({"a": 1});
        set_sub(&mut v, &[], json!("new")).unwrap();
        assert_eq!(v, json!("new"));
    }

    #[test]
    fn set_sub_on_non_object_errors() {
        let mut v = json!({"a": "literal"});
        let err = set_sub(&mut v, &["a".into(), "b".into()], json!(1)).unwrap_err();
        assert!(matches!(
            err,
            crate::error::UeberError::SetSubOnNonObject { .. }
        ));
    }
}
