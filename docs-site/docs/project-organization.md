# Project Organization

Candle services are all organized by "project directory". This is the directory
that has a .candle.json file.

If there isn't a `.candle.json` file in the current directory, then Candle will search parent directories
to find it.

Example:

```
cd ~/projects/my-project-1/web/src
candle ls                           # Shows Candle services for ~/projects/my-project-1/
```

## Global Commands

An exception to the project directory rule: there are a few Candle commands which work with all services across your system, regardless of project directory. These are not frequently used:

 - `list-all` - List all running services on the system
 - `kill-all` - Kill all running services on the system
 - `list-ports-all` - List open ports for all running services on the system
 - `find-orphans` - List running services whose project no longer accounts for them


