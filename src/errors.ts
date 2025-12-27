// Error subclasses for specific error types

export class UsageError extends Error {
  isUsageError = true;

  constructor(message: string) {
    super(message);
    this.name = 'UsageError';
  }
}

export class ConfigFileError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ConfigFileError';
  }
}

export class MissingServiceWithNameError extends Error {
  cwd: string;
  commandName: string;
  isUsageError = true;

  constructor(cwd: string, commandName: string) {
    super(`No service '${commandName}' configured for directory: ${cwd}`);
    this.name = 'NeedRunCommandError';
    this.cwd = cwd;
    this.commandName = commandName;
  }
}

export class MissingSetupFileError extends Error {
  cwd: string;
  isUsageError = true;

  constructor(cwd: string) {
    super(`No .candle.json file found in (or above) current directory: ${cwd}`);
    this.name = 'MissingSetupFile';
    this.cwd = cwd;
  }
}
