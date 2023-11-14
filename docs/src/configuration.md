# Configuration

Surfer is designed to be configurable to your liking. Configurations are loaded in order from

- The default config
- `~/.config/surfer/config.toml`
- `surfer.toml` in the current working directory
- Environment variables

Configuration keys are overridden by later configs if present, otherwise they
are inherited.
For example, if `~/.config/surfer/config.toml` sets `line-height = 5` it will override
the `line-height` setting in the default configuration, but leave the rest unchanged.

The next chapter lists the available configuration settings and how they are used.
