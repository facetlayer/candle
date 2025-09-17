import { readStdinAsJson } from "./log-collector/readStdinJson.ts";
import type { LogCollectorLaunchInfo } from "./log-collector/LogCollectorLaunchInfo.ts";
import { getServiceConfigByName } from "./configFile.ts";
import * as Path from "node:path";  
import { ProcessLogType, saveProcessLog } from "./logs/processLogs.ts";
import { startShellCommand, Subprocess } from "@andyfischer/subprocess-wrapper";
import { createProcessEntry, deleteProcessEntry } from "./database/processTable.ts";

function startService(message: LogCollectorLaunchInfo): Subprocess {

    const { commandName, projectDir } = message;
    const serviceConfig = getServiceConfigByName(commandName, projectDir);
    const shell = serviceConfig.serviceConfig.shell;
    let launchDir = projectDir;
    if (serviceConfig.serviceConfig.root) {
        launchDir = Path.join(projectDir, serviceConfig.serviceConfig.root);
    }

    return startShellCommand(shell, {
        spawnOptions: {
            cwd: launchDir,
        },
        onStdout: (line) => {
            saveProcessLog({
                command_name: commandName,
                project_dir: projectDir,
                content: line,
                log_type: ProcessLogType.stdout,
            });
        },
        onStderr: (line) => {
            saveProcessLog({
                command_name: commandName,
                project_dir: projectDir,
                content: line,
                log_type: ProcessLogType.stderr,
            });
        },
    });
}

async function main() {
    const launchInfo: LogCollectorLaunchInfo = await readStdinAsJson();

    console.log('Got launchInfo: ', launchInfo);

    const subprocess = startService(launchInfo);

    console.log('Launched service: ', subprocess.proc.pid);

    createProcessEntry({
        commandName: launchInfo.commandName,
        projectDir: launchInfo.projectDir,
        pid: subprocess.proc.pid,
        logCollectorPid: process.pid,
    });

    try {
        await subprocess.waitForStart();
    } catch (error) {
        console.error('Process failed to start: ', error);
        saveProcessLog({
            command_name: launchInfo.commandName,
            project_dir: launchInfo.projectDir,
            log_type: ProcessLogType.process_start_failed,
            content: "Process failed to start: " + error.message,
        });
        process.exit(1);
    }

    saveProcessLog({
        command_name: launchInfo.commandName,
        project_dir: launchInfo.projectDir,
        log_type: ProcessLogType.process_started,
    });

    await subprocess.waitForExit();

    console.log('process exited, cleaning up');

    saveProcessLog({
        command_name: launchInfo.commandName,
        project_dir: launchInfo.projectDir,
        log_type: ProcessLogType.process_exited,
        content: "Process exited with code " + subprocess.proc.exitCode,
    });

    deleteProcessEntry({
        commandName: launchInfo.commandName,
        projectDir: launchInfo.projectDir,
        pid: subprocess.proc.pid,
    });
}

main().catch(error => {
    console.error(error);
    process.exit(1);
});