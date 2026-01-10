# Multi-stage Dockerfile for encoding-pipeline
# Builds all video encoding tools from source for maximum compatibility

# =============================================================================
# Stage 1: Build all dependencies from source
# =============================================================================
FROM debian:bookworm-slim AS builder

ENV DEBIAN_FRONTEND=noninteractive
ENV PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:/usr/local/lib64/pkgconfig"
ENV CARGO_HOME="/root/.cargo"
ENV PATH="/root/.cargo/bin:$PATH"

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential cmake meson ninja-build nasm yasm git curl wget \
    pkg-config autoconf automake libtool python3 python3-pip python3-dev \
    cython3 libssl-dev zlib1g-dev libfreetype6-dev \
    libfontconfig1-dev libass-dev libfribidi-dev libharfbuzz-dev libbz2-dev \
    libmp3lame-dev libopus-dev libvpx-dev libfdk-aac-dev \
    libxml2-dev libnuma-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN . $CARGO_HOME/env

WORKDIR /build

# -----------------------------------------------------------------------------
# Build libvmaf (required by FFmpeg and aomenc)
# -----------------------------------------------------------------------------
RUN git clone --depth 1 --branch v3.0.0 https://github.com/Netflix/vmaf.git && \
    cd vmaf/libvmaf && \
    meson setup build --buildtype release --prefix=/usr/local \
        -Denable_tests=false -Denable_docs=false && \
    ninja -C build && \
    ninja -C build install && \
    ldconfig

# -----------------------------------------------------------------------------
# Build x264
# -----------------------------------------------------------------------------
RUN git clone --depth 1 https://code.videolan.org/videolan/x264.git && \
    cd x264 && \
    ./configure --prefix=/usr/local --enable-static --enable-pic && \
    make -j$(nproc) && \
    make install

# -----------------------------------------------------------------------------
# Build x265 multilib (8/10/12-bit support)
# -----------------------------------------------------------------------------
RUN git clone --depth 1 https://bitbucket.org/multicoreware/x265_git.git x265 && \
    cd x265/build/linux && \
    # Build 12-bit
    mkdir -p 12bit && cd 12bit && \
    cmake ../../../source -DHIGH_BIT_DEPTH=ON \
        -DEXPORT_C_API=OFF -DENABLE_SHARED=OFF -DENABLE_CLI=OFF -DMAIN12=ON && \
    make -j$(nproc) && \
    cd .. && \
    # Build 10-bit
    mkdir -p 10bit && cd 10bit && \
    cmake ../../../source -DHIGH_BIT_DEPTH=ON \
        -DEXPORT_C_API=OFF -DENABLE_SHARED=OFF -DENABLE_CLI=OFF && \
    make -j$(nproc) && \
    cd .. && \
    # Build 8-bit with linked 10/12-bit
    mkdir -p 8bit && cd 8bit && \
    ln -sf ../10bit/libx265.a libx265_main10.a && \
    ln -sf ../12bit/libx265.a libx265_main12.a && \
    cmake ../../../source -DCMAKE_INSTALL_PREFIX=/usr/local \
        -DEXTRA_LIB="x265_main10.a;x265_main12.a" -DEXTRA_LINK_FLAGS=-L. \
        -DLINKED_10BIT=ON -DLINKED_12BIT=ON && \
    make -j$(nproc) && \
    make install

# -----------------------------------------------------------------------------
# Build SVT-AV1
# -----------------------------------------------------------------------------
RUN git clone --depth 1 https://gitlab.com/AOMediaCodec/SVT-AV1.git && \
    cd SVT-AV1/Build && \
    cmake .. -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr/local \
        -DBUILD_SHARED_LIBS=OFF && \
    make -j$(nproc) && \
    make install

# -----------------------------------------------------------------------------
# Build aomenc (libaom)
# -----------------------------------------------------------------------------
RUN git clone --depth 1 --branch v3.8.0 https://aomedia.googlesource.com/aom && \
    mkdir aom_build && cd aom_build && \
    cmake ../aom -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr/local \
        -DENABLE_TESTS=OFF -DENABLE_DOCS=OFF \
        -DCONFIG_TUNE_VMAF=1 && \
    make -j$(nproc) && \
    make install

# -----------------------------------------------------------------------------
# Build zimg (required by VapourSynth)
# -----------------------------------------------------------------------------
RUN git clone --depth 1 --branch release-3.0.5 https://github.com/sekrit-twc/zimg.git && \
    cd zimg && \
    ./autogen.sh && \
    ./configure --prefix=/usr/local && \
    make -j$(nproc) && \
    make install

# -----------------------------------------------------------------------------
# Build VapourSynth R72 (R73 has breaking API changes)
# -----------------------------------------------------------------------------
RUN pip3 install cython --break-system-packages && \
    git clone --depth 1 --branch R72 https://github.com/vapoursynth/vapoursynth.git && \
    cd vapoursynth && \
    ./autogen.sh && \
    ./configure --prefix=/usr/local && \
    make -j$(nproc) && \
    make install && \
    ldconfig

# -----------------------------------------------------------------------------
# Build L-SMASH (for lsmash chunking in av1an)
# -----------------------------------------------------------------------------
RUN git clone --depth 1 https://github.com/l-smash/l-smash.git && \
    cd l-smash && \
    ./configure --prefix=/usr/local && \
    make -j$(nproc) && \
    make install

# Build L-SMASH-Works VapourSynth plugin
RUN git clone --depth 1 https://github.com/AkarinVS/L-SMASH-Works.git && \
    cd L-SMASH-Works/VapourSynth && \
    meson setup build --prefix=/usr/local && \
    ninja -C build && \
    ninja -C build install

# -----------------------------------------------------------------------------
# Build FFmpeg with all encoders + libvmaf
# -----------------------------------------------------------------------------
RUN git clone --depth 1 --branch n7.1 https://github.com/FFmpeg/FFmpeg.git && \
    cd FFmpeg && \
    ./configure --prefix=/usr/local \
        --enable-gpl --enable-version3 --enable-nonfree \
        --enable-libvmaf --enable-libx265 --enable-libx264 \
        --enable-libsvtav1 --enable-libaom \
        --enable-libopus --enable-libfdk-aac \
        --enable-libass --enable-libfreetype \
        --enable-libmp3lame --enable-libvpx \
        --disable-debug --disable-doc && \
    make -j$(nproc) && \
    make install

# -----------------------------------------------------------------------------
# Build mkvtoolnix
# -----------------------------------------------------------------------------
RUN apt-get update && apt-get install -y \
    libboost-all-dev libmagic-dev libmatroska-dev libebml-dev \
    libflac-dev libogg-dev libvorbis-dev docbook-xsl xsltproc \
    ruby-dev rake libcmark-dev nlohmann-json3-dev libpugixml-dev \
    libdvdread-dev libpcre2-dev qt6-base-dev qt6-multimedia-dev \
    && rm -rf /var/lib/apt/lists/*

RUN git clone --depth 1 --branch release-84.0 https://gitlab.com/mbunkus/mkvtoolnix.git && \
    cd mkvtoolnix && \
    ./autogen.sh && \
    ./configure --prefix=/usr/local --disable-gui && \
    rake -j$(nproc) && \
    rake install

# -----------------------------------------------------------------------------
# Build av1an
# -----------------------------------------------------------------------------
RUN git clone --depth 1 https://github.com/rust-av/Av1an.git && \
    cd Av1an && \
    cargo build --release && \
    cp target/release/av1an /usr/local/bin/

# -----------------------------------------------------------------------------
# Build the encoding-pipeline application
# -----------------------------------------------------------------------------
COPY . /app
WORKDIR /app

RUN cargo build --release && \
    cp target/release/encoding-pipeline /usr/local/bin/

# =============================================================================
# Stage 2: Runtime image
# =============================================================================
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    python3 libpython3.11 \
    libass9 libfontconfig1 libfreetype6 \
    libopus0 libvpx7 libfdk-aac2 libmp3lame0 \
    libx264-164 libnuma1 \
    libboost-filesystem1.81.0 libboost-system1.81.0 \
    libmagic1 libmatroska7 libebml5 \
    libflac12 libogg0 libvorbis0a libvorbisenc2 \
    libdvdread8 libpcre2-8-0 libpugixml1v5 \
    && rm -rf /var/lib/apt/lists/*

# Copy built binaries and libraries
COPY --from=builder /usr/local /usr/local

# Update library cache
ENV LD_LIBRARY_PATH="/usr/local/lib:/usr/local/lib64"
RUN ldconfig

# Create directories
RUN mkdir -p /config /media/incoming /media/encoded /tmp/encode_pipeline

# Set working directory
WORKDIR /app

# Default entrypoint
ENTRYPOINT ["encoding-pipeline"]
CMD ["run", "--config", "/config/pipeline.yaml"]
