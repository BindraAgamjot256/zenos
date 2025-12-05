use crate::serial_print;

#[macro_export]
macro_rules! test_assert {
    ($cond:expr) => {
        if !$cond {
            return None;
        }
    };
}

#[macro_export]
macro_rules! test_assert_eq {
    ($left:expr, $right:expr) => {{
        let (left_val, right_val) = (&$left, &$right);
        if !(*left_val == *right_val) {
            return None;
        }
    }};
}

#[macro_export]
macro_rules! test_assert_ne {
    ($left:expr, $right:expr) => {{
        let (left_val, right_val) = (&$left, &$right);
        if !(*left_val != *right_val) {
            return None;
        }
    }};
}

#[allow(clippy::result_unit_err)]
pub trait Testable {
    fn run(&self) -> Result<(), ()>;
}

impl<T> Testable for T
where
    T: Fn() -> Option<()>,
{
    fn run(&self) -> Result<(), ()> {
        const WIDTH: usize = 70; // total width before the result
        let name = core::any::type_name::<T>();
        serial_print!("{:w$}", name, w = WIDTH);

        // Run the test
        let err = self();
        if err.is_none() {
            if name.contains("should_fail") {
                serial_print!("\x1b[1;92mOK\x1b[0m\n");
                return Ok(());
            }
            serial_print!("\x1b[1;91mFAILED\x1b[0m\n");
            return Err(());
        }
        serial_print!("\x1b[1;92mOK\x1b[0m\n");
        Ok(())
    }
}

mod tests {
    pub fn test_assert_eq_works() -> Option<()> {
        test_assert_eq!(1, 1);
        Some(())
    }

    pub fn test_assert_ne_works() -> Option<()> {
        test_assert_ne!(1, 2);
        Some(())
    }

    pub fn test_assert_eq_should_fail() -> Option<()> {
        test_assert_eq!(1, 2);
        Some(())
    }

    pub fn test_assert_ne_should_fail() -> Option<()> {
        test_assert_ne!(1, 1);
        Some(())
    }
    pub fn test_should_fail() -> Option<()> {
        None
    }
}

pub(crate) static TESTS: &[&(dyn Testable + Sync)] = {
    if cfg!(test) || cfg!(debug_assertions) {
        &[
            &tests::test_assert_eq_should_fail,
            &tests::test_assert_eq_works,
            &tests::test_assert_ne_should_fail,
            &tests::test_assert_ne_works,
            &tests::test_should_fail,
        ]
    } else {
        &[]
    }
};
