FROM alpine:3.20

RUN apk add --no-cache ca-certificates
COPY foton /usr/local/bin/foton
WORKDIR /data

ENTRYPOINT ["/usr/local/bin/foton"]
