import type { MeResponse } from "@nata/api-client";
import { describe, expect, test } from "bun:test";

import { homePath } from "../lib/landing";

function me(role: MeResponse["role"]): MeResponse {
  return {
    user: {
      id: "00000000-0000-0000-0000-000000000000",
      name: "Test",
      email: null,
      phone: null,
    },
    tenant: {
      id: "00000000-0000-0000-0000-000000000000",
      name: "Test Tenant",
    },
    role,
    permissions: [],
  };
}

describe("homePath", () => {
  test("sends a cashier to the register, because the overview reads materials", () => {
    expect(homePath(me("cashier"))).toBe("/sales");
  });

  test("sends every other role to the overview", () => {
    for (const role of ["owner", "admin", "production", "viewer"] as const) {
      expect(homePath(me(role))).toBe("/dashboard");
    }
  });
});
