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

COPY rust-toolchain.toml /tmp/impl-os-rust-toolchain.toml
RUN toolchain=$(sed -n 's/^channel = "\(.*\)"/\1/p' /tmp/impl-os-rust-toolchain.toml) \
    && rustup component add --toolchain "$toolchain" rust-analyzer rust-src \
    && rm /tmp/impl-os-rust-toolchain.toml

# Rename the base image's registry settings to avoid deprecation warning in current cargo version.
RUN mv /root/.cargo/config /root/.cargo/config.toml

# Reuse the base image's ordinary account; its IDs are only image defaults.
# Dev Containers or entrypoint.sh matches them to each Linux host at runtime.
RUN usermod --login dev --home /home/dev --move-home ubuntu \
    && groupmod --new-name dev ubuntu \
    && mkdir -p /home/dev/.rustup/toolchains

# Keep only this project's toolchain in the developer's writable home. Do not
# copy all the unrelated toolchains bundled in the contest image (~12 GiB).
COPY rust-toolchain.toml /tmp/impl-os-rust-toolchain.toml
RUN toolchain=$(sed -n 's/^channel = "\(.*\)"/\1/p' /tmp/impl-os-rust-toolchain.toml) \
    && mv /root/.cargo /home/dev/.cargo \
    && cp /root/.rustup/settings.toml /home/dev/.rustup/settings.toml \
    && mv "/root/.rustup/toolchains/${toolchain}-x86_64-unknown-linux-gnu" /home/dev/.rustup/toolchains/ \
    && chown -R dev:dev /home/dev \
    && rm /tmp/impl-os-rust-toolchain.toml

ENV HOME=/home/dev \
    CARGO_HOME=/home/dev/.cargo \
    RUSTUP_HOME=/home/dev/.rustup
ENV PATH="/home/dev/.cargo/bin:${PATH}"
COPY .devcontainer/entrypoint.sh /usr/local/bin/impl-os-entrypoint
RUN chmod 755 /usr/local/bin/impl-os-entrypoint
USER dev
ENTRYPOINT ["/usr/local/bin/impl-os-entrypoint"]
CMD ["bash"]
