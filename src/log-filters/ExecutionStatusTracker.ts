import { ProcessLog } from "../logs/processLogs.ts";
import { ProcessLogType } from "../logs/ProcessLogType.ts";

interface ExecutionStatus {
    latestLifecycleEvent:
        | typeof ProcessLogType.process_start_initiated
        | typeof ProcessLogType.process_start_failed
        | typeof ProcessLogType.process_started
        | typeof ProcessLogType.process_exited;
}

const lifecycleEventTypes = new Set<number>([
    ProcessLogType.process_start_initiated,
    ProcessLogType.process_start_failed,
    ProcessLogType.process_started,
    ProcessLogType.process_exited,
]);

export class ExecutionStatusTracker {
    private executionStatus: Map<string, ExecutionStatus> = new Map();

    apply(logs: ProcessLog[]): void {
        for (const log of logs) {
            const commandName = log.command_name;
            if (lifecycleEventTypes.has(log.log_type)) {
                this.executionStatus.set(commandName, { latestLifecycleEvent: log.log_type as ExecutionStatus['latestLifecycleEvent'] });
            }
        }
    }

    countRunningProcesses(): number {
        const runningProcesses = new Set<string>();
        for (const [commandName, executionStatus] of this.executionStatus.entries()) {
            if (executionStatus.latestLifecycleEvent === ProcessLogType.process_started
                || executionStatus.latestLifecycleEvent === ProcessLogType.process_start_initiated) {
                runningProcesses.add(commandName);
            }
        }
        return runningProcesses.size;
    }
}