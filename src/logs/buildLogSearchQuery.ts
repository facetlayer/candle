import { SqlBuilder } from './SqlBuilder.ts';
import type { LogSearchOptions } from './processLogs.ts';

export function buildLogSearchQuery(options: LogSearchOptions): SqlBuilder {
  const { projectDir, commandNames, limit, sinceTimestamp, afterLogId } = options;
  const builder = new SqlBuilder();

  // Prioritize projectDir + commandNames approach
  if (projectDir !== undefined && commandNames !== undefined && commandNames.length > 0) {
    if (commandNames.length === 1) {
      builder.add(
        'select po.* from process_output po where po.project_dir = ? and po.command_name = ?',
        [projectDir, commandNames[0]]
      );
    } else {
      const placeholders = commandNames.map(() => '?').join(', ');
      builder.add(
        `select po.* from process_output po where po.project_dir = ? and po.command_name in (${placeholders})`,
        [projectDir, ...commandNames]
      );
    }
  } else if (projectDir !== undefined) {
    builder.add(
      "select po.* from process_output po where po.project_dir = ? and po.command_name = 'default'",
      [projectDir]
    );
  } else if (commandNames !== undefined && commandNames.length > 0) {
    if (commandNames.length === 1) {
      builder.add('select po.* from process_output po where po.command_name = ?', [commandNames[0]]);
    } else {
      const placeholders = commandNames.map(() => '?').join(', ');
      builder.add(
        `select po.* from process_output po where po.command_name in (${placeholders})`,
        commandNames
      );
    }
  } else {
    throw new Error('Must provide projectDir or commandNames');
  }

  if (sinceTimestamp !== undefined) {
    builder.add(' and po.timestamp > ?', [sinceTimestamp]);
  }

  if (afterLogId != null) {
    builder.add(' and po.id > ?', [afterLogId]);
  }

  // Order by 'desc' so that we get the most recent logs first.
  builder.add(' order by po.timestamp desc, po.id desc');

  if (limit !== undefined) {
    builder.add(' limit ?', [limit]);
  }

  return builder;
}
