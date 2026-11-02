# Single source for the contest image; Just reads this default too.
ARG CONTEST_IMAGE=zhouzhouyi/os-contest:20260510@sha256:85dec949df7cef41fd03d30c6ad69f952204540e18d2c62bced9d2e262fef12d
FROM ${CONTEST_IMAGE}

# Only add developer tools; do not replace the contest's QEMU/GCC/firmware.
# SHA256 is from casey/just's official 1.40.0 release SHA256SUMS.
ARG JUST_VERSION=1.40.0
ARG JUST_SHA256=181b91d0ceebe8a57723fb648ed2ce1a44d849438ce2e658339df4f8db5f1263
RUN curl -fL --retry 3 "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-x86_64-unknown-linux-musl.tar.gz" -o /tmp/just.tar.gz \
    && echo "${JUST_SHA256}  /tmp/just.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/just.tar.gz -C /usr/local/bin just \
    && rm /tmp/just.tar.gz

COPY rust-toolchain.toml /tmp/rust-toolchain.toml
RUN cd /tmp && rustup component add rust-analyzer rust-src

# Rename the base image's registry settings to avoid deprecation warning in current cargo version.
RUN mv /root/.cargo/config /root/.cargo/config.toml

# CLI builds set host IDs on Linux; other hosts use these defaults. Dev
# Containers handles Linux ID updates separately when creating its container.
ARG DEV_UID=1000
ARG DEV_GID=1000
RUN usermod --login dev --home /home/dev --move-home ubuntu \
    && groupmod --new-name dev ubuntu \
    && test "$DEV_UID" -gt 0 && test "$DEV_GID" -gt 0 \
    && if ! getent group "$DEV_GID" >/dev/null; then groupmod --gid "$DEV_GID" dev; fi \
    && usermod --gid "$DEV_GID" dev \
    && if [ "$(id -u dev)" != "$DEV_UID" ]; then usermod --uid "$DEV_UID" dev; fi \
    && mkdir -p /home/dev/.rustup/toolchains

# Keep only this project's toolchain in the developer's writable home. Do not
# copy all the unrelated toolchains bundled in the contest image (~12 GiB).
RUN rustc_path=$(cd /tmp && rustup which rustc) \
    && toolchain_dir=$(dirname "$(dirname "$rustc_path")") \
    && mv /root/.cargo /home/dev/.cargo \
    && cp /root/.rustup/settings.toml /home/dev/.rustup/settings.toml \
    && mv "$toolchain_dir" /home/dev/.rustup/toolchains/ \
    && chown -R "dev:${DEV_GID}" /home/dev \
    && rm /tmp/rust-toolchain.toml

ENV HOME=/home/dev \
    CARGO_HOME=/home/dev/.cargo \
    RUSTUP_HOME=/home/dev/.rustup
ENV PATH="/home/dev/.cargo/bin:${PATH}"
USER dev
CMD ["bash"]
