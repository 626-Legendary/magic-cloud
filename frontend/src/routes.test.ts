import { describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";
import { routes } from "./routes";

describe("page route constraints", () => {
  it("keeps supported pages and the home redirect", async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      sensitive: true,
      routes,
    });
    for (const [path, name] of [
      ["/", "files"],
      ["/files/root", "files"],
      ["/recent", "recent"],
      ["/shares", "shares"],
      ["/trash", "trash"],
      ["/settings/", "settings"],
      ["/s/example-token?node=example", "public"],
    ]) {
      await router.push(path);
      expect(router.currentRoute.value.name).toBe(name);
    }
  });
  it("rejects unknown names, extra segments, case changes and missing parameters", () => {
    const router = createRouter({
      history: createMemoryHistory(),
      sensitive: true,
      routes,
    });
    for (const path of [
      "/this_is_the_test_route",
      "/abcdefg",
      "/settings/extra",
      "/files",
      "/s",
      "/s/token/extra",
      "/Settings",
      "/files/root/extra",
    ]) {
      expect(router.resolve(path).name, path).toBe("not-found");
    }
  });
});
