import {
    workspace,
    ExtensionContext,
} from "vscode";

import {
    Executable,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

export async function activate(context: ExtensionContext) {
    const command = process.env.SERVER_PATH || "angular-lsp";
    const run: Executable = {
        command,
        options: {
            env: {
                ...process.env,
                // eslint-disable-next-line @typescript-eslint/naming-convention
                RUST_LOG: "debug",
            },
        },
    };
    const serverOptions: ServerOptions = {
        run,
        debug: run,
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [ "html" ],
        synchronize: {
            fileEvents: workspace.createFileSystemWatcher("**/*.ts"),
        },
    };

    // Create the language client and start the client.
    client = new LanguageClient("angular-language-server", "Angular language server", serverOptions, clientOptions);
    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
