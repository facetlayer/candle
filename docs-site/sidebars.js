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
    {type: 'doc', id: 'getting-started', label: 'Getting Started'},
    {type: 'doc', id: 'configuration', label: 'Configuration'},
    {
      type: 'category',
      label: 'Commands',
      items: [
        {type: 'doc', id: 'commands/run', label: 'run'},
        {type: 'doc', id: 'commands/start', label: 'start'},
        {type: 'doc', id: 'commands/restart', label: 'restart'},
        {type: 'doc', id: 'commands/kill', label: 'kill'},
        {type: 'doc', id: 'commands/list', label: 'list / ls'},
        {type: 'doc', id: 'commands/logs', label: 'logs'},
        {type: 'doc', id: 'commands/watch', label: 'watch'},
        {type: 'doc', id: 'commands/wait-for-log', label: 'wait-for-log'},
        {type: 'doc', id: 'commands/open-browser', label: 'open-browser'},
      ],
    },
    {
      type: 'category',
      label: 'Maintenance',
      items: [
        {type: 'doc', id: 'commands/clear-logs', label: 'clear-logs'},
        {type: 'doc', id: 'commands/erase-database', label: 'erase-database'},
        {type: 'doc', id: 'commands/kill-all', label: 'kill-all'},
        {type: 'doc', id: 'commands/list-all', label: 'list-all'},
        {type: 'doc', id: 'commands/list-ports', label: 'list-ports'},
        {type: 'doc', id: 'commands/list-ports-all', label: 'list-ports-all'},
      ],
    },
    {
      type: 'category',
      label: 'Configuration Commands',
      items: [
        {type: 'doc', id: 'commands/add-service', label: 'add-service'},
      ],
    },
    {
      type: 'category',
      label: 'Help & Utilities',
      items: [
        {type: 'doc', id: 'commands/help', label: 'help'},
        {type: 'doc', id: 'commands/mcp', label: 'mcp'},
      ],
    },
    {type: 'doc', id: 'project-organization', label: 'Project Organization'},
    {type: 'doc', id: 'database', label: 'Database'},
    {type: 'doc', id: 'mcp-integration', label: 'MCP Integration'},
  ],
};

module.exports = sidebars;
