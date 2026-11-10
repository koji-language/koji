// The extension: the syntax of Koji (its grammar), and the language server of `kojic lsp` (the
// errors of a file as it is edited, the type under the cursor, the declaration of a name).
const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

let client;

function activate(context) {
    const command = vscode.workspace.getConfiguration('koji').get('kojicPath') || 'kojic';
    const server = { command, args: ['lsp'] };
    client = new LanguageClient('koji', 'Koji', { run: server, debug: server }, {
        documentSelector: [{ scheme: 'file', language: 'koji' }],
    });
    client.start();
    context.subscriptions.push({ dispose: () => client.stop() });
}

function deactivate() {
    return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
