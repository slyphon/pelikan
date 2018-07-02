// for docs on the 'failure' crate see https://boats.gitlab.io/failure/intro.html

#[derive(Debug, Fail)]
pub enum CDBError {
    #[fail(display = "Value too large, max_size: {}, val_size: {}", max_size, val_size)]
    ValueTooLarge { max_size: usize, val_size: usize },
}

impl CDBError {
    pub fn value_too_large(max_size: usize, val_size: usize) -> CDBError {
        CDBError::ValueTooLarge { max_size, val_size }
    }
}
