export interface LogCollectorLaunchInfo {
  commandName: string;
  projectDir: string;
  shell: string;
  root?: string;
  enableStdin?: boolean;
  port?: number;
}
