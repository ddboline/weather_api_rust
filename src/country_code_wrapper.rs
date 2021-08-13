use derive_more::{Deref, Display, From, Into};
use isocountry::CountryCode;
use rweb::openapi::{ComponentDescriptor, ComponentOrInlineSchema, Entity};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

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
    fn type_name() -> Cow<'static, str> {
        String::type_name()
    }
    #[inline]
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        String::describe(comp_d)
    }
}
