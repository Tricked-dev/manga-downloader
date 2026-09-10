// Run against an isolated server and Dex fixture; see README.md in this directory.
import assert from "node:assert/strict";
import { chromium } from "playwright";

const baseURL = process.env.E2E_BASE_URL ?? "http://localhost:4000";
const email = process.env.E2E_EMAIL ?? "reader@example.test";
const password = process.env.E2E_PASSWORD ?? "browser-fixture-password";
const executablePath = process.env.CHROMIUM_EXECUTABLE_PATH;
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
const context = await browser.newContext({ baseURL, viewport: { width: 1440, height: 1000 } });
context.setDefaultTimeout(15000);
const page = await context.newPage();
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
let libraryId;

async function request(path, method = "GET", data) {
  return page.evaluate(async ({ path, method, data }) => {
    const response = await fetch(path, { method, ...(data ? { headers: { "Content-Type": "application/json" }, body: JSON.stringify(data) } : {}) });
    const text = await response.text();
    return { status: response.status, body: text ? JSON.parse(text) : null };
  }, { path, method, data });
}

try {
  await page.goto("/");
  await page.waitForURL("**/login?**");
  await page.getByRole("button", { name: "Sign in with SSO" }).click();
  await page.locator('input[name="login"]').fill(email);
  await page.locator('input[name="password"]').fill(password);
  await page.getByRole("button", { name: /^login$/i }).click();
  await page.waitForURL(baseURL + "/");
  assert.equal((await request("/auth/session")).body.user.email, email);
  assert.equal((await request("/v1/sources")).status, 200);
  console.log("OIDC login and authenticated API: passed");

  // Exercise the actual built route loaders and components, not mocked API data.
  for (const path of ["/", "/updates", "/sources", "/downloads", "/clients", "/stats", "/about", "/settings"]) {
    await page.goto(path);
    await page.locator("main h1").waitFor();
    assert(!await page.getByText("Internal Error", { exact: true }).count(), path);
    console.log(`Page loaded: ${path}`);
  }
  await page.getByText("Automatically upscale new downloads", { exact: true }).filter({ visible: true }).waitFor();
  const automatic = page.getByRole("switch", { name: "Automatically upscale new downloads" });
  const previous = await automatic.getAttribute("aria-checked");
  await automatic.click();
  await page.getByRole("button", { name: "Save Settings", exact: true }).click();
  await page.getByText("Saved successfully", { exact: true }).waitFor();
  assert.equal((await request("/v1/settings")).body.settings.auto_upscale, String(previous !== "true"));
  await automatic.click();
  await Promise.all([
    page.waitForResponse((response) => response.url().endsWith("/v1/settings") && response.request().method() === "PUT"),
    page.getByRole("button", { name: "Save Settings", exact: true }).click(),
  ]);
  assert.equal((await request("/v1/settings")).body.settings.auto_upscale, previous);
  console.log("Built page navigation and cookie-authenticated settings save: passed");

  const created = await request("/v1/library", "POST", {
    source: "comix", source_id: `browser-fixture-${Date.now()}`, title: "Browser fixture",
    cover_url: new URL("/apple-touch-icon.png", baseURL).href,
    description: "Isolated browser test entry", author: "Fixture", genres: [], status: "ongoing",
  });
  assert.equal(created.status, 201);
  libraryId = created.body.id;
  assert.equal((await request(`/auth/public-share/${libraryId}`, "POST")).status, 200);
  const guest = await browser.newContext({ baseURL });
  const guestPage = await guest.newPage();
  guestPage.on("pageerror", (error) => errors.push(error.message));
  await guestPage.goto(`/library/${libraryId}?public=1`);
  await guestPage.getByRole("heading", { name: "Browser fixture", exact: true }).waitFor();
  assert.equal(await guestPage.getByRole("navigation").count(), 0);
  assert.equal((await guest.request.get(`/v1/library/${libraryId}`)).status(), 200);
  assert.equal((await guest.request.get("/v1/settings")).status(), 401);
  assert.equal((await request(`/auth/public-share/${libraryId}`, "DELETE")).status, 204);
  assert.equal((await guest.request.get(`/v1/library/${libraryId}`)).status(), 401);
  await guest.close();
  console.log("Public share browser view, scope and revocation: passed");

  assert.equal((await request(`/v1/library/${libraryId}`, "DELETE")).status, 204);
  libraryId = undefined;
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "Open navigation" }).click();
  await page.getByRole("button", { name: "Sign out", exact: true }).filter({ visible: true }).click();
  await page.waitForURL("**/login");
  assert.equal((await request("/auth/session")).body.user, null);
  assert.equal((await context.request.get(new URL("/v1/sources", baseURL).href)).status(), 401);
  assert.deepEqual(errors, []);
  console.log("Mobile navigation and logout: passed");
} finally {
  if (libraryId) await request(`/v1/library/${libraryId}`, "DELETE").catch(() => {});
  await browser.close();
}
