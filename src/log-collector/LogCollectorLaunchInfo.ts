export interface LogCollectorLaunchInfo {
  commandName: string;
  projectDir: string;
  shell: string;
  root?: string;
  usePty?: boolean;
}
