import { getServiceInfoByName } from "../configFile.ts";

export function assertValidCommandName(commandName: string) {
    getServiceInfoByName(commandName);
}

export function assertValidCommandNames(commandNames: string[]) {
    for (const commandName of commandNames) {
        assertValidCommandName(commandName);
    }
}