FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /src/target/release/rustle /usr/local/bin/rustle
# Settings and the uploaded key live in /data, so mount a volume there.
RUN mkdir /data && chown nobody /data
ENV ADDR=0.0.0.0:8000 DATA_DIR=/data
EXPOSE 8000
USER nobody
CMD ["rustle"]
