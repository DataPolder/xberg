# AI-RULEZ :: GENERATED FILE — DO NOT EDIT
# Content-Hash: blake3:0e17ccbd3652cd0f7f07b3b26eb28e3c2944109c5f4d71415198822f2e862a9b
# Source-Hash: blake3:ec177e558e2839c3e8f6610cfccdf9bd49cc40d3fea83e0d9bc4f86ea18d9922
# Schema-Version: v1

"""Hermes adapter for xberg.

This generated no-op keeps the plugin loadable without inventing runtime behavior.
To add Hermes tools, hooks, commands, or other registrations:

1. Create .ai-rulez/hermes/index.py.
2. Implement register(ctx) in that user-owned source file.
3. Run ai-rulez generate --plugin.

Project-local Hermes plugins are trusted code. Enable them explicitly with
HERMES_ENABLE_PROJECT_PLUGINS=true and validate all external input.
"""


def register(ctx):
    """Register this plugin with Hermes Agent."""
    del ctx
