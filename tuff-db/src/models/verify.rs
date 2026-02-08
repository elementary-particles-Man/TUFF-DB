use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize,
)]
pub enum VerificationStatus {
    Smoke = 0,
    GrayBlack = 1,
    GrayMid = 2,
    GrayWhite = 3,
    White = 4,
}
