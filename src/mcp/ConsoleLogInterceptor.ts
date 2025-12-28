// Console.log wrapper for collecting logs
export class ConsoleLogInterceptor {
  private collectedLogs: string[] = [];
  private originalConsoleLog = console.log;
  private originalConsoleError = console.error;
  private isInstalled = false;

  install() {
    if (this.isInstalled) return;

    console.log = (...args: any[]) => {
      const logMessage = args
        .map(arg => (typeof arg === 'string' ? arg : JSON.stringify(arg)))
        .join(' ');
      this.collectedLogs.push(logMessage);
    };

    console.error = (...args: any[]) => {
      const logMessage = args
        .map(arg => (typeof arg === 'string' ? arg : JSON.stringify(arg)))
        .join(' ');
      this.collectedLogs.push(`[stderr] ${logMessage}`);
    };

    this.isInstalled = true;
  }

  remove() {
    console.log = this.originalConsoleLog;
    console.error = this.originalConsoleError;
    this.isInstalled = false;
  }

  takeLogs(): string[] {
    const logs = [...this.collectedLogs];
    this.collectedLogs = [];
    return logs;
  }
}
