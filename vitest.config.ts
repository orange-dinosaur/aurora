import { defineConfig } from "vitest/config";

// The round-trip tests drive a headless Lexical editor, which needs no DOM, so
// there is nothing here to configure an environment with.
export default defineConfig({
	test: {
		include: ["src/**/*.test.ts"],
		environment: "node",
	},
});
