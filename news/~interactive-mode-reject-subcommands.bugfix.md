Typing a `client` or `daemon` subcommand at the interactive-mode
prompt is now rejected with an explanatory message instead of being
run. Interactive mode only accepts cssh options and positional
arguments (host names and cluster tags); subcommands are meant to be
invoked by running the binary directly.
