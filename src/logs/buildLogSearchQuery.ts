import { SqlBuilder } from './SqlBuilder.ts';
import type { LogSearchOptions } from './processLogs.ts';

export function buildLogSearchQuery(options: LogSearchOptions): SqlBuilder {
  const { projectDir, commandName, limit, sinceTimestamp, afterLogId } = options;
  const builder = new SqlBuilder();

  // Prioritize projectDir + commandName approach
  if (projectDir !== undefined && commandName !== undefined) {
    builder.add(
      'select po.* from process_output po where po.project_dir = ? and po.command_name = ?',
      [projectDir, commandName]
    );
  } else if (projectDir !== undefined) {
    builder.add(
      "select po.* from process_output po where po.project_dir = ? and po.command_name = 'default'",
      [projectDir]
    );
  } else if (commandName !== undefined) {
    builder.add('select po.* from process_output po where po.command_name = ?', [commandName]);
  } else {
    throw new Error('Must provide projectDir and commandName');
  }

  if (sinceTimestamp !== undefined) {
    builder.add(' and po.timestamp > ?', [sinceTimestamp]);
  }

  if (afterLogId !== undefined) {
    builder.add(' and po.id > ?', [afterLogId]);
  }

  // Order by 'desc' so that we get the most recent logs first.
  builder.add(' order by po.timestamp desc, po.id desc');

  if (limit !== undefined) {
    builder.add(' limit ?', [limit]);
  }

  return builder;
}
