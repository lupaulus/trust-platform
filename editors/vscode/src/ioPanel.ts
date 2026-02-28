import * as vscode from "vscode";

import {
  runtimeSourceOptionsForTarget,
  type RuntimeSourceOptions,
} from "./runtimeSourceOptions";
import { compileActiveProgram } from "./io-panel/compile";
import {
  applySettingsUpdate,
  collectSettingsSnapshot,
} from "./io-panel/settings";
import {
  getStructuredTextSession,
  trackStructuredTextSession,
  untrackStructuredTextSession,
} from "./io-panel/session";
import { probeEndpointReachable, runtimeStatusPayload } from "./io-panel/status";
import { getHtml } from "./io-panel/view";
import {
  IoState,
  RuntimeStatusPayload,
  SettingsPayload,
} from "./io-panel/types";

const DEBUG_TYPE = "structured-text";

let panel: vscode.WebviewPanel | undefined;

export function registerIoPanel(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("trust-lsp.debug.openIoPanel", () => {
      showPanel(context);
    })
  );

  const activeSession = vscode.debug.activeDebugSession;
  if (activeSession && activeSession.type === DEBUG_TYPE) {
    trackStructuredTextSession(activeSession);
  }

  context.subscriptions.push(
    vscode.debug.onDidReceiveDebugSessionCustomEvent((event) => {
      if (event.event !== "stIoState") {
        return;
      }
      if (event.session.type !== DEBUG_TYPE) {
        return;
      }
      if (!panel) {
        return;
      }
      const body = event.body as IoState | undefined;
      panel.webview.postMessage({
        type: "ioState",
        payload: body ?? { inputs: [], outputs: [], memory: [] },
      });
    })
  );

  context.subscriptions.push(
    vscode.debug.onDidStartDebugSession((session) => {
      if (session.type !== DEBUG_TYPE) {
        return;
      }
      trackStructuredTextSession(session);
      void requestIoState();
      void sendRuntimeStatus();
    })
  );

  context.subscriptions.push(
    vscode.debug.onDidTerminateDebugSession((session) => {
      if (session.type !== DEBUG_TYPE) {
        return;
      }
      untrackStructuredTextSession(session);
      void sendRuntimeStatus();
    })
  );

  context.subscriptions.push(
    vscode.debug.onDidChangeActiveDebugSession((session) => {
      if (panel) {
        void requestIoState();
      }
      if (session && session.type === DEBUG_TYPE) {
        trackStructuredTextSession(session);
      }
      void sendRuntimeStatus();
    })
  );

  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (
        event.affectsConfiguration("trust-lsp.runtime.controlEndpoint") ||
        event.affectsConfiguration("trust-lsp.runtime.controlEndpointEnabled") ||
        event.affectsConfiguration("trust-lsp.runtime.inlineValuesEnabled") ||
        event.affectsConfiguration("trust-lsp.runtime.mode")
      ) {
        void sendRuntimeStatus();
      }
    })
  );
}

function showPanel(context: vscode.ExtensionContext): void {
  if (panel) {
    panel.reveal();
    void requestIoState();
    void sendRuntimeStatus();
    return;
  }

  panel = vscode.window.createWebviewPanel(
    "trust-io-panel",
    "Structured Text Runtime",
    vscode.ViewColumn.Two,
    {
      enableScripts: true,
      retainContextWhenHidden: true,
      localResourceRoots: [
        vscode.Uri.joinPath(context.extensionUri, "media"),
        vscode.Uri.joinPath(context.extensionUri, "node_modules"),
      ],
    }
  );

  panel.webview.html = getHtml(panel.webview, context.extensionUri);
  panel.onDidDispose(() => {
    panel = undefined;
  });

  panel.webview.onDidReceiveMessage(handleWebviewMessage);

  void requestIoState();
  void sendRuntimeStatus();

  context.subscriptions.push(panel);
}

function postPanelStatus(message: string): void {
  panel?.webview.postMessage({
    type: "status",
    payload: message,
  });
}

function handleWebviewMessage(message: any): void {
  const type = typeof message?.type === "string" ? message.type : "";
  switch (type) {
    case "refresh":
      void requestIoState();
      break;
    case "writeInput":
      void writeInput(String(message.address || ""), String(message.value || ""));
      break;
    case "forceInput":
      void forceInput(String(message.address || ""), String(message.value || ""));
      break;
    case "releaseInput":
      void releaseInput(String(message.address || ""));
      break;
    case "startDebug":
      void startDebugging();
      break;
    case "compile":
      void compileActiveProgram(
        {
          getPanel: () => panel,
          getStructuredTextSession,
          startDebugging,
        },
        {}
      );
      break;
    case "compileAndStart":
      void compileActiveProgram(
        {
          getPanel: () => panel,
          getStructuredTextSession,
          startDebugging,
        },
        { startDebugAfter: true }
      );
      break;
    case "stopDebug":
      void stopDebugging();
      break;
    case "runtimeStart":
      void handleRuntimePrimary();
      break;
    case "runtimeSetMode":
      void setRuntimeMode(message.mode);
      break;
    case "requestSettings":
      panel?.webview.postMessage({
        type: "settings",
        payload: collectSettingsSnapshot(),
      });
      break;
    case "saveSettings":
      void saveSettings(message.payload);
      break;
    case "webviewError": {
      const detail =
        typeof message.message === "string" ? message.message : "Unknown error";
      console.error("Runtime panel webview error:", detail, message.stack || "");
      postPanelStatus(`Runtime panel error: ${detail}`);
      break;
    }
    case "webviewReady":
      console.info("Runtime panel webview ready.");
      void sendRuntimeStatus();
      break;
    default:
      break;
  }
}

async function saveSettings(payload: SettingsPayload | undefined): Promise<void> {
  await applySettingsUpdate(payload);
  postPanelStatus("Settings saved.");
}

export async function __testApplySettingsUpdate(
  payload: SettingsPayload | undefined
): Promise<void> {
  await applySettingsUpdate(payload);
}

export function __testCollectSettingsSnapshot(): SettingsPayload {
  return collectSettingsSnapshot();
}

function runtimeConfigTarget(): vscode.Uri | undefined {
  const activeSession = getStructuredTextSession();
  if (activeSession?.workspaceFolder) {
    return activeSession.workspaceFolder.uri;
  }
  const editor = vscode.window.activeTextEditor;
  if (editor) {
    const folder = vscode.workspace.getWorkspaceFolder(editor.document.uri);
    if (folder) {
      return folder.uri;
    }
  }
  return vscode.workspace.workspaceFolders?.[0]?.uri;
}

function runtimeConfigScope(
  target: vscode.Uri | undefined
): vscode.ConfigurationTarget {
  return target
    ? vscode.ConfigurationTarget.WorkspaceFolder
    : vscode.ConfigurationTarget.Workspace;
}

async function currentRuntimeStatusPayload(): Promise<RuntimeStatusPayload> {
  return runtimeStatusPayload({
    runtimeConfigTarget,
    getStructuredTextSession,
  });
}

async function sendRuntimeStatus(): Promise<void> {
  if (!panel) {
    return;
  }
  const payload = await currentRuntimeStatusPayload();
  panel.webview.postMessage({
    type: "runtimeStatus",
    payload,
  });
}

async function requestIoState(): Promise<void> {
  const session = getStructuredTextSession();
  if (!session) {
    panel?.webview.postMessage({
      type: "status",
      payload: "No active Structured Text debug session.",
    });
    return;
  }

  try {
    await session.customRequest("stIoState");
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    panel?.webview.postMessage({
      type: "status",
      payload: `I/O state request failed: ${message}`,
    });
  }
}

async function writeInput(address: string, value: string): Promise<void> {
  const session = getStructuredTextSession();
  if (!session) {
    panel?.webview.postMessage({
      type: "status",
      payload: "No active Structured Text debug session.",
    });
    return;
  }
  if (!address) {
    panel?.webview.postMessage({
      type: "status",
      payload: "Missing I/O address.",
    });
    return;
  }

  try {
    await session.customRequest("stIoWrite", { address, value });
    panel?.webview.postMessage({
      type: "status",
      payload: `I/O write queued for ${address}.`,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    panel?.webview.postMessage({
      type: "status",
      payload: `I/O write failed: ${message}`,
    });
  }
}

async function forceInput(address: string, value: string): Promise<void> {
  const session = getStructuredTextSession();
  if (!session) {
    panel?.webview.postMessage({
      type: "status",
      payload: "No active Structured Text debug session.",
    });
    return;
  }
  if (!address) {
    panel?.webview.postMessage({
      type: "status",
      payload: "Missing I/O address.",
    });
    return;
  }

  try {
    await session.customRequest("setExpression", {
      expression: address,
      value: `force: ${value}`,
    });
    panel?.webview.postMessage({
      type: "status",
      payload: `I/O force active at ${address}.`,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    panel?.webview.postMessage({
      type: "status",
      payload: `I/O force failed: ${message}`,
    });
  }
}

async function releaseInput(address: string): Promise<void> {
  const session = getStructuredTextSession();
  if (!session) {
    panel?.webview.postMessage({
      type: "status",
      payload: "No active Structured Text debug session.",
    });
    return;
  }
  if (!address) {
    panel?.webview.postMessage({
      type: "status",
      payload: "Missing I/O address.",
    });
    return;
  }

  try {
    await session.customRequest("setExpression", {
      expression: address,
      value: "release",
    });
    panel?.webview.postMessage({
      type: "status",
      payload: `I/O force released at ${address}.`,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    panel?.webview.postMessage({
      type: "status",
      payload: `I/O release failed: ${message}`,
    });
  }
}

async function stopDebugging(): Promise<void> {
  const session = getStructuredTextSession();
  if (!session) {
    panel?.webview.postMessage({
      type: "status",
      payload: "No active Structured Text debug session.",
    });
    return;
  }
  try {
    await vscode.debug.stopDebugging(session);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    panel?.webview.postMessage({
      type: "status",
      payload: `Stop debugging failed: ${message}`,
    });
  }
}

async function startDebugging(programOverride?: string): Promise<void> {
  try {
    await vscode.commands.executeCommand("trust-lsp.debug.start", programOverride);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    panel?.webview.postMessage({
      type: "status",
      payload: `Start debugging failed: ${message}`,
    });
  }
}

async function startAttachDebugging(
  endpoint: string,
  authToken?: string
): Promise<boolean> {
  const folder = vscode.workspace.workspaceFolders?.[0];
  const runtimeOptions = runtimeSourceOptions();
  const config: vscode.DebugConfiguration = {
    type: DEBUG_TYPE,
    request: "attach",
    name: "Attach Structured Text",
    endpoint,
    authToken,
    ...runtimeOptions,
  };
  if (folder) {
    config.cwd = folder.uri.fsPath;
  }
  try {
    const started = await vscode.debug.startDebugging(folder, config);
    if (!started) {
      panel?.webview.postMessage({
        type: "status",
        payload: "Attach failed to start.",
      });
    }
    return started;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    panel?.webview.postMessage({
      type: "status",
      payload: `Attach failed: ${message}`,
    });
    return false;
  }
}

async function setRuntimeMode(mode: unknown): Promise<void> {
  const normalized = mode === "online" ? "online" : "simulate";
  const target = runtimeConfigTarget();
  const config = vscode.workspace.getConfiguration("trust-lsp", target);
  await config.update("runtime.mode", normalized, runtimeConfigScope(target));
  void sendRuntimeStatus();
}

async function handleRuntimePrimary(): Promise<void> {
  const status = await currentRuntimeStatusPayload();
  if (status.running || status.runtimeState === "connected") {
    await handleRuntimeStop();
    return;
  }
  await handleRuntimeStart();
}

async function handleRuntimeStart(): Promise<void> {
  const status = await currentRuntimeStatusPayload();
  const target = runtimeConfigTarget();
  const config = vscode.workspace.getConfiguration("trust-lsp", target);
  const mode = config.get<"simulate" | "online">("runtime.mode", "simulate");

  if (mode === "simulate") {
    await compileActiveProgram(
      {
        getPanel: () => panel,
        getStructuredTextSession,
        startDebugging,
      },
      { startDebugAfter: true }
    );
    return;
  }

  const endpoint = status.endpoint;
  if (!status.endpointConfigured) {
    panel?.webview.postMessage({
      type: "status",
      payload: "Runtime endpoint not set.",
    });
    void sendRuntimeStatus();
    return;
  }

  if (!status.endpointEnabled) {
    await config.update(
      "runtime.controlEndpointEnabled",
      true,
      runtimeConfigScope(target)
    );
  }

  const reachable = await probeEndpointReachable(endpoint);
  if (reachable) {
    const authToken = config.get<string>("runtime.controlAuthToken") ?? "";
    await startAttachDebugging(endpoint, authToken || undefined);
    void sendRuntimeStatus();
    return;
  }

  panel?.webview.postMessage({
    type: "status",
    payload: `Runtime not reachable: ${endpoint}`,
  });
  void sendRuntimeStatus();
}

async function handleRuntimeStop(): Promise<void> {
  const activeSession = getStructuredTextSession();
  if (activeSession) {
    await stopDebugging();
    return;
  }
  const status = await currentRuntimeStatusPayload();
  if (status.runtimeState === "connected") {
    const target = runtimeConfigTarget();
    const config = vscode.workspace.getConfiguration("trust-lsp", target);
    await config.update(
      "runtime.controlEndpointEnabled",
      false,
      runtimeConfigScope(target)
    );
    void sendRuntimeStatus();
  }
}

function runtimeSourceOptions(target?: vscode.Uri): RuntimeSourceOptions {
  return runtimeSourceOptionsForTarget(target);
}
