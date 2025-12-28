export class SqlBuilder {
  private sql: string = '';
  private params: any[] = [];

  constructor() {
    this.sql = '';
    this.params = [];
  }

  add(sql: string, params: any[] = []) {
    this.sql += sql;
    this.params.push(...params);
  }

  getSql(): string {
    return this.sql;
  }

  getParams(): any[] {
    return this.params;
  }
}
  