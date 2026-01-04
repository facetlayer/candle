export interface MonitoredProcess {
  pid: number;
  getExitCode(): number | null;
  waitForStart(): Promise<void>;
  waitForExit(): Promise<void>;
}
