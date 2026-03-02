#[derive(Debug, Clone, Copy)]
pub enum BackpressurePolicy {
    Drop,
    Sample { every_nth: u32 },
}

impl BackpressurePolicy {
    pub fn should_forward(self, counter: &mut u64) -> bool {
        match self {
            BackpressurePolicy::Drop => true,
            BackpressurePolicy::Sample { every_nth } => {
                if every_nth <= 1 {
                    return true;
                }
                *counter = counter.wrapping_add(1);
                (*counter).is_multiple_of(every_nth as u64)
            }
        }
    }
}
