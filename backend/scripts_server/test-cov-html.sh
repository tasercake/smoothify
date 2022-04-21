#!/usr/bin/env bash

set -ex

bash scripts/test.sh --cov-report=html "${@}"
