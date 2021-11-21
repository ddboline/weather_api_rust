FROM rust:alpine3.14 as buildstage

RUN apk add musl-dev && mkdir -p /build/weather_api_rust/src/

ADD Cargo.toml /build/weather_api_rust/
COPY src /build/weather_api_rust/src
COPY templates /build/weather_api_rust/templates

RUN mkdir -p /weather_api_rust && \
    cd /build/weather_api_rust && \
    cargo install --path=. --root=/weather_api_rust

FROM alpine:3.14

COPY --from=buildstage /weather_api_rust/bin/* /usr/bin/

STOPSIGNAL SIGTERM

EXPOSE 3097

CMD ["/usr/bin/weather-api-rust"]
