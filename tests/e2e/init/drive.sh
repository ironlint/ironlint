#!/usr/bin/env bash
# Runs inside the container against bind-mounted HOME and project directories.
set -euo pipefail

echo "== seeding Pi home for detection =="
mkdir -p "$HOME/.pi"

echo "== scaffolding v1 policy and installing the Pi adapter =="
cd /work
ironlint init --yes

echo "== running the trusted starter policy =="
ironlint check --event accept --format json >/work/acceptance.json

echo "== inspecting the installation =="
ironlint doctor --format json >/work/doctor.json

echo "== drive.sh complete =="
