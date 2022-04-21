#!/usr/bin/env bash

set -ex

bash scripts/test-backend.sh --cov-report=html "${@}"
