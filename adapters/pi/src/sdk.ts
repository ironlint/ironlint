import { constants, lstatSync, readFileSync, realpathSync } from "node:fs"
import { access, mkdir, readFile, writeFile } from "node:fs/promises"
import { homedir } from "node:os"
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"
import type { ResourceLoader } from "@earendil-works/pi-coding-agent"
import type { CompletionSession } from "./completion.ts"

const PINNED_PI = "0.87.1"

function verifyRuntime(): void {
  let entry: string
  try { entry = fileURLToPath(import.meta.resolve("@earendil-works/pi-coding-agent")) }
  catch { throw new Error(`Pi SDK ${PINNED_PI} is required for controlled completion`) }
  const packageFile = join(dirname(entry), "..", "package.json")
  const version = (JSON.parse(readFileSync(packageFile, "utf8")) as { version?: unknown }).version
  if (version !== PINNED_PI) throw new Error(`unsupported Pi SDK version ${String(version)}; install ${PINNED_PI}`)
}

function ownerResources(createExtensionRuntime: typeof import("@earendil-works/pi-coding-agent").createExtensionRuntime): ResourceLoader {
  return {
    getExtensions: () => ({ extensions: [], errors: [], runtime: createExtensionRuntime() }),
    getSkills: () => ({ skills: [], diagnostics: [] }),
    getPrompts: () => ({ prompts: [], diagnostics: [] }),
    getThemes: () => ({ themes: [], diagnostics: [] }),
    getAgentsFiles: () => ({ agentsFiles: [] }),
    getSystemPrompt: () => "You are a coding assistant. Work on the user's task using the available tools. Leave intermediate failures editable and repair project checks when asked.",
    getSystemPromptSource: () => undefined,
    getAppendSystemPrompt: () => [],
    getAppendSystemPromptSources: () => [],
    extendResources: () => {},
    reload: async () => {},
  }
}

export function confined(root: string, target: string, allowRoot = false): string {
  const path = resolve(target)
  const relation = relative(root, path)
  if ((!relation && !allowRoot) || relation === ".." || relation.startsWith(`..${sep}`) || isAbsolute(relation)
    || relation.split(sep).some((part) => part.toLowerCase() === ".git")) {
    throw new Error(`tool path is outside the candidate workspace: ${target}`)
  }
  let current = root
  for (const part of relation.split(sep)) {
    current = join(current, part)
    try { if (lstatSync(current).isSymbolicLink()) throw new Error(`tool path uses a symlink: ${target}`) }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error }
  }
  return path
}

export async function constrainedTools(root: string): Promise<Array<import("@earendil-works/pi-coding-agent").ToolDefinition>> {
  const { createReadToolDefinition, createEditToolDefinition, createWriteToolDefinition } = await import("@earendil-works/pi-coding-agent")
  const workspace = realpathSync(root)
  const readOps = {
    readFile: (path: string) => readFile(confined(workspace, path)),
    access: (path: string) => access(confined(workspace, path), constants.R_OK),
  }
  const writeOps = {
    mkdir: async (path: string) => { await mkdir(confined(workspace, path, true), { recursive: true }) },
    writeFile: (path: string, content: string) => writeFile(confined(workspace, path), content, "utf8"),
  }
  const editOps = {
    ...readOps,
    access: (path: string) => access(confined(workspace, path), constants.R_OK | constants.W_OK),
    writeFile: writeOps.writeFile,
  }
  return [
    createReadToolDefinition(workspace, { operations: readOps }),
    createEditToolDefinition(workspace, { operations: editOps }),
    createWriteToolDefinition(workspace, { operations: writeOps }),
  ] as unknown as Array<import("@earendil-works/pi-coding-agent").ToolDefinition>
}

export function ownerModelSettings(agentDir: string): { defaultProvider?: string; defaultModel?: string } {
  let raw: unknown
  try { raw = JSON.parse(readFileSync(join(agentDir, "settings.json"), "utf8")) }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return {}
    throw new Error(`cannot load owner Pi settings: ${(error as Error).message}`)
  }
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) throw new Error("invalid owner Pi settings")
  const settings = raw as Record<string, unknown>
  const defaults: { defaultProvider?: string; defaultModel?: string } = {}
  for (const key of ["defaultProvider", "defaultModel"] as const) {
    const value = settings[key]
    if (value === undefined) continue
    if (typeof value !== "string" || !value) throw new Error(`invalid owner Pi ${key}`)
    defaults[key] = value
  }
  return defaults
}

export async function createPiSession(root: string, agentDirectory?: string): Promise<CompletionSession> {
  verifyRuntime()
  const { createAgentSession, createExtensionRuntime, ModelRuntime, SessionManager, SettingsManager } = await import("@earendil-works/pi-coding-agent")
  const workspace = realpathSync(root)
  const agentDir = agentDirectory ?? join(homedir(), ".pi", "agent")
  const modelRuntime = await ModelRuntime.create({
    authPath: join(agentDir, "auth.json"), modelsPath: join(agentDir, "models.json"), allowModelNetwork: false,
  })
  const settingsManager = SettingsManager.inMemory({ ...ownerModelSettings(agentDir), retry: { enabled: true, maxRetries: 2 } })
  const { session } = await createAgentSession({
    cwd: workspace, agentDir, modelRuntime, settingsManager, sessionManager: SessionManager.inMemory(workspace),
    resourceLoader: ownerResources(createExtensionRuntime), tools: ["read", "edit", "write"],
    customTools: await constrainedTools(workspace),
  })
  return {
    prompt: (text) => session.prompt(text),
    waitForIdle: () => session.waitForIdle(),
    abort: () => session.abort(),
    dispose: () => session.dispose(),
    subscribe: (listener) => session.subscribe((event) => listener(event.type)),
    getLastAssistantText: () => session.getLastAssistantText(),
    get isIdle() { return session.isIdle },
    get pendingMessageCount() { return session.pendingMessageCount },
    get finalStopReason() {
      const last = [...session.state.messages].reverse().find((message) => message.role === "assistant")
      return last?.role === "assistant" ? last.stopReason : undefined
    },
    get finalError() {
      const last = [...session.state.messages].reverse().find((message) => message.role === "assistant")
      return last?.role === "assistant" ? last.errorMessage : undefined
    },
  }
}
