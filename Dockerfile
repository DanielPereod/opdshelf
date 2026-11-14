# Front-end assets (Tailwind CSS and Font Awesome)
FROM node:22-alpine AS assets
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY static ./static
COPY views ./views
RUN npm run build

# Server binary; templates and static files are embedded into it
FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY views ./views
COPY --from=assets /app/static ./static
RUN cargo build --release --locked

FROM alpine:3.22
# latest for images built from main, beta for the beta branch: picks which
# releases of the Android app and KOReader plugin the web panel offers
ARG CHANNEL=latest
# pdfinfo/pdftoppm for PDF covers, bsdtar for CBR/CB7/CBT comics, tzdata for TZ (days in reading statistics)
RUN apk add --no-cache poppler-utils libarchive-tools tzdata
COPY --from=builder /app/target/release/opdshelf /usr/local/bin/opdshelf
WORKDIR /app
RUN mkdir -p /app/books

ENV PORT=3000 \
    HOST=0.0.0.0 \
    BOOKS_DIR=/app/books \
    APPS_CHANNEL=${CHANNEL}

EXPOSE 3000
VOLUME ["/app/books"]
HEALTHCHECK --interval=30s --timeout=5s CMD wget -qO- "http://127.0.0.1:${PORT}/healthz" || exit 1

CMD ["opdshelf"]
