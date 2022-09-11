use derive_more::{Deref, Display, From, FromStr, Into};
use rweb::openapi::{ComponentDescriptor, ComponentOrInlineSchema, Entity, Schema, Type};
use serde::{Deserialize, Serialize};

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

impl Entity for LatitudeWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        "latitude".into()
    }

    #[inline]
    fn describe(_: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        ComponentOrInlineSchema::Inline(Schema {
            schema_type: Some(Type::Number),
            format: "latitude".into(),
            ..Schema::default()
        })
    }
}
