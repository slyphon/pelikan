extern crate bytes;
extern crate memmap;

#[macro_use]
extern crate log;
extern crate clap;
extern crate env_logger;
extern crate failure;
#[macro_use]
extern crate failure_derive;



// dev dependencies
#[cfg(test)] extern crate tempfile;
#[cfg(test)] extern crate tinycdb;

pub mod cdb;
pub use cdb::{CDBError, Result, CDB};
pub use memmap::Mmap;
