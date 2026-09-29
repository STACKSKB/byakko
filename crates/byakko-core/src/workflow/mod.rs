pub mod host;
pub mod macro_assignment;
pub mod observation;
pub mod picture_preparation;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Problem {
    Validation(String),
    Device(crate::contract::Problem),
}
