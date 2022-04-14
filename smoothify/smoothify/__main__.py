"""
Smoothify CLI entrypoint
"""

import click

from smoothify.core import SmoothifyManager


@click.command()
def main():
    print("Welcome to smoothify!")
    print("This CLI is a work in progress.")
    manager = SmoothifyManager()
