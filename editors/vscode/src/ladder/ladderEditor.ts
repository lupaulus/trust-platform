import * as vscode from "vscode";
import * as path from "path";
import type { LadderProgram } from "./ladderEngine.types";
import { LadderEngine } from "./ladderEngine";
import { RuntimeClient, getRuntimeConfig } from "../statechart/runtimeClient";

type ExecutionMode = "simulation" | "hardware";

interface ExecutionState {
  program: LadderProgram;
  mode: ExecutionMode;
  isRunning: boolean;
  engine: LadderEngine;
  webviewPanel: vscode.WebviewPanel;
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
    <script>
      const vscode = acquireVsCodeApi();
    </script>
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
    console.log("[Ladder] resolveCustomTextEditor called for:", docId);

    // Setup webview
    webviewPanel.webview.options = {
      enableScripts: true,
      localResourceRoots: [
        vscode.Uri.file(path.join(this.context.extensionPath, "media")),
      ],
    };

    // Set initial HTML content
    const html = this.getHtmlForWebview(webviewPanel.webview);
    console.log("[Ladder] Setting webview HTML, length:", html.length);
    webviewPanel.webview.html = html;

    // Handle messages from webview
    webviewPanel.webview.onDidReceiveMessage(async (message) => {
      console.log("[Ladder] Received message from webview:", message.type);
      switch (message.type) {
        case "save":
          await this.saveProgram(document, message.program);
          break;

        case "runSimulation":
          await this.runSimulation(docId, message.program, webviewPanel);
          break;

        case "runHardware":
          await this.runHardware(docId, message.program, webviewPanel);
          break;

        case "stop":
          await this.stopExecution(docId);
          break;

        case "ready":
          // Webview is ready, send initial program data
          console.log("[Ladder] Webview ready, loading program...");
          const program = this.loadProgram(document);
          console.log("[Ladder] Loaded program with", program.rungs.length, "rungs");
          console.log("[Ladder] Sending loadProgram message to webview");
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
  private async runSimulation(docId: string, program: LadderProgram, webviewPanel: vscode.WebviewPanel): Promise<void> {
    console.log("[Ladder] Starting simulation mode");
    
    // Create ladder engine
    const engine = new LadderEngine(program, "simulation", {
      scanCycleMs: 100, // 100ms scan cycle
    });

    // Set up state change callback to send updates to webview
    engine.setStateChangeCallback((state) => {
      webviewPanel.webview.postMessage({
        type: "stateUpdate",
        state,
      });
    });

    this.activeExecutions.set(docId, {
      program,
      mode: "simulation",
      isRunning: true,
      engine,
      webviewPanel,
    });

    // Start execution
    await engine.start();

    // Notify webview
    webviewPanel.webview.postMessage({ type: "executionStarted", mode: "simulation" });
    
    vscode.window.showInformationMessage("🚀 Ladder simulation started (100ms scan cycle)");
  }

  /**
   * Run in hardware mode
   */
  private async runHardware(docId: string, program: LadderProgram, webviewPanel: vscode.WebviewPanel): Promise<void> {
    console.log("[Ladder] Starting hardware mode");

    try {
      // Get runtime configuration
      const config = await getRuntimeConfig();
      if (!config) {
        vscode.window.showErrorMessage("No runtime configuration found. Please configure trust-runtime connection.");
        return;
      }

      // Create and connect runtime client
      const runtimeClient = new RuntimeClient(config);
      await runtimeClient.connect();

      // Create ladder engine with hardware mode
      const engine = new LadderEngine(program, "hardware", {
        scanCycleMs: 100,
        runtimeClient,
      });

      // Set up state change callback
      engine.setStateChangeCallback((state) => {
        webviewPanel.webview.postMessage({
          type: "stateUpdate",
          state,
        });
      });

      this.activeExecutions.set(docId, {
        program,
        mode: "hardware",
        isRunning: true,
        engine,
        webviewPanel,
      });

      // Start execution
      await engine.start();

      // Notify webview
      webviewPanel.webview.postMessage({ type: "executionStarted", mode: "hardware" });
      
      vscode.window.showInformationMessage("🔧 Ladder hardware execution started");
    } catch (error) {
      vscode.window.showErrorMessage(`Failed to start hardware execution: ${error}`);
      console.error("[Ladder] Hardware execution error:", error);
    }
  }

  /**
   * Stop execution
   */
  private async stopExecution(docId: string): Promise<void> {
    const state = this.activeExecutions.get(docId);
    if (!state) return;

    console.log("[Ladder] Stopping execution");
    state.isRunning = false;

    // Stop engine and cleanup
    await state.engine.cleanup();
    
    // Notify webview
    state.webviewPanel.webview.postMessage({ type: "executionStopped" });
    
    this.activeExecutions.delete(docId);
    
    vscode.window.showInformationMessage("⏹️ Ladder execution stopped");
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
