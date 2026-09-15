FROM rust:1.88-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY adapters ./adapters
RUN cargo build --locked -p ironlint-cli && cp target/debug/ironlint /ironlint

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends git jq \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 1000 tester
COPY --from=build /ironlint /usr/local/bin/ironlint
COPY scripts/verify-acceptance.sh /opt/ironlint-tests/verify-acceptance.sh
COPY tests/e2e/features/driver.sh /usr/local/bin/ironlint-fixture-harness
COPY tests/e2e/features/test.sh /opt/ironlint-tests/test.sh
RUN chmod +x /usr/local/bin/ironlint-fixture-harness
USER tester
ENV HOME=/home/tester XDG_CONFIG_HOME=/home/tester/.config
ENTRYPOINT ["bash", "/opt/ironlint-tests/test.sh"]
