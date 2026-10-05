/**
 * Creating a sidebar enables you to:
 - create an ordered group of docs
 - render a sidebar for each doc of that group
 - provide next/previous navigation

 The sidebars can be generated from the filesystem, or explicitly defined here.

 Create as many sidebars as you want.
 */

// @ts-check

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  tutorialSidebar: [
    {type: 'doc', id: 'index', label: 'Introduction'},
    {type: 'doc', id: 'installation', label: 'Installation'},
    {type: 'doc', id: 'getting-started', label: 'Getting Started'},
    {type: 'doc', id: 'configuration', label: 'Configuration'},
    // Command categories mirror the groups in `candle --help` (rust/src/cli/help.rs).
    {
      type: 'category',
      label: 'Process Management',
      items: [
        {type: 'doc', id: 'commands/list', label: 'list / ls'},
        {type: 'doc', id: 'commands/ps', label: 'ps / status'},
        {type: 'doc', id: 'commands/start', label: 'start / run'},
        {type: 'doc', id: 'commands/restart', label: 'restart'},
        {type: 'doc', id: 'commands/kill', label: 'kill / stop'},
      ],
    },
    {
      type: 'category',
      label: 'Port Detection',
      items: [
        {type: 'doc', id: 'commands/list-ports', label: 'list-ports'},
        {type: 'doc', id: 'commands/open-browser', label: 'open-browser'},
      ],
    },
    {
      type: 'category',
      label: 'Logs',
      items: [
        {type: 'doc', id: 'commands/logs', label: 'logs'},
        {type: 'doc', id: 'commands/watch', label: 'watch'},
        {type: 'doc', id: 'commands/wait-for-log', label: 'wait-for-log'},
      ],
    },
    {
      type: 'category',
      label: 'Configuration',
      items: [
        {type: 'doc', id: 'commands/setup-project', label: 'setup-project'},
        {type: 'doc', id: 'commands/add-service', label: 'add-service'},
        {type: 'doc', id: 'commands/remove-service', label: 'remove-service'},
        {type: 'doc', id: 'commands/set-config', label: 'set-config'},
      ],
    },
    {
      type: 'category',
      label: 'Documentation',
      items: [
        {type: 'doc', id: 'commands/list-docs', label: 'list-docs'},
        {type: 'doc', id: 'commands/get-doc', label: 'get-doc'},
      ],
    },
    {
      type: 'category',
      label: 'Troubleshooting & Maintenance',
      items: [
        {type: 'doc', id: 'commands/list-all', label: 'list-all'},
        {type: 'doc', id: 'commands/kill-all', label: 'kill-all'},
        {type: 'doc', id: 'commands/find-orphans', label: 'find-orphans'},
        {type: 'doc', id: 'commands/list-ports-all', label: 'list-ports-all'},
        {type: 'doc', id: 'commands/clear-logs', label: 'clear-logs'},
        {type: 'doc', id: 'commands/erase-database', label: 'erase-database'},
      ],
    },
    {
      type: 'category',
      label: 'Other',
      items: [
        {type: 'doc', id: 'commands/help', label: 'help'},
        {type: 'doc', id: 'commands/mcp', label: 'mcp'},
      ],
    },
    {type: 'doc', id: 'project-organization', label: 'Project Organization'},
    {type: 'doc', id: 'project-dir', label: 'Targeting Another Project'},
    {type: 'doc', id: 'mcp-integration', label: 'MCP Integration'},
  ],
};

module.exports = sidebars;
