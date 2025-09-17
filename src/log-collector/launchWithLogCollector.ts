import { startShellCommand, Subprocess } from "@andyfischer/subprocess-wrapper";
import { ProjectRootDir } from "../dirs.ts";
import * as Path from "node:path";

export function launchWithLogCollector(commandName: string, projectDir: string) {

    const command = [process.argv[0], Path.join(ProjectRootDir, 'dist', 'main-log-collector.js')];

    const subprocess = startShellCommand(command, {
        spawnOptions: {
            stdio: ['pipe', 'pipe', 'pipe'],
            detached: true,
        },
        onStdout: (line) => {
            //console.log('log-collector stdout: ', line);
        },
        onStderr: (line) => {
            //console.log('log-collector stderr: ', line);
        },
    });

    subprocess.proc.stdin.write(JSON.stringify({
        commandName,
        projectDir,
    }));

    subprocess.proc.stdin.end();
    return subprocess;
}