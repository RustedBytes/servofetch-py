FROM python:3.14-slim-bookworm AS wheel-builder

ARG RUST_VERSION=1.95.0
ARG TARGET=x86_64-unknown-linux-gnu

ENV CARGO_INCREMENTAL=0 \
    CARGO_HOME=/root/.cargo \
    RUST_BACKTRACE=1 \
    RUSTUP_HOME=/root/.rustup \
    PATH=/root/.cargo/bin:$PATH

WORKDIR /workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        clang \
        cmake \
        curl \
        libegl1 \
        libegl1-mesa-dev \
        libfontconfig-dev \
        libfontconfig1 \
        libfreetype-dev \
        libfreetype6 \
        libgl1-mesa-dri \
        libgles2 \
        libglib2.0-dev \
        libharfbuzz0b \
        libssl-dev \
        libx11-6 \
        libxcb-render0 \
        libxcb-shape0 \
        libxcb-xfixes0 \
        libxcb1 \
        llvm \
        mesa-utils \
        pkg-config \
        xvfb \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain "${RUST_VERSION}" --target "${TARGET}" \
    && rustup default "${RUST_VERSION}"

RUN python -m pip install --upgrade pip \
    && python -m pip install "maturin>=1.13,<2"

COPY pyproject.toml Cargo.toml Cargo.lock README.md ./
COPY src ./src
COPY vendor ./vendor

RUN python -m maturin build --release --locked --target "${TARGET}" \
        --interpreter python --out dist --compatibility linux

CMD ["sh", "-c", "ls -lh /workspace/dist"]
