import { startShellCommand, Subprocess } from "@facetlayer/subprocess-wrapper";
import { ProjectRootDir } from "../dirs.ts";
import * as Path from "node:path";

export async function launchWithLogCollector(commandName: string, projectDir: string) {

    const command = process.argv[0];
    const args = [Path.join(ProjectRootDir, 'dist', 'main-log-collector.js')];

    const subprocess = startShellCommand(command, args, {
        stdio: ['pipe', 'pipe', 'pipe'],
        detached: true,
        onStdout: (line) => {
            //console.log('log-collector stdout: ', line);
        },
        onStderr: (line) => {
            //console.log('log-collector stderr: ', line);
        },
    });

    await subprocess.waitForStart();

    subprocess.proc.stdin.write(JSON.stringify({
        commandName,
        projectDir,
    }));

    subprocess.proc.stdin.end();
    return subprocess;
}
