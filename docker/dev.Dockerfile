# callgraph fejlesztői környezet: a build-, teszt- és mérőeszközök a konténerben vannak, a helyi gépen nem kellenek.
# Használat: bash tools/dev.sh <parancs>  (a tools/dev.sh építi és futtatja ezt az image-et)
FROM rust:1-bookworm

# rustfmt + clippy (ARCHITECTURE.md szabályai), musl target (a CI Linux-buildje x86_64/aarch64-unknown-linux-musl)
RUN rustup component add rustfmt clippy \
    && rustup target add "$(uname -m)-unknown-linux-musl"

# musl-tools: musl build; python3: golden/compare.py, tools/*.sh; php-cli: az ast_graph.php referencia (bench, order_check);
# nodejs: golden/make_js_golden.sh; zip: a csomag
RUN apt-get update \
    && apt-get install -y --no-install-recommends musl-tools python3 php-cli nodejs zip \
    && rm -rf /var/lib/apt/lists/*

# A konténer a hoszt felhasználójával fut (--user), ezért a cargo-otthon és a HOME mindenkinek írható.
# A /cargo egy named volume (registry + git cache) — a tartalma a buildek között megmarad.
ENV CARGO_HOME=/cargo \
    HOME=/tmp/home \
    CARGO_TARGET_DIR=/src/target/docker \
    PATH=/usr/local/cargo/bin:$PATH
RUN mkdir -p /cargo /tmp/home && chmod 1777 /cargo /tmp/home

WORKDIR /src
