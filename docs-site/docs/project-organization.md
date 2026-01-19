# Project Organization

Candle services are all organized by "project directory". The project directory is the directory
that has a .candle.json file.

For example, let's say you have a `.candle.json` file in `~/projects/my-project-1`, and you have
another `.candle.json` file in `~/projects/my-project-2`. You can then use the standard Candle
commands (like `candle run`, `candle ls`, `candle logs`) in `~/projects/my-project-1`, and it
will only show services that were started from that directory. And running similar commands in
`~/projects/my-project-2` will show services from *that* directory.

## Parent Directories

If there isn't a `.candle.json` file in the current directory, then Candle will search parent directories
to find it.

This works just like other popular tools like Git, where you can use the CLI while in subdirectories and
it works fine.

Example:

```
cd ~/projects/my-project-1/web/src
candle ls                           # Shows Candle services for ~/projects/my-project-1/
```

## Git Worktrees

Since Git worktrees are different directories, then using Candle with a worktree will
let you launch a different set of services than the main Git folder. This means that your worktrees can be developed on independently, which was a huge benefit and motivation for this tool in the first place!

## Global Commands

An exception to the project directory rule: there are a few Candle commands which work with all services across your system, regardless of project directory. These are not frequently used.

 - `list-all` - List all running services on the system
 - `kill-all` - Kill all running services on the system


