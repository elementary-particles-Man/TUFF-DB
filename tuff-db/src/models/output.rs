use crate::models::abstract_::Abstract;
use crate::models::verify::VerificationStatus;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputGate {
    pub min_status: VerificationStatus,
}

impl OutputGate {
    pub fn allow(&self, status: VerificationStatus) -> bool {
        status >= self.min_status
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputPacket {
    pub abstract_: Abstract,
    pub status: VerificationStatus,
}
