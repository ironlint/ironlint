# syntax=docker/dockerfile:1
#
# Onboarding feature test image for `ironlint init`.
#
# The host is typically macOS, so a host-built `ironlint` is a Mach-O binary that
# can't run in a Linux container. We therefore compile a Linux `ironlint` from
# source in a build stage, then drop it into a minimal runtime stage.
#
# Adapter packages are runtime inputs. Editing them does not invalidate the
# evaluator build layer; the runtime image receives the current packages.

FROM rust:1.88-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY adapters/shared/ironlint-config ./adapters/shared/ironlint-config
RUN cargo build --locked --release -p ironlint-cli && cp target/release/ironlint /ironlint

# The test never executes a live harness. Git is needed by doctor's optional
# hook-path inspection; node/bun and real harness installations are unnecessary.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends git \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 1000 tester
COPY --from=build /ironlint /usr/local/bin/ironlint
COPY adapters /usr/local/bin/adapters
COPY tests/e2e/init/drive.sh /usr/local/bin/drive.sh
USER tester
ENTRYPOINT ["bash", "/usr/local/bin/drive.sh"]
