/// The trigger mode of an interrupt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(u8)]
pub enum Trigger {
    Edge = 0,
    Level = 1,
}

/// The polarity of an interrupt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(u8)]
pub enum Polarity {
    ActiveHigh = 0,
    ActiveLow = 1,
}
