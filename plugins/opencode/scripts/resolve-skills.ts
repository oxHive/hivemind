import { readdirSync, readlinkSync, readFileSync, unlinkSync, writeFileSync } from "node:fs"
import { resolve, dirname } from "node:path"

const skillsDir = resolve(import.meta.dir, "..", "skills")

function resolveSymlinks(dir: string) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const fullPath = resolve(dir, entry.name)

    if (entry.isSymbolicLink()) {
      const target = readlinkSync(fullPath)
      const targetPath = resolve(dirname(fullPath), target)
      const content = readFileSync(targetPath)
      unlinkSync(fullPath)
      writeFileSync(fullPath, content)
      console.log(`  resolved: ${fullPath}`)
    } else if (entry.isDirectory()) {
      resolveSymlinks(fullPath)
    }
  }
}

console.log("Resolving skill symlinks...")
resolveSymlinks(skillsDir)
console.log("Done.")
