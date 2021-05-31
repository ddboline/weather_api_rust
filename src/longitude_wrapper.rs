use derive_more::{Deref, Display, From, FromStr, Into};
use rweb::openapi::{Entity, Schema, Type};
use serde::{Deserialize, Serialize};

use weather_util_rust::longitude::Longitude;

#[derive(
    Serialize, Debug, FromStr, PartialEq, Clone, Copy, Deref, Into, From, Deserialize, Hash, Display,
)]
pub struct LongitudeWrapper(Longitude);

impl Entity for LongitudeWrapper {
    #[inline]
    fn describe() -> Schema {
        Schema {
            schema_type: Some(Type::Number),
            format: "longitude".into(),
            ..Schema::default()
        }
    }
}
