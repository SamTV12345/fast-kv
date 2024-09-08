use std::fs::File;
use std::io::{ErrorKind, Write};
use std::sync::{Mutex};
use std::io::Error as IoError;
use napi::{Error, Status};
use rev_lines::RevLines;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Seek;
use crate::utils;
use hashbrown::HashSet;

#[napi(js_name = "Dirty")]
pub struct Dirty {
    mutex: Mutex<File>,
}


const DELETED: &str = "UNDEFINED";

// Define your own error wrapper
pub struct FileErrorWrapper(IoError);

// Implement `From` for your new wrapper type
impl From<IoError> for FileErrorWrapper {
    fn from(err: IoError) -> Self {
        FileErrorWrapper(err)
    }
}


impl From<String> for FileErrorWrapper {
    fn from(err: String) -> Self {
        FileErrorWrapper(IoError::new(ErrorKind::Other, err))
    }
}

// Implement `From` for converting SqliteErrorWrapper to napi::Error
impl From<FileErrorWrapper> for Error {
    fn from(wrapper: FileErrorWrapper) -> Self {
        Error::new(
            Status::GenericFailure,
            format!("File error: {}", wrapper.0)
        )
    }
}

#[derive(Serialize, Deserialize)]
pub struct DirtyVal {
    pub key: String,
    pub val: String
}

#[napi]
impl Dirty {
    #[napi(constructor)]
    pub fn new(filename: String) -> napi::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .write(true)
            .create(true)
            .open(filename);

        match file {
            Ok(f) => Ok(Dirty {
                    mutex: Mutex::new(f),
                })
                ,
            Err(e) => {

                    Err(FileErrorWrapper::from(e.to_string()).into())
                }
        }
    }
    #[napi]
    pub fn get(&self, key: String) -> napi::Result<Option<String>> {
        let mt = self.mutex.lock().map_err(|e|FileErrorWrapper::from(e.to_string()))?;
        let rev_lines = RevLines::new(&*mt);

        for line in rev_lines {
            match line {
                Ok(l) => {
                    let str = l.trim_end();
                    let dv_res = serde_json::from_str::<DirtyVal>(str);

                    match dv_res {
                        Ok(dv) => {
                            if dv.key == key {
                                if dv.val == DELETED {
                                    return Ok(None)
                                }

                                return Ok(Some(dv.val))
                            }
                        },
                        Err(_) => {
                            continue
                        }
                    }
                },
                Err(e) => {
                   return Err(Error::from(FileErrorWrapper::from(e.to_string())));
                }
            }
        }
        Ok(None)
    }
    #[napi]
    pub fn set(&self, key: String, val: String) -> napi::Result<()> {
        let dv = DirtyVal {
            key,
            val
        };

        let mut serialized = serde_json::to_string(&dv).map_err(|e|FileErrorWrapper::from(e.to_string()))?;

        let mut mt = self.mutex.lock().map_err(|e|FileErrorWrapper::from(e.to_string()))?;

        serialized.push_str("\n");
        mt.seek(std::io::SeekFrom::End(0)).map_err(FileErrorWrapper::from)?;
        mt.write_all(serialized.as_bytes()).map_err(FileErrorWrapper::from)?;

        Ok(())
    }

    #[napi]
    pub fn remove(&self, key:String) -> napi::Result<()> {
        self.set(key, DELETED.to_string())
    }
    #[napi]
    pub fn find_keys(&self, key: String, not_key: Option<String>) -> napi::Result<Vec<String>> {
        let not_key_regex: Option<regex::Regex>;
        let key_regex = utils::update_regex(&key).map_err(|e|FileErrorWrapper::from(e.to_string()))?;

        let mut deleted_keys = HashSet::new();
        if let Some(not_key) = not_key {
            not_key_regex = Some(utils::update_regex(&not_key).map_err(|e|FileErrorWrapper::from(e
                .to_string()))?);
        } else {
            not_key_regex = None;
        }

        let mt = self.mutex.lock().map_err(|e|FileErrorWrapper::from(e.to_string()))?;
        let rev_lines = RevLines::new(&*mt);
        let mut results = HashSet::new();

        for line in rev_lines {
            match line {
                Ok(l) => {
                    let str = l.trim_end();
                    let dv_res = serde_json::from_str::<DirtyVal>(str);

                    match dv_res {
                        Ok(dv) => {
                            if dv.key == DELETED {
                                deleted_keys.insert(dv.key);
                                continue
                            }

                            if deleted_keys.contains(&dv.key) {
                                continue
                            }

                            if key_regex.is_match(&dv.key) {
                                if let Some(not_key) = &not_key_regex {
                                    if !not_key.is_match(&dv.key) {
                                        results.insert(dv.key);
                                    }
                                } else {
                                    results.insert(dv.key);
                                }
                            }
                        },
                        Err(_) => {
                            continue
                        }
                    }
                },
                Err(e) => {
                    return Err(Error::from(FileErrorWrapper::from(e.to_string())));
                }
            }
        }
        Ok(results.into_iter().collect())
    }

    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        let file = self.mutex.lock().map_err(|e|FileErrorWrapper::from(e.to_string()))?;
        std::mem::drop(file);
        Ok(())
    }
}