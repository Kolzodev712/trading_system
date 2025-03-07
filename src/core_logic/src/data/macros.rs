use super::super::utils::{deserialize_smol_str, serialize_smol_str};

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

macro_rules! impl_str_based_type {
    ($Name:tt) => {
        impl std::fmt::Display for $Name {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl From<$Name> for SmolStr {
            #[inline]
            fn from(value: $Name) -> Self {
                value.0.into()
            }
        }

        impl From<&$Name> for SmolStr {
            #[inline]
            fn from(value: &$Name) -> Self {
                value.0.clone()
            }
        }

        impl From<SmolStr> for $Name {
            #[inline]
            fn from(value: SmolStr) -> Self {
                Self(value)
            }
        }

        impl From<&SmolStr> for $Name {
            #[inline]
            fn from(value: &SmolStr) -> Self {
                Self(value.clone())
            }
        }

        impl From<&str> for $Name {
            #[inline]
            fn from(value: &str) -> Self {
                Self(value.into())
            }
        }

        impl From<String> for $Name {
            #[inline]
            fn from(value: String) -> Self {
                Self(value.into())
            }
        }

        impl From<&String> for $Name {
            #[inline]
            fn from(value: &String) -> Self {
                Self(value.into())
            }
        }

        impl AsRef<str> for $Name {
            #[inline]
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl std::borrow::Borrow<str> for $Name {
            #[inline]
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<&str> for $Name {
            #[inline]
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<String> for $Name {
            #[inline]
            fn eq(&self, other: &String) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&String> for $Name {
            #[inline]
            fn eq(&self, other: &&String) -> bool {
                self.0 == *other
            }
        }
    };
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub struct Symbol(
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    SmolStr,
);
impl_str_based_type!(Symbol);

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[repr(transparent)]
pub struct MillisecondTimestamp(pub u64);
