// Console.log wrapper for collecting logs
export class ConsoleLogInterceptor {
  private collectedLogs: string[] = [];
  private originalConsoleLog = console.log;
  private isInstalled = false;

  install() {
    if (this.isInstalled) return;

    console.log = (...args: any[]) => {
      const logMessage = args
        .map(arg => (typeof arg === 'string' ? arg : JSON.stringify(arg)))
        .join(' ');
      this.collectedLogs.push(logMessage);
    };

    this.isInstalled = true;
  }

  remove() {
    console.log = this.originalConsoleLog;
    this.isInstalled = false;
  }

  takeLogs(): string[] {
    const logs = [...this.collectedLogs];
    this.collectedLogs = [];
    return logs;
  }
}
