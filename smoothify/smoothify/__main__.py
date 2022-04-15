"""
Smoothify CLI entrypoint
"""
import asyncio

import click
from tekore import Spotify

from smoothify.core import SmoothifyManager
from smoothify.optim.kd_tree import KDTreeBottleneckTSP


@click.group(invoke_without_command=True)
@click.pass_context
def cli(ctx: click.Context):
    click.echo("Welcome to smoothify!")
    click.echo("This CLI is a work in progress.")
    if not ctx.invoked_subcommand:
        click.echo("Please specify a subcommand.")
        click.echo("To see how to use this CLI, run with --help.")


@cli.command()
@click.argument("access_token", type=str)
def library(access_token: str):
    """
    Smoothify your library
    """
    spotify = Spotify(
        access_token, asynchronous=True, max_limits_on=True, chunked_on=True
    )
    manager = SmoothifyManager(spotify=spotify, smoothifier=KDTreeBottleneckTSP())
    asyncio.run(manager.smoothify_user_library())


@cli.command()
@click.argument("access_token", type=str)
@click.argument("playlist_id", type=str)
def playlist(access_token: str, playlist_id: str):
    """
    Smoothify a specific playlist
    """
    spotify = Spotify(
        access_token, asynchronous=True, max_limits_on=True, chunked_on=True
    )
    manager = SmoothifyManager(spotify=spotify, smoothifier=KDTreeBottleneckTSP())
    asyncio.run(manager.smoothify_playlist(playlist_id))
