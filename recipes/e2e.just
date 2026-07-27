# Dashboard test suites. Unit tests (Vitest) run in Node, no server needed.
# E2E tests (Playwright) spin up a real `hivemind up --headless` backend
# (fresh temp DB + XDG_CONFIG_HOME per run, see dashboard/playwright.config.js)
# plus `vite dev`, then drive a real browser against them.

e2e-unit:
  cd dashboard && bun run test:unit

# Headless, CI-friendly: no visible browser window.
e2e-headless:
  cd dashboard && bun run test:e2e

# Headed + single worker, for watching the browser interact locally.
e2e-browser:
  cd dashboard && bun run test:e2e:ui

# Both suites, headless -- what CI should run.
e2e: e2e-unit e2e-headless
