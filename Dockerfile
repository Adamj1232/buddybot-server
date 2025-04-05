FROM rust:1.72 as builder

WORKDIR /usr/src/buddybot-server
COPY . .

RUN cargo build --release

FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/buddybot-server/target/release/buddybot-server /usr/local/bin/buddybot-server

ENV RUST_LOG=info

EXPOSE 8080

CMD ["buddybot-server"]