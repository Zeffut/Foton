FROM rustlang/rust:nightly-alpine3.23 AS builder
LABEL authors="junkydeveloper"

WORKDIR /foton

COPY . .
RUN cargo build --release --locked --features stand-alone

FROM scratch
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --chmod=755 --from=builder /foton/target/release/foton /

EXPOSE 25565

ENTRYPOINT ["/foton"]
