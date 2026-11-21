import coreWebVitals from "eslint-config-next/core-web-vitals";

/**
 * Flat config for the web workspace. `core-web-vitals` is the Next.js baseline:
 * React hooks, JSX accessibility, and the `@next/next` rules. Build output is
 * generated, so it is ignored rather than linted.
 */
const config = [
  ...coreWebVitals,
  {
    ignores: [".next/**", "out/**", "node_modules/**"],
  },
  {
    rules: {
      // Every screen loads its data in a mount effect and stores the result in
      // state; that is this codebase's data-loading pattern. The rule is a
      // newer, stricter default than the screens were written against, so it
      // reports instead of blocking. Moving the screens to another pattern is
      // its own change.
      "react-hooks/set-state-in-effect": "warn",
    },
  },
];

export default config;
