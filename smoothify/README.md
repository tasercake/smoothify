# Smoothify

This is the core library that provides the combinatorial optimization routines used by Smoothify.

## Getting Started

### Clone This Repository

```bash
# Clone repo from github
git clone https://github.com/tasercake/smoothify
# Change into the smoothify directory
cd smoothify
```

(I haven't gotten around to publishing this project yet, and I'm not sure if I ever will)

### [Optional] Create a Virtual Environment

This isn't strictly necessary, but is good practice when starting a new Python project.

I like to use [pyenv](https://github.com/pyenv/pyenv) with [pyenv-virtualenv](https://github.com/pyenv/pyenv-virtualenv)
to manage environments for my Python projects.

[Anaconda](https://docs.conda.io/projects/conda/en/latest/user-guide/install/index.html) 
or [Miniconda](https://docs.conda.io/en/latest/miniconda.html) are popular alternatives.

### Install Dependencies

```bash
pip install .

# This package can be installed in editable mode
pip install -e .
```

### Run Notebook

First, the notebook server:

```bash
jupyter lab
```

In Jupyter Lab, open up `notebooks/K-D Tree.ipynb` and follow the instructions to get a smoothified playlist.
