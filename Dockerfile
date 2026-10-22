# syntax=docker/dockerfile:1.27@sha256:4edf897a3ffa55b89f906fc8cc78afdb3f1834cc9c7083565e611a8a7d5fe99e
# Stages: dev is the development environment, check is the single gate, build compiles the
# release binary, and runtime is the deployable image.

ARG COMPONENT=veoveo-hub

FROM rust:1.99.0-trixie@sha256:6ff07edce8775d0f64be7aba9197229407301bddf2054d62c27b541a6238a181 AS dev
# PostgreSQL 18.6 from the PGDG repository, whose signing key Debian's postgresql-common
# ships. Tests start their own clusters, so the packages create none.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates clang cmake git protobuf-compiler libprotobuf-dev pkg-config procps postgresql-common \
    && /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh -y \
    && mkdir -p /etc/postgresql-common/createcluster.d \
    && printf 'create_main_cluster = false\n' > /etc/postgresql-common/createcluster.d/veoveo.conf \
    && apt-get install -y --no-install-recommends \
        postgresql-18=18.6-1.pgdg13+2 postgresql-client-18=18.6-1.pgdg13+2 libpq5=18.6-1.pgdg13+2 \
    && rm -rf /var/lib/apt/lists/* /var/log/apt /var/log/dpkg.log \
    && rustup component add clippy rustfmt
ENV PATH=/usr/lib/postgresql/18/bin:${PATH}
# OpenJDK 21 from Debian runs the Keycloak distribution below.
RUN apt-get update \
    && apt-get install -y --no-install-recommends openjdk-21-jre-headless=21.0.12.1+1-1~deb13u1 \
    && rm -rf /var/lib/apt/lists/* /var/log/apt /var/log/dpkg.log
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo install --locked \
        cargo-nextest@0.9.148 \
        cargo-deny@0.20.2 \
        cargo-hack@0.6.45 \
        cargo-semver-checks@0.51.0 \
        cargo-insta@1.49.0 \
    && cargo install --locked sqlx-cli@0.9.0 --no-default-features --features postgres,rustls,sqlx-toml
COPY env/profile.d/rust.sh /etc/profile.d/rust.sh
# A development environment installs toolchains, components and crates, so the sandbox user owns them.
RUN useradd --create-home --uid 1000 --shell /bin/bash sandbox \
    && chown -R sandbox:sandbox /usr/local/rustup /usr/local/cargo
# Keycloak 26.8.1, the OpenID provider browser-session tests log in through. Each test
# process starts a private server from this distribution, built once here for an
# in-memory database, as tests start PostgreSQL.
COPY --from=quay.io/keycloak/keycloak:26.8.1@sha256:79c623a22abacf3fe6c00a5fb30f5b7843d717b234033b80995891c06116d244 \
    --chown=sandbox:sandbox /opt/keycloak /opt/keycloak
USER sandbox
RUN /opt/keycloak/bin/kc.sh build --db=dev-mem

FROM dev AS source
WORKDIR /home/sandbox/src
COPY --chown=sandbox:sandbox . .

FROM source AS check
# The committed .sqlx query metadata must match the Hub's queries against the kernel
# migrations of the pinned veoveo-core checkout followed by the Hub's own migrations.
RUN cargo fetch --locked \
    && store="$(echo /usr/local/cargo/git/checkouts/veoveo-core-*/*/platform/store)" \
    && hub=crates/veoveo-hub \
    && initdb --pgdata /tmp/sqlx/data --username postgres --auth=trust --no-instructions --no-sync \
    && pg_ctl start --pgdata /tmp/sqlx/data --wait --silent \
        --options "-c listen_addresses=127.0.0.1 -c unix_socket_directories=/tmp/sqlx -c fsync=off" \
    && createdb --host 127.0.0.1 --username postgres veoveo \
    && DATABASE_URL=postgres://postgres@127.0.0.1/veoveo cargo sqlx migrate run \
        --source "${store}/migrations" --config "${store}/sqlx.toml" \
    && DATABASE_URL=postgres://postgres@127.0.0.1/veoveo cargo sqlx migrate run \
        --source "${hub}/migrations" --config "${hub}/sqlx.toml" \
    && DATABASE_URL=postgres://postgres@127.0.0.1/veoveo cargo sqlx prepare --workspace --all --check \
        -- --all-targets --all-features \
    && pg_ctl stop --pgdata /tmp/sqlx/data --mode fast --wait --silent \
    && rm -rf /tmp/sqlx
RUN cargo fmt --check \
    && cargo clippy --locked --all-targets --all-features -- -D warnings \
    && cargo deny --locked check \
    && cargo hack check --each-feature --no-dev-deps \
    && cargo nextest run --locked --profile ci

FROM source AS build
ARG COMPONENT
RUN cargo build --locked --release --bin "${COMPONENT}" \
    && cp "target/release/${COMPONENT}" /home/sandbox/component

FROM debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f AS runtime
ARG COMPONENT
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libstdc++6 \
    && rm -rf /var/lib/apt/lists/* /var/log/apt /var/log/dpkg.log \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo component \
    && mkdir -p /etc/veoveo \
    && chown -R component:component /etc/veoveo /var/lib/veoveo
COPY --from=build /home/sandbox/component /usr/local/bin/veoveo-hub
USER component
# A container listens on every interface; a sandbox keeps the loopback default.
ENV VEOVEO_LISTEN_ADDRESS=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/veoveo-hub"]
CMD ["serve"]
LABEL org.opencontainers.image.source="https://github.com/VeoveoAI/veoveo-hub" \
      org.opencontainers.image.licenses="Apache-2.0"
