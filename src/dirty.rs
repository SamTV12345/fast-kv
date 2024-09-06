use std::fs::File;
use std::io::{ErrorKind, Write};
use std::sync::{Mutex};
use std::io::Error as IoError;
use napi::{Error, Status};
use rev_lines::RevLines;
use serde::{Deserialize, Serialize};

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
        let file = std::fs::File::open(&filename);

        match file {
            Ok(f) => Ok(Dirty {
                    mutex: Mutex::new(f),
                })
                ,
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    let f = std::fs::File::create(&filename).map_err(FileErrorWrapper::from)?;

                    Ok(Dirty {
                        mutex: Mutex::new(f),
                    })
                } else {
                    Err(FileErrorWrapper::from(e.to_string()).into())
                }
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
        mt.write_all(serialized.as_bytes()).map_err(FileErrorWrapper::from)?;

        Ok(())
    }

    #[napi]
    pub fn remove(&self, key:String) -> napi::Result<()> {
        self.set(key, DELETED.to_string())
    }
    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        let file = self.mutex.lock().map_err(|e|FileErrorWrapper::from(e.to_string()))?;
        std::mem::drop(file);
        Ok(())
    }
}