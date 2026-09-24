FROM rustlang/rust:nightly-alpine3.23-2026-07-23@sha256:e4a0ce16a94f2585bc5fe1d852d70f77befdc89860da2d1afd89fd40d6ce830c AS builder
LABEL authors="junkydeveloper"

WORKDIR /foton

COPY . .
RUN cargo build --release --locked --features stand-alone

FROM scratch
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --chmod=755 --from=builder /foton/target/release/foton /

EXPOSE 25565

ENTRYPOINT ["/foton"]
