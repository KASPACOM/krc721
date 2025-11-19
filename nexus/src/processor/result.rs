use crate::processor::Error;

pub type Result<Ok = (), Err = Error> = std::result::Result<Ok, Err>;
