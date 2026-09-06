# syntax=docker/dockerfile:1

# ---- frontend ----
FROM node:22-bookworm AS frontend
WORKDIR /playground
COPY playground/frontend/package.json playground/frontend/*.config.* playground/frontend/tsconfig.json ./
RUN npm install
COPY playground/frontend/ ./
RUN NODE_ENV=production npm run build  # outputs to ../static (playground/static)

# ---- build ----
FROM rust:1-bookworm AS build
WORKDIR /src
COPY --from=frontend /static /src/playground/static
COPY . .
RUN cargo build --release -p vakd -p vakbrowse-api -p vakbrowse-cli -p vakbrowse-mcp -p vakbrowse-ffi \
      --features vakbrowse-server/dom-backend,vakbrowse-api/playground \
 && ./target/release/vakd doctor --no-probe   # bakes the pinned chrome-headless-shell into the image cache
# Operators can flip the server-wide default backend offline (chrome-free):
#   docker run -e VAKBROWSE_BACKEND=dom -p 7788:7788 vakbrowse

# ---- runtime ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates fonts-liberation \
      libnss3 libnspr4 libatk1.0-0 libatk-bridge2.0-0 libcups2 libdrm2 \
      libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 \
      libgbm1 libasound2 libpango-1.0-0 libcairo2 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/vakd       /usr/local/bin/
COPY --from=build /src/target/release/vakd-rest  /usr/local/bin/
COPY --from=build /src/target/release/vak-mcp    /usr/local/bin/
COPY --from=build /src/target/release/vak        /usr/local/bin/
COPY --from=build /src/target/release/libvakbrowse_ffi.so /usr/local/lib/
COPY --from=build /src/playground/static /playground
COPY --from=build /root/.cache/vakbrowse         /root/.cache/vakbrowse
ENV VAKBROWSE_HTTP_HOST=0.0.0.0 VAKBROWSE_HTTP_PORT=7788 VAKBROWSE_PLAYGROUND_DIR=/playground
EXPOSE 7788
CMD ["vakd-rest"]
