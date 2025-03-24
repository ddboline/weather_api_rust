use derive_more::{Deref, Display, From, Into};
use isocountry::CountryCode;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use utoipa::{PartialSchema, ToSchema};

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

impl PartialSchema for CountryCodeWrapper {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        String::schema()
    }
}

impl ToSchema for CountryCodeWrapper {
    fn name() -> Cow<'static, str> {
        String::name()
    }

    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        String::schemas(schemas);
    }
}
