#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct ValueVersion {
    /// The version the value was changed the last time.
    pub changed: Version,
    /// The version the value was last time validated.
    pub validated: Version,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64);

impl Version {
    pub fn bump(&mut self) {
        self.0 += 1;
    }
}
