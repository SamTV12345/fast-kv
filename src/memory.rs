use hashbrown::HashMap;
use napi::Error;
use regex::Regex;

#[napi(js_name = "MemoryDB")]
pub struct MemoryDB {
    db: HashMap<String, String>,
}


fn update_regex(key: &str) -> Result<Regex, Error> {
    let mut not_key_regex_str = "^".to_string();
    not_key_regex_str.push_str(&key);
    not_key_regex_str = not_key_regex_str.replace("*", ".*");
    not_key_regex_str.push('$');
    Regex::new(&not_key_regex_str)
        .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
}


#[napi]
impl MemoryDB {
    #[napi(constructor)]
    pub fn new() -> Self {
        MemoryDB {
            db: HashMap::new(),
        }
    }
    #[napi]
    pub fn get(&self, key: String) -> napi::Result<Option<String>> {
        let val = self.db.get(&key).map(|v| v.clone());
        Ok(val)
    }
    #[napi]
    pub fn set(&mut self, key: String, value: String) -> napi::Result<()> {
        self.db.insert(key, value);
        Ok(())
    }
    #[napi]
    pub fn remove(&mut self, key: String) ->napi::Result<()> {
        self.db.remove(&key);
        Ok(())
    }
    #[napi]
    pub fn find_keys(&self, key: String, not_key: Option<String>) -> napi::Result<Vec<String>> {
        let not_key_regex: Option<Regex>;
        let key_regex = update_regex(&key)?;

        if let Some(not_key) = not_key {
            not_key_regex = Some(update_regex(&not_key)?);
        } else {
            not_key_regex = None;
        }


        let result = self.db
            .iter()
            .filter(|(k,_)|{
                if let Some(not_key) = &not_key_regex {
                    key_regex.is_match(&k) && !not_key.is_match(k)
                } else {
                    key_regex.is_match(&k)
                }
            })
            .map(|k|k.0.clone())
            .collect::<Vec<String>>();

        Ok(result)
    }
    #[napi]
    pub fn close(&mut self) -> napi::Result<()> {
        self.db.clear();
        Ok(())
    }
}