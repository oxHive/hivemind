import type { Plugin } from "@opencode-ai/plugin"
import { existsSync, mkdirSync, readdirSync, copyFileSync } from "node:fs"
import { resolve, join } from "node:path"
import { homedir } from "node:os"

const HIVEMIND_INSTRUCTIONS = `# HiveMind Memory System

You have access to HiveMind via MCP tools: memory_store, memory_recall,
memory_search, memory_update, memory_delete, memory_store_edge, hivemind_session_start.

At the start of every session, before doing anything else:

1. Check if .hivemind.toml exists in the project root.
2. If it exists, call hivemind_session_start with the project root path immediately.
3. Incorporate the returned context silently -- do not narrate it.

After calling hivemind_session_start:

- If budget.truncated is true, mention once: "Some memory entries were skipped
  due to token budget. Run hivemind status to review."
- If any skipped entry has reason not_found, mention once which recalls were not
  found so the user can check their .hivemind.toml.
- Then proceed normally.

If .hivemind.toml does not exist:

- Do not call hivemind_session_start.
- Tools remain available on demand.
- If the user seems to be starting a new project, suggest: "Run hivemind init
  to set up memory hooks for this project."

## Suggest storing -- never auto-store

When the user shares something worth persisting (preferences, project context,
design decisions), suggest: "That seems worth remembering -- should I store it?"
Wait for explicit confirmation before calling memory_store.
`

export default (async ({ client, directory, $ }) => {
  const hivemindBin = await resolveHivemind($)
  const installedSkills = installSkills()
  if (installedSkills.length) {
    await client.app.log({
      body: {
        service: "hivemind",
        level: "info",
        message: `installed ${installedSkills.length} skill(s) to ${globalSkillsDir()}`,
      },
    })
  }

  if (hivemindBin) {
    await client.app.log({
      body: {
        service: "hivemind",
        level: "info",
        message: `hivemind binary found: ${hivemindBin}`,
      },
    })
  } else {
    await client.app.log({
      body: {
        service: "hivemind",
        level: "warn",
        message:
          "hivemind binary not found in PATH. MCP server not registered. Install: cargo binstall oxhivemind",
      },
    })
  }

  return {
    config: (cfg) => {
      if (!hivemindBin) return
      if (!cfg.mcp) cfg.mcp = {}
      if (cfg.mcp.hivemind) return

      cfg.mcp.hivemind = {
        type: "local",
        command: [hivemindBin],
        enabled: true,
      }
    },

    "experimental.chat.system.transform": async (_input, output) => {
      if (!output.content) output.content = []
      const configPath = resolve(directory, ".hivemind.toml")
      if (!existsSync(configPath)) {
        output.content.push(
          "HiveMind is available but not initialized for this project. Run: hivemind init",
        )
        return
      }

      output.content.push(HIVEMIND_INSTRUCTIONS)
    },
  }
}) satisfies Plugin

// OpenCode never scans npm package contents for skills — it only discovers
// them from specific filesystem paths (.opencode/skills, ~/.claude/skills,
// ~/.config/opencode/skills, etc — see https://opencode.ai/docs/skills/).
// So the skills/ bundled in this package are otherwise invisible to
// OpenCode-only users; copy them into its global skills directory on every
// plugin load so they're actually picked up. Overwrites each time (these
// aren't meant to be hand-edited, same as Claude Code plugin skills) so
// updates to this package propagate on next OpenCode start.
function globalSkillsDir(): string {
  const xdg = process.env.XDG_CONFIG_HOME
  const base = xdg && xdg.trim() ? xdg : join(homedir(), ".config")
  return join(base, "opencode", "skills")
}

function installSkills(): string[] {
  try {
    const sourceDir = resolve(import.meta.dir, "skills")
    if (!existsSync(sourceDir)) return []
    const targetRoot = globalSkillsDir()
    const installed: string[] = []
    for (const name of readdirSync(sourceDir)) {
      const sourceSkill = join(sourceDir, name, "SKILL.md")
      if (!existsSync(sourceSkill)) continue
      const targetDir = join(targetRoot, name)
      mkdirSync(targetDir, { recursive: true })
      copyFileSync(sourceSkill, join(targetDir, "SKILL.md"))
      installed.push(name)
    }
    return installed
  } catch {
    return []
  }
}

async function resolveHivemind(
  $: (strings: TemplateStringsArray, ...values: unknown[]) => Promise<{ stdout: Uint8Array }>,
): Promise<string | null> {
  try {
    const result = await $`which hivemind`
    const path = result.stdout.toString().trim()
    if (path) return path
  } catch {
    // not found
  }
  return null
}
