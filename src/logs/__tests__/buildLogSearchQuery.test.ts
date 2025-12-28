import { describe, it, expect } from 'vitest';
import { buildLogSearchQuery } from '../buildLogSearchQuery.ts';

describe('buildLogSearchQuery', () => {
  it('builds query with projectDir and commandName', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandName: 'myCommand',
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.project_dir = ? and po.command_name = ? order by po.timestamp desc, po.id desc'
    );
    expect(builder.getParams()).toEqual(['/path/to/project', 'myCommand']);
  });

  it('builds query with only projectDir (uses default command)', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandName: undefined as any,
    });

    expect(builder.getSql()).toBe(
      "select po.* from process_output po where po.project_dir = ? and po.command_name = 'default' order by po.timestamp desc, po.id desc"
    );
    expect(builder.getParams()).toEqual(['/path/to/project']);
  });

  it('builds query with only commandName', () => {
    const builder = buildLogSearchQuery({
      projectDir: undefined as any,
      commandName: 'myCommand',
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.command_name = ? order by po.timestamp desc, po.id desc'
    );
    expect(builder.getParams()).toEqual(['myCommand']);
  });

  it('throws error when neither projectDir nor commandName provided', () => {
    expect(() =>
      buildLogSearchQuery({
        projectDir: undefined as any,
        commandName: undefined as any,
      })
    ).toThrow('Must provide projectDir and commandName');
  });

  it('adds sinceTimestamp filter', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandName: 'myCommand',
      sinceTimestamp: 1234567890,
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.project_dir = ? and po.command_name = ? and po.timestamp > ? order by po.timestamp desc, po.id desc'
    );
    expect(builder.getParams()).toEqual(['/path/to/project', 'myCommand', 1234567890]);
  });

  it('adds afterLogId filter', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandName: 'myCommand',
      afterLogId: 42,
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.project_dir = ? and po.command_name = ? and po.id > ? order by po.timestamp desc, po.id desc'
    );
    expect(builder.getParams()).toEqual(['/path/to/project', 'myCommand', 42]);
  });

  it('adds limit clause', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandName: 'myCommand',
      limit: 100,
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.project_dir = ? and po.command_name = ? order by po.timestamp desc, po.id desc limit ?'
    );
    expect(builder.getParams()).toEqual(['/path/to/project', 'myCommand', 100]);
  });

  it('combines all optional filters', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandName: 'myCommand',
      sinceTimestamp: 1234567890,
      afterLogId: 42,
      limit: 100,
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.project_dir = ? and po.command_name = ? and po.timestamp > ? and po.id > ? order by po.timestamp desc, po.id desc limit ?'
    );
    expect(builder.getParams()).toEqual(['/path/to/project', 'myCommand', 1234567890, 42, 100]);
  });
});
