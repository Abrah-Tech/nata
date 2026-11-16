import { ApiClient } from "@nata/api-client";

export const api = new ApiClient({
  baseUrl: process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3001",
});
