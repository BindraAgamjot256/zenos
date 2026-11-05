#[macro_export]
macro_rules! driver_init {
    // Branch A: With global name (Includes static creation + lock writing)
    ($vis:vis $fn_name:ident, $driver_type:ty, $global_name:ident, ($($args:tt)*)) => {
        static $global_name: RwLock<Option<KArc<$driver_type, GlobalAllocator>>> = RwLock::new(None);

        $vis fn $fn_name() -> Result<KArc<$driver_type, GlobalAllocator>, &'static str> {
            let ka = KBox::new(<$driver_type>::new($($args)*))
                .map_err(|_| "Memory allocation failed")?
                .into_arc();

            let mut lock = $global_name.write();
            *lock = Some(ka.clone());
            Ok(ka)
        }
    };

    // Branch B: NO global name (Skips static and skips lock assignment entirely)
    ($vis:vis $fn_name:ident, $driver_type:ty, ($($args:tt)*)) => {
        $vis fn $fn_name() -> Result<KArc<$driver_type, GlobalAllocator>, &'static str> {
            let ka = KBox::new(<$driver_type>::new($($args)*))
                .map_err(|_| "Memory allocation failed")?
                .into_arc();

            Ok(ka)
        }
    };
}
