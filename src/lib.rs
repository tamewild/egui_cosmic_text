#![forbid(unsafe_code)]

pub mod atlas;
pub mod cursor;
pub mod draw;
pub mod util;
#[cfg(feature = "widget")]
pub mod widget;

pub use cosmic_text;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
