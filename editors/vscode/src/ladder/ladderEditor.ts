import * as vscode from "vscode";
import * as path from "path";
import type { LadderProgram } from "./ladderEngine";

type ExecutionMode = "simulation" | "hardware";

interface ExecutionState {
  program: LadderProgram;
  mode: ExecutionMode;
  isRunning: boolean;
}

const WEBVIEW_HTML_TEMPLATE = `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta
      http-equiv="Content-Security-Policy"
      content="default-src 'none'; img-src {{cspSource}} data: https:; style-src {{cspSource}} 'unsafe-inline'; script-src {{cspSource}} 'unsafe-eval' 'unsafe-inline';"
    />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Ladder Logic Editor</title>
    <link rel="stylesheet" href="{{webviewStyle}}" />
    <style>
      * {
        box-sizing: border-box;
        margin: 0;
        padding: 0;
      }

      html,
      body,
      #root {
        width: 100%;
        height: 100%;
        overflow: hidden;
        font-family: var(
          --vscode-font-family,
          -apple-system,
          BlinkMacSystemFont,
          "Segoe UI",
          Roboto,
          Oxygen,
          Ubuntu,
          Cantarell,
          sans-serif
        );
        background-color: var(--vscode-editor-background);
        color: var(--vscode-editor-foreground);
      }
    </style>
  </head>
  <body>
    <div id="root"></div>
    <script src="{{webviewScript}}"></script>
  </body>
</html>
`;

/**
 * Provider for Ladder Logic visual programming editor
 */
export class LadderEditorProvider implements vscode.CustomTextEditorProvider {
  private static readonly viewType = "trust-lsp.ladder.editor";
  private activeExecutions = new Map<string, ExecutionState>();

  public static register(context: vscode.ExtensionContext): vscode.Disposable {
    const provider = new LadderEditorProvider(context);
    const providerRegistration = vscode.window.registerCustomEditorProvider(
      LadderEditorProvider.viewType,
      provider,
      {
        webviewOptions: {
          retainContextWhenHidden: true,
        },
      }
    );
    return providerRegistration;
  }

  constructor(private readonly context: vscode.ExtensionContext) {}

  /**
   * Called when a custom editor is opened
   */
  public async resolveCustomTextEditor(
    document: vscode.TextDocument,
    webviewPanel: vscode.WebviewPanel,
    _token: vscode.CancellationToken
  ): Promise<void> {
    const docId = document.uri.toString();

    // Setup webview
    webviewPanel.webview.options = {
      enableScripts: true,
      localResourceRoots: [
        vscode.Uri.file(path.join(this.context.extensionPath, "media")),
      ],
    };

    // Set initial HTML content
    webviewPanel.webview.html = this.getHtmlForWebview(webviewPanel.webview);

    // Handle messages from webview
    webviewPanel.webview.onDidReceiveMessage(async (message) => {
      switch (message.type) {
        case "save":
          await this.saveProgram(document, message.program);
          break;

        case "runSimulation":
          await this.runSimulation(docId, message.program);
          break;

        case "runHardware":
          await this.runHardware(docId, message.program);
          break;

        case "stop":
          await this.stopExecution(docId);
          break;

        case "ready":
          // Webview is ready, send initial program data
          const program = this.loadProgram(document);
          webviewPanel.webview.postMessage({
            type: "loadProgram",
            program,
          });
          break;
      }
    });

    // Handle document changes
    const changeDocumentSubscription = vscode.workspace.onDidChangeTextDocument(
      (e) => {
        if (e.document.uri.toString() === document.uri.toString()) {
          const program = this.loadProgram(document);
          webviewPanel.webview.postMessage({
            type: "loadProgram",
            program,
          });
        }
      }
    );

    // Cleanup on panel dispose
    webviewPanel.onDidDispose(() => {
      changeDocumentSubscription.dispose();
      this.stopExecution(docId);
      this.activeExecutions.delete(docId);
    });
  }

  /**
   * Load ladder program from document
   */
  private loadProgram(document: vscode.TextDocument): LadderProgram {
    const text = document.getText();
    if (!text.trim()) {
      // Return empty program if document is empty
      return {
        rungs: [],
        variables: [],
        metadata: {
          name: "New Ladder Program",
          description: "Ladder logic program",
        },
      };
    }

    try {
      return JSON.parse(text);
    } catch (error) {
      vscode.window.showErrorMessage(`Failed to parse ladder program: ${error}`);
      return {
        rungs: [],
        variables: [],
        metadata: {
          name: "Error",
          description: "Failed to parse",
        },
      };
    }
  }

  /**
   * Save ladder program to document
   */
  private async saveProgram(
    document: vscode.TextDocument,
    program: LadderProgram
  ): Promise<void> {
    const json = JSON.stringify(program, null, 2);
    const edit = new vscode.WorkspaceEdit();

    edit.replace(
      document.uri,
      new vscode.Range(0, 0, document.lineCount, 0),
      json
    );

    await vscode.workspace.applyEdit(edit);
    await document.save();
    
    vscode.window.showInformationMessage("Ladder program saved");
  }

  /**
   * Run in simulation mode
   */
  private async runSimulation(docId: string, program: LadderProgram): Promise<void> {
    console.log("[Ladder] Starting simulation mode");
    
    this.activeExecutions.set(docId, {
      program,
      mode: "simulation",
      isRunning: true,
    });

    // TODO: Implement ladder interpreter for simulation
    vscode.window.showInformationMessage(
      "Ladder simulation mode - interpreter not yet implemented"
    );
  }

  /**
   * Run in hardware mode
   */
  private async runHardware(docId: string, program: LadderProgram): Promise<void> {
    console.log("[Ladder] Starting hardware mode");

    this.activeExecutions.set(docId, {
      program,
      mode: "hardware",
      isRunning: true,
    });

    // TODO: Implement hardware execution via RuntimeClient
    vscode.window.showInformationMessage(
      "Ladder hardware mode - RuntimeClient integration not yet implemented"
    );
  }

  /**
   * Stop execution
   */
  private async stopExecution(docId: string): Promise<void> {
    const state = this.activeExecutions.get(docId);
    if (!state) return;

    console.log("[Ladder] Stopping execution");
    state.isRunning = false;

    // TODO: Stop interpreter/runtime client
    
    this.activeExecutions.delete(docId);
  }

  /**
   * Get HTML content for webview
   */
  private getHtmlForWebview(webview: vscode.Webview): string {
    const scriptUri = webview.asWebviewUri(
      vscode.Uri.file(
        path.join(this.context.extensionPath, "media", "ladderWebview.js")
      )
    );

    const styleUri = webview.asWebviewUri(
      vscode.Uri.file(
        path.join(this.context.extensionPath, "media", "ladderWebview.css")
      )
    );

    const cspSource = webview.cspSource;

    return WEBVIEW_HTML_TEMPLATE.replace(/{{cspSource}}/g, cspSource)
      .replace("{{webviewScript}}", scriptUri.toString())
      .replace("{{webviewStyle}}", styleUri.toString());
  }
}
