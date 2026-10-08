FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /src/target/release/wardian /usr/local/bin/wardian
# Settings and the uploaded key live in /data, so mount a volume there. The master key that seals
# the keys lives in /keys, on a volume of its own (ADR-2610081501).
RUN mkdir /data /keys && chown nobody /data /keys && chmod 700 /keys
ENV ADDR=0.0.0.0:8000 DATA_DIR=/data
EXPOSE 8000
USER nobody
CMD ["wardian"]
