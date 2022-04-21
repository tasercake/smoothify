#!/usr/bin/env bash

set -ex

pytest --cov=server --cov-report=term-missing server/tests "${@}"
