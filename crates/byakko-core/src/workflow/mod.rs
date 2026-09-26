pub mod macro_assignment;
pub mod picture_preparation;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Problem {
    Validation(String),
    Device(crate::contract::Problem),
}
