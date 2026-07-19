import "@testing-library/jest-dom/vitest";

Object.defineProperty(globalThis, "crypto", {
  configurable: true,
  value: {
    ...globalThis.crypto,
    randomUUID: () => "11111111-2222-4333-8444-555555555555",
  },
});
