use derive_more::{Deref, Display, From, FromStr, Into};
use serde::{Deserialize, Serialize};
use utoipa::{
    PartialSchema, ToSchema,
    openapi::schema::{ObjectBuilder, Type},
};

use weather_util_rust::latitude::Latitude;

#[derive(
    Serialize,
    Debug,
    FromStr,
    PartialEq,
    Clone,
    Copy,
    Deref,
    Into,
    From,
    Deserialize,
    Hash,
    Display,
    Eq,
)]
pub struct LatitudeWrapper(Latitude);

impl PartialSchema for LatitudeWrapper {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        ObjectBuilder::new()
            .format(Some(utoipa::openapi::SchemaFormat::Custom(
                "latitude".into(),
            )))
            .schema_type(Type::Number)
            .build()
            .into()
    }
}

impl ToSchema for LatitudeWrapper {
    fn name() -> std::borrow::Cow<'static, str> {
        "latitude".into()
    }
}
