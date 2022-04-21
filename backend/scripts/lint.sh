#!/usr/bin/env bash

set -x

mypy server
black server --check
isort --recursive --check-only server
flake8
