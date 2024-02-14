use std::error::Error;

pub type GenericResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
pub type UnitResult = GenericResult<()>;

pub fn bool_string(val: bool) -> &'static str {
    if val { "#t" } else { "#f" }
}
