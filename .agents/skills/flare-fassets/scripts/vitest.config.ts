import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["execute-direct-mint.helpers.test.ts"],
  },
});
