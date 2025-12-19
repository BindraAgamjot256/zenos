use crate::serial_print;
use core::fmt::Debug;
use spin::Lazy;

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


#[cfg(feature = "run-kunittest")]
mod tests {
    use zenos_macros::test;
    #[test]
    pub fn test_assert_eq_works() -> Option<()> {
        test_assert_eq!(1, 1);
        Some(())
    }

    #[test]
    pub fn test_assert_ne_works() -> Option<()> {
        test_assert_ne!(1, 2);
        Some(())
    }

    #[test]
    pub fn test_assert_eq_should_fail() -> Option<()> {
        test_assert_eq!(1, 2);
        Some(())
    }
    #[test]
    pub fn test_assert_ne_should_fail() -> Option<()> {
        test_assert_ne!(1, 1);
        Some(())
    }
    #[test]
    pub fn test_should_fail() -> Option<()> {
        None
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Test {
    pub handler: fn() -> Option<()>,
    pub name: &'static str,
}

#[allow(improper_ctypes)]
unsafe extern "C" {
    static __start_tests: Test;
    static __stop_tests: Test;
}

#[cfg(feature = "run-kunittest")]
unsafe fn build_test_table() -> [Option<Test>; 256] {
    let mut table: [Option<Test>; 256] = [None; 256];

    let mut current = &__start_tests as *const Test;
    let stop = &__stop_tests as *const Test;

    for entry in table.iter_mut() {
        if current >= stop {
            break;
        }

        // Dereference the pointer to get the struct fields
        // Since it's #[repr(C)], this read is safe and aligned
        let entry_ref = &*current;

        entry.replace(Test {
            handler: entry_ref.handler,
            name: entry_ref.name,
        });

        // Move to next entry
        current = current.add(1);
    }

    table
}

impl Testable for Test {
    fn run(&self) -> Result<(), ()> {
        const WIDTH: usize = 70; // total width before the result
        serial_print!("{:w$}", self.name, w = WIDTH);

        // Run the test
        let err = { (self.handler)() };
        if err.is_none() {
            if self.name.contains("should_fail") {
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

impl Debug for Test {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Test").field("name", &self.name).finish()
    }
}

#[cfg(feature = "run-kunittest")]
pub static TESTS: Lazy<[Option<Test>; 256]> = Lazy::new(|| {
    let tab = unsafe { build_test_table() };
    tab
});
