FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /src/target/release/wasm-host /usr/local/bin/wasm-host
ENV ADDR=0.0.0.0:8000
EXPOSE 8000
USER nobody
CMD ["wasm-host"]
