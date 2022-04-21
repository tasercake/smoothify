#! /usr/bin/env bash

# Let the DB start
python ../server/backend_pre_start.py

# Run migrations
alembic upgrade head

# Create initial data in DB
python ../server/initial_data.py
