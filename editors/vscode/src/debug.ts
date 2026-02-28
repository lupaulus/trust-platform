import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";

import { getBinaryPath } from "./binary";
import {
  __testCreateDefaultConfigurationAuto,
  __testEnsureConfigurationEntryAuto,
  captureStructuredTextEditor,
  clearSessionProgram,
  DEBUG_TYPE,
  debugChannel,
  ensureConfigurationEntry,
  ensureConfigurationEntryAuto,
  initializeDebugConfigurationState,
  isConfigurationFile,
  loadRuntimeControlConfig,
  markSessionProgram,
  maybeReloadForEditor,
  preferredStructuredTextUri,
  runtimeSourceOptions,
  selectWorkspaceFolderPathForMode,
  validateConfiguration,
} from "./debug/configuration";

const LAUNCH_WARN_DELAY_MS = 1500;

type LaunchFallbackState = {
  seenLaunch: boolean;
  fallbackTimer?: NodeJS.Timeout;
};

const launchFallbackState = new Map<string, LaunchFallbackState>();

function resolveAdapterCommand(
  config: vscode.WorkspaceConfiguration,
  context: vscode.ExtensionContext
): string {
  return getBinaryPath(context, "trust-debug", "debug.adapter.path");
}

async function ensureAdapterCommand(
  config: vscode.WorkspaceConfiguration,
  context: vscode.ExtensionContext
): Promise<string | undefined> {
  const command = resolveAdapterCommand(config, context);

  if (path.isAbsolute(command) && !fs.existsSync(command)) {
    void vscode.window
      .showErrorMessage(
        `Structured Text debug adapter not found at '${command}'. ` +
          `Install the extension from the Marketplace or set trust-lsp.debug.adapter.path.`,
        "Open Settings"
      )
      .then((choice) => {
        if (choice === "Open Settings") {
          void vscode.commands.executeCommand(
            "workbench.action.openSettings",
            "trust-lsp.debug.adapter.path"
          );
        }
      });
    return undefined;
  }

  return command;
}

function adapterEnv(
  config: vscode.WorkspaceConfiguration
): Record<string, string> {
  const overrides =
    config.get<Record<string, string>>("debug.adapter.env") ?? {};
  return {
    ...(process.env as Record<string, string>),
    ...overrides,
  };
}

class StructuredTextDebugAdapterFactory
  implements vscode.DebugAdapterDescriptorFactory, vscode.Disposable
{
  constructor(private readonly context: vscode.ExtensionContext) {}

  dispose(): void {
    // No resources to dispose yet.
  }

  createDebugAdapterDescriptor(
    _session: vscode.DebugSession
  ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
    const config = vscode.workspace.getConfiguration("trust-lsp");
    debugChannel().appendLine("createDebugAdapterDescriptor called");
    return ensureAdapterCommand(config, this.context).then((command) => {
      if (!command) {
        debugChannel().appendLine(
          "No debug adapter command resolved; aborting session."
        );
        return undefined;
      }
      debugChannel().appendLine(`Launching adapter: ${command}`);
      const args = config.get<string[]>("debug.adapter.args") ?? [];
      const options: vscode.DebugAdapterExecutableOptions = {
        env: adapterEnv(config),
      };
      return new vscode.DebugAdapterExecutable(command, args, options);
    });
  }
}

class StructuredTextDebugConfigurationProvider
  implements vscode.DebugConfigurationProvider
{
  async resolveDebugConfiguration(
    folder: vscode.WorkspaceFolder | undefined,
    config: vscode.DebugConfiguration
  ): Promise<vscode.DebugConfiguration | null | undefined> {
    if (!config.type && !config.request && !config.name) {
      config.type = DEBUG_TYPE;
      config.request = "launch";
      config.name = "Debug Structured Text";
    }

    if (!config.type) {
      config.type = DEBUG_TYPE;
    }
    if (!config.request) {
      config.request = "launch";
    }
    if (!config.name) {
      config.name = "Debug Structured Text";
    }

    if (config.request === "attach") {
      if (!config.endpoint) {
        const controlConfig = loadRuntimeControlConfig(folder);
        if (!controlConfig?.endpoint) {
          vscode.window.showErrorMessage(
            "Attach requires runtime.control.endpoint in runtime.toml."
          );
          return null;
        }
        config.endpoint = controlConfig.endpoint;
        if (controlConfig.authToken && !config.authToken) {
          config.authToken = controlConfig.authToken;
        }
      }
      const runtimeOptions = runtimeSourceOptions();
      Object.assign(config, runtimeOptions);
    } else {
      if (!config.program) {
        const configUri = await ensureConfigurationEntryAuto();
        if (!configUri) {
          return null;
        }
        config.program = configUri.fsPath;
      } else {
        const programUri = vscode.Uri.file(config.program);
        if (!(await isConfigurationFile(programUri))) {
          const configUri = await ensureConfigurationEntryAuto();
          if (!configUri) {
            return null;
          }
          config.program = configUri.fsPath;
        }
      }
    }

    if (!config.cwd && folder) {
      config.cwd = folder.uri.fsPath;
    }

    debugChannel().appendLine(
      `Resolved debug config: type=${config.type} request=${config.request} program=${config.program ?? "<none>"} cwd=${config.cwd ?? "<none>"}`
    );

    return config;
  }

  resolveDebugConfigurationWithSubstitutedVariables(
    _folder: vscode.WorkspaceFolder | undefined,
    config: vscode.DebugConfiguration
  ): vscode.DebugConfiguration | null | undefined {
    debugChannel().appendLine(
      `Resolved debug config (substituted): type=${config.type} request=${config.request} program=${config.program ?? "<none>"} cwd=${config.cwd ?? "<none>"}`
    );
    return config;
  }
}

class StructuredTextDebugAdapterTrackerFactory
  implements vscode.DebugAdapterTrackerFactory
{
  createDebugAdapterTracker(
    session: vscode.DebugSession
  ): vscode.ProviderResult<vscode.DebugAdapterTracker> {
    if (session.type !== DEBUG_TYPE) {
      return undefined;
    }
    const interestingCommands = new Set([
      "initialize",
      "launch",
      "configurationDone",
      "setBreakpoints",
      "threads",
      "stackTrace",
      "scopes",
      "variables",
      "continue",
      "pause",
      "disconnect",
    ]);
    const interestingEvents = new Set([
      "initialized",
      "stopped",
      "continued",
      "terminated",
      "exited",
      "output",
    ]);
    const sessionId = session.id ?? session.name;
    const state: LaunchFallbackState = {
      seenLaunch: false,
    };
    launchFallbackState.set(sessionId, state);
    const channel = debugChannel();
    const formatMessage = (value: unknown): string => {
      try {
        return JSON.stringify(value);
      } catch (err) {
        return String(err);
      }
    };
    channel.appendLine(`Debug adapter tracker attached: ${session.name}`);
    return {
      onWillReceiveMessage: (message) => {
        channel.appendLine(`[DAP <-] ${formatMessage(message)}`);
        const command = (message as { command?: string }).command;
        if (command && interestingCommands.has(command)) {
          channel.appendLine(`[DAP <-] ${command}`);
        }
        if (command === "launch") {
          state.seenLaunch = true;
        }
      },
      onDidSendMessage: (message) => {
        channel.appendLine(`[DAP ->] ${formatMessage(message)}`);
        const event = (message as { event?: string }).event;
        if (event && interestingEvents.has(event)) {
          channel.appendLine(`[DAP ->] event ${event}`);
        }
        if (event === "initialized" && !state.fallbackTimer) {
          state.fallbackTimer = setTimeout(() => {
            const current = launchFallbackState.get(sessionId);
            if (!current || current.seenLaunch) {
              return;
            }
            channel.appendLine(
              "[DAP] launch not seen after initialized; waiting for VS Code"
            );
          }, LAUNCH_WARN_DELAY_MS);
        }
      },
      onError: (error) => {
        channel.appendLine(`[DAP] error: ${error}`);
      },
      onExit: (code, signal) => {
        channel.appendLine(
          `[DAP] exit: code=${code ?? "<none>"} signal=${signal ?? "<none>"}`
        );
        const current = launchFallbackState.get(sessionId);
        if (current?.fallbackTimer) {
          clearTimeout(current.fallbackTimer);
        }
        launchFallbackState.delete(sessionId);
      },
    };
  }
}

export function registerDebugAdapter(
  context: vscode.ExtensionContext
): void {
  initializeDebugConfigurationState(context.workspaceState);
  captureStructuredTextEditor(vscode.window.activeTextEditor);

  const factory = new StructuredTextDebugAdapterFactory(context);
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(DEBUG_TYPE, factory)
  );
  context.subscriptions.push(factory);
  debugChannel().appendLine("Structured Text debug adapter factory registered.");

  const provider = new StructuredTextDebugConfigurationProvider();
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(DEBUG_TYPE, provider)
  );

  const trackerFactory = new StructuredTextDebugAdapterTrackerFactory();
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterTrackerFactory(DEBUG_TYPE, trackerFactory)
  );

  const stringifySession = (session: vscode.DebugSession): string => {
    try {
      return JSON.stringify(session.configuration);
    } catch (err) {
      return String(err);
    }
  };

  context.subscriptions.push(
    vscode.debug.onDidStartDebugSession((session) => {
      debugChannel().appendLine(
        `Debug session started: ${session.name} type=${session.type} config=${stringifySession(session)}`
      );
      markSessionProgram(session);
    })
  );

  context.subscriptions.push(
    vscode.debug.onDidTerminateDebugSession((session) => {
      debugChannel().appendLine(
        `Debug session terminated: ${session.name} type=${session.type} config=${stringifySession(session)}`
      );
      if (session.type === DEBUG_TYPE) {
        const sessionId = session.id ?? session.name;
        const current = launchFallbackState.get(sessionId);
        if (current?.fallbackTimer) {
          clearTimeout(current.fallbackTimer);
        }
        launchFallbackState.delete(sessionId);
        clearSessionProgram(session);
      }
    })
  );

  context.subscriptions.push(
    vscode.debug.onDidChangeActiveDebugSession((session) => {
      if (session) {
        debugChannel().appendLine(
          `Debug session active: ${session.name} type=${session.type}`
        );
      } else {
        debugChannel().appendLine("Debug session active: <none>");
      }
    })
  );

  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor((editor) => {
      captureStructuredTextEditor(editor);
      void maybeReloadForEditor(editor);
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      "trust-lsp.debug.start",
      async (programOverride?: string | vscode.Uri) => {
        let programUri: vscode.Uri | undefined;
        let folder: vscode.WorkspaceFolder | undefined;

        if (typeof programOverride === "string" && programOverride.trim()) {
          programUri = vscode.Uri.file(programOverride);
        } else if (programOverride instanceof vscode.Uri) {
          programUri = programOverride;
        }

        if (programUri) {
          if (!(await isConfigurationFile(programUri))) {
            vscode.window.showErrorMessage(
              "Debugging requires a CONFIGURATION entry file."
            );
            return;
          }
        } else {
          programUri = await ensureConfigurationEntryAuto();
          if (!programUri) {
            return;
          }
        }

        folder = vscode.workspace.getWorkspaceFolder(programUri);
        if (!folder) {
          folder = vscode.workspace.workspaceFolders?.[0];
        }

        const diagnostics = vscode.languages.getDiagnostics(programUri);
        if (
          diagnostics.some(
            (diagnostic) => diagnostic.severity === vscode.DiagnosticSeverity.Error
          )
        ) {
          vscode.window.showErrorMessage(
            "Configuration has errors. Fix them before starting a debug session."
          );
          return;
        }
        if (!(await validateConfiguration(programUri))) {
          return;
        }

        const program = programUri.fsPath;
        debugChannel().appendLine(`Start debugging command: program=${program}`);

        const runtimeOptions = runtimeSourceOptions(programUri);
        const config: vscode.DebugConfiguration = {
          type: DEBUG_TYPE,
          request: "launch",
          name: "Debug Structured Text",
          program,
          ...runtimeOptions,
        };

        if (folder) {
          config.cwd = folder.uri.fsPath;
        }

        const pendingTimer = setTimeout(() => {
          const active = vscode.debug.activeDebugSession;
          debugChannel().appendLine(
            `startDebugging still pending after 5s: active=${active?.name ?? "<none>"} type=${active?.type ?? "<none>"} config=${JSON.stringify(config)}`
          );
        }, 5000);
        void vscode.debug.startDebugging(folder, config).then(
          (started) => {
            clearTimeout(pendingTimer);
            debugChannel().appendLine(
              `startDebugging result: ${started} folder=${folder?.name ?? "<none>"} config=${JSON.stringify(config)}`
            );
          },
          (err: unknown) => {
            clearTimeout(pendingTimer);
            debugChannel().appendLine(
              `startDebugging error: ${err instanceof Error ? err.message : String(err)} folder=${folder?.name ?? "<none>"} config=${JSON.stringify(config)}`
            );
          }
        );
      }
    )
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("trust-lsp.debug.attach", async () => {
      const folder = vscode.workspace.workspaceFolders?.[0];
      const controlConfig = loadRuntimeControlConfig(folder);
      if (!controlConfig?.endpoint) {
        vscode.window.showErrorMessage(
          "Attach requires runtime.control.endpoint in runtime.toml."
        );
        return;
      }
      const runtimeOptions = runtimeSourceOptions();
      const config: vscode.DebugConfiguration = {
        type: DEBUG_TYPE,
        request: "attach",
        name: "Attach Structured Text",
        endpoint: controlConfig.endpoint,
        authToken: controlConfig.authToken,
        ...runtimeOptions,
      };
      if (folder) {
        config.cwd = folder.uri.fsPath;
      }
      void vscode.debug.startDebugging(folder, config);
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      "trust-lsp.debug.ensureConfiguration",
      async () => {
        await ensureConfigurationEntry();
      }
    )
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("trust-lsp.debug.reload", async () => {
      const session = vscode.debug.activeDebugSession;
      if (!session || session.type !== DEBUG_TYPE) {
        vscode.window.showErrorMessage(
          "No active Structured Text debug session to reload."
        );
        return;
      }

      const config = session.configuration ?? {};
      const program =
        typeof config.program === "string" ? config.program : undefined;
      const preferred = preferredStructuredTextUri();
      const activeFile = preferred?.fsPath;

      try {
        const target =
          program && program.trim().length > 0
            ? vscode.Uri.file(program)
            : preferred;
        const runtimeOptions = runtimeSourceOptions(target);
        await session.customRequest("stReload", {
          program: program ?? activeFile,
          ...runtimeOptions,
        });
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        vscode.window.showErrorMessage(`Hot reload failed: ${message}`);
      }
    })
  );
}

export {
  __testCreateDefaultConfigurationAuto,
  __testEnsureConfigurationEntryAuto,
  selectWorkspaceFolderPathForMode,
};
