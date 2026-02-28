//! Deterministic same-host realtime (T0/HardRT) communication contracts.

include!("realtime/realtime_part_01.rs");
include!("realtime/realtime_part_02.rs");

#[cfg(test)]
mod tests {
    include!("realtime/realtime_tests_part_01.rs");
    include!("realtime/realtime_tests_part_02.rs");
    include!("realtime/realtime_tests_part_03.rs");
}
