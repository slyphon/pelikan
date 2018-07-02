extern crate bytes;
extern crate env_logger;
extern crate failure;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate log;
extern crate memmap;

// dev dependencies
#[cfg(test)] extern crate tempfile;
#[cfg(test)] extern crate tinycdb;


pub use cdb::{CDBData, CDBError, LoadOption, Reader, Result, Source, Writer};
pub use memmap::Mmap;

pub mod cdb;

