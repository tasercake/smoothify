#! /usr/bin/env bash
set -e

python ./server/tests_pre_start.py

bash ./scripts/test.sh "$@"
