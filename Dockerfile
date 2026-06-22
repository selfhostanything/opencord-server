FROM rust:1.96-trixie AS build

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY src ./src

ARG SERVICE=api
RUN cargo build --release --bin ${SERVICE}

FROM debian:trixie-slim

ARG SERVICE=api
COPY --from=build /src/target/release/${SERVICE} /usr/local/bin/opencord

EXPOSE 8080

CMD ["/usr/local/bin/opencord"]
