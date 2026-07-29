"""
pyrustapi alias package re-exporting rustapi.
Allows both `import rustapi` and `import pyrustapi`.
"""
from rustapi import *  # noqa: F401, F403
from rustapi import __all__, __version__  # noqa: F401

