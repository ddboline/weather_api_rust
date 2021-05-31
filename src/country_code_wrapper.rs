use derive_more::{Deref, Display, From, Into};
use isocountry::CountryCode;
use rweb::openapi::{Entity, Schema};
use serde::{Deserialize, Serialize};

#[derive(
    Serialize,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    Deref,
    Into,
    From,
    Deserialize,
    Hash,
    Display,
)]
pub struct CountryCodeWrapper(CountryCode);

impl Entity for CountryCodeWrapper {
    #[inline]
    fn describe() -> Schema {
        String::describe()
    }
}
