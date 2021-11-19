# Smoothify

## Getting Started

### Install Poetry

This package uses `poetry` as its build system. You'll need to install it first if you don't already have it.

```bash
pip install poetry
```

### Environment & Dependencies

```bash
poetry install
```

This creates a Python virtual environment and installs all the dependencies required by *Smoothify*.

### Run notebook Server

First, the notebook server:

```bash
poetry run jupyter lab
```

You can then pick a notebook (from the `notebooks` directory) and run it.

For now, open up `notebooks/K-D Tree.ipynb` in Jupyter Lab.

### Run notebook

Grab a Spotify API Access Token for your account [here](https://developer.spotify.com/console/get-current-user-saved-tracks/). Make sure to check the required permissions/scopes when getting your token.

In the notebook, replace `ACCESS_TOKEN` with the token you just got from Spotify.

You should now be able to run all the cells in the notebook, and what you'll get is a list of Spotify Track UIDs.

Copy these Track UIDs, create a new Spotify playlist, and hit <kbd>Ctrl</kbd>+<kbd>V</kbd> to populate the playlist.
