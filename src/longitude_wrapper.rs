use derive_more::{Deref, Display, From, FromStr, Into};
use serde::{Deserialize, Serialize};
use utoipa::{
    PartialSchema, ToSchema,
    openapi::schema::{ObjectBuilder, Type},
};

use weather_util_rust::longitude::Longitude;

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
pub struct LongitudeWrapper(Longitude);

impl PartialSchema for LongitudeWrapper {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        ObjectBuilder::new()
            .format(Some(utoipa::openapi::SchemaFormat::Custom(
                "longitude".into(),
            )))
            .schema_type(Type::Number)
            .build()
            .into()
    }
}

impl ToSchema for LongitudeWrapper {
    fn name() -> std::borrow::Cow<'static, str> {
        "longitude".into()
    }
}
