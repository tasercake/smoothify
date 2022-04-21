#!/usr/bin/env bash

set -ex

pytest --cov=app --cov-report=term-missing app/tests "${@}"
