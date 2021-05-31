use derive_more::{Deref, Display, From, FromStr, Into};
use rweb::openapi::{Entity, Schema, Type};
use serde::{Deserialize, Serialize};

use weather_util_rust::latitude::Latitude;

#[derive(
    Serialize, Debug, FromStr, PartialEq, Clone, Copy, Deref, Into, From, Deserialize, Hash, Display,
)]
pub struct LatitudeWrapper(Latitude);

impl Entity for LatitudeWrapper {
    #[inline]
    fn describe() -> Schema {
        Schema {
            schema_type: Some(Type::Number),
            format: "latitude".into(),
            ..Schema::default()
        }
    }
}
