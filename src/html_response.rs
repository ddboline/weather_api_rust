use http::status::StatusCode;
use indexmap::IndexMap;
use rweb::{
    hyper::{Body, Response},
    openapi::{self, Entity, MediaType, ObjectOrReference, ResponseEntity, Responses},
    Reply,
};
use stack_string::StackString;
use std::borrow::Cow;

use crate::errors::ServiceError as Error;

pub struct HtmlResponse<T>
where
    T: Send,
    Body: From<T>,
{
    data: T,
    status: StatusCode,
}

impl<T> HtmlResponse<T>
where
    T: Send,
    Body: From<T>,
{
    pub fn new(data: T) -> Self {
        Self {
            data,
            status: StatusCode::OK,
        }
    }
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
}

impl<T> Reply for HtmlResponse<T>
where
    T: Send,
    Body: From<T>,
{
    fn into_response(self) -> Response<Body> {
        let reply = rweb::reply::html(self.data);
        let reply = rweb::reply::with_status(reply, self.status);
        reply.into_response()
    }
}

impl<T> Entity for HtmlResponse<T>
where
    T: Send,
    Body: From<T>,
{
    fn describe() -> openapi::Schema {
        Result::<StackString, Error>::describe()
    }
}

impl<T> ResponseEntity for HtmlResponse<T>
where
    T: Send,
    Body: From<T>,
{
    fn describe_responses() -> Responses {
        let mut content = IndexMap::new();
        content.insert(
            Cow::Borrowed("text/html"),
            MediaType {
                schema: Some(ObjectOrReference::Object(Self::describe())),
                examples: None,
                encoding: Default::default(),
            },
        );

        let mut map = IndexMap::new();
        map.insert(
            Cow::Borrowed("200"),
            openapi::Response {
                content,
                ..Default::default()
            },
        );
        map
    }
}
