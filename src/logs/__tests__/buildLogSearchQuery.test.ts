import { describe, it, expect } from 'vitest';
import { buildLogSearchQuery } from '../buildLogSearchQuery.ts';

describe('buildLogSearchQuery', () => {
  it('builds query with projectDir and single commandName', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandNames: ['myCommand'],
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.project_dir = ? and po.command_name = ? order by po.timestamp desc, po.id desc'
    );
    expect(builder.getParams()).toEqual(['/path/to/project', 'myCommand']);
  });

  it('builds query with projectDir and multiple commandNames using IN clause', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandNames: ['cmd1', 'cmd2', 'cmd3'],
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.project_dir = ? and po.command_name in (?, ?, ?) order by po.timestamp desc, po.id desc'
    );
    expect(builder.getParams()).toEqual(['/path/to/project', 'cmd1', 'cmd2', 'cmd3']);
  });

  it('builds query with only projectDir (uses default command)', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandNames: [],
    });

    expect(builder.getSql()).toBe(
      "select po.* from process_output po where po.project_dir = ? and po.command_name = 'default' order by po.timestamp desc, po.id desc"
    );
    expect(builder.getParams()).toEqual(['/path/to/project']);
  });

  it('builds query with only single commandName', () => {
    const builder = buildLogSearchQuery({
      projectDir: undefined as any,
      commandNames: ['myCommand'],
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.command_name = ? order by po.timestamp desc, po.id desc'
    );
    expect(builder.getParams()).toEqual(['myCommand']);
  });

  it('builds query with only multiple commandNames using IN clause', () => {
    const builder = buildLogSearchQuery({
      projectDir: undefined as any,
      commandNames: ['cmd1', 'cmd2'],
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.command_name in (?, ?) order by po.timestamp desc, po.id desc'
    );
    expect(builder.getParams()).toEqual(['cmd1', 'cmd2']);
  });

  it('throws error when neither projectDir nor commandNames provided', () => {
    expect(() =>
      buildLogSearchQuery({
        projectDir: undefined as any,
        commandNames: [],
      })
    ).toThrow('Must provide projectDir or commandNames');
  });

  it('throws error when commandNames is undefined', () => {
    expect(() =>
      buildLogSearchQuery({
        projectDir: undefined as any,
        commandNames: undefined as any,
      })
    ).toThrow('Must provide projectDir or commandNames');
  });

  it('adds sinceTimestamp filter', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandNames: ['myCommand'],
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
      commandNames: ['myCommand'],
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
      commandNames: ['myCommand'],
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
      commandNames: ['myCommand'],
      sinceTimestamp: 1234567890,
      afterLogId: 42,
      limit: 100,
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.project_dir = ? and po.command_name = ? and po.timestamp > ? and po.id > ? order by po.timestamp desc, po.id desc limit ?'
    );
    expect(builder.getParams()).toEqual(['/path/to/project', 'myCommand', 1234567890, 42, 100]);
  });

  it('combines multiple commandNames with all optional filters', () => {
    const builder = buildLogSearchQuery({
      projectDir: '/path/to/project',
      commandNames: ['cmd1', 'cmd2'],
      sinceTimestamp: 1234567890,
      afterLogId: 42,
      limit: 100,
    });

    expect(builder.getSql()).toBe(
      'select po.* from process_output po where po.project_dir = ? and po.command_name in (?, ?) and po.timestamp > ? and po.id > ? order by po.timestamp desc, po.id desc limit ?'
    );
    expect(builder.getParams()).toEqual(['/path/to/project', 'cmd1', 'cmd2', 1234567890, 42, 100]);
  });
});
