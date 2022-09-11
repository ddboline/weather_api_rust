use derive_more::{Deref, Display, From, FromStr, Into};
use rweb::openapi::{ComponentDescriptor, ComponentOrInlineSchema, Entity, Schema, Type};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

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

impl Entity for LongitudeWrapper {
    fn type_name() -> Cow<'static, str> {
        "longitude".into()
    }

    #[inline]
    fn describe(_: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        ComponentOrInlineSchema::Inline(Schema {
            schema_type: Some(Type::Number),
            format: "longitude".into(),
            ..Schema::default()
        })
    }
}
