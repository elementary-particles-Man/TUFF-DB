use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(s)?))
            }
        }
    };
}

id_newtype!(AbstractId);
id_newtype!(TagGroupId);
id_newtype!(TopicId);
