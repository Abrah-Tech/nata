export type HealthResponse = {
  status: string;
  service: string;
  version: string;
};

export type Role = "owner" | "admin" | "production" | "viewer" | "cashier";

export type TenantSummary = {
  id: string;
  name: string;
  role: Role;
};

export type UnitRef = {
  id: string;
  code: string;
};

export type Unit = UnitRef & {
  name: string;
};

export type Supplier = {
  id: string;
  name: string;
  phone: string | null;
  lead_time_days: number;
  notes: string | null;
  created_at: string;
  updated_at: string;
};

export type Warehouse = {
  id: string;
  name: string;
  created_at: string;
};

export type WarehouseRequest = {
  name: string;
};

export type Material = {
  id: string;
  name: string;
  base_unit: UnitRef;
  min_stock: string;
  shelf_life_days: number | null;
  default_supplier_id: string | null;
  last_cost: string;
  created_at: string;
  updated_at: string;
};

export type Product = {
  id: string;
  name: string;
  sku: string | null;
  sell_price: string;
  /** Days from production until a batch expires; null means it does not. */
  shelf_life_days: number | null;
  /** Cache of every non-empty batch, including expired stock. */
  stock_qty: string;
  /** Stock that can still be sold: quantity left and not past expiry. */
  sellable_qty: string;
  /** Null until the product has a recipe; such a product cannot be produced. */
  current_recipe_id: string | null;
  current_recipe_version: number | null;
  created_at: string;
  updated_at: string;
};

export type ProductRequest = {
  name: string;
  sku?: string | null;
  sell_price?: string;
  shelf_life_days?: number | null;
};

export type RecipeItem = {
  material_id: string;
  material_name: string;
  unit_code: string;
  qty: string;
  waste_pct: string;
};

export type Recipe = {
  id: string;
  product_id: string;
  version: number;
  yield_qty: string;
  labor_minutes: number;
  is_active: boolean;
  created_at: string;
  items: RecipeItem[];
};

export type CreateRecipeRequest = {
  yield_qty: string;
  labor_minutes?: number;
  items: {
    material_id: string;
    qty: string;
    waste_pct?: string;
  }[];
};

export type ProductionRequest = {
  product_id: string;
  qty: string;
  produced_on?: string;
};

export type ProductionBatchPlan = {
  batch_id: string;
  expires_at: string | null;
  qty: string;
  unit_cost: string;
};

export type ProductionMaterialPlan = {
  material_id: string;
  material_name: string;
  unit_code: string;
  required: string;
  available: string;
  shortfall: string;
  batches: ProductionBatchPlan[];
};

export type ProductionPreview = {
  product_id: string;
  product_name: string;
  sell_price: string;
  /** Cache of every non-empty batch; `sellable_qty` is what FEFO can ship. */
  stock_qty: string;
  /** Stock that can still be sold, computed from the batch ledger. */
  sellable_qty: string;
  recipe_id: string;
  recipe_version: number;
  yield_qty: string;
  /** Labor minutes for one batch, and the tenant rates that price them. */
  labor_minutes: number;
  labor_rate_per_hour: string;
  overhead_rate_per_hour: string;
  qty: string;
  can_produce: boolean;
  /** Null when the run is short, because a partial cost would understate HPP. */
  material_cost: string | null;
  labor_cost: string | null;
  overhead_cost: string | null;
  unit_cost: string | null;
  margin: string | null;
  margin_pct: string | null;
  materials: ProductionMaterialPlan[];
};

export type PurchaseRecommendation = {
  material_id: string;
  material_name: string;
  unit_code: string;
  on_hand: string;
  min_stock: string;
  average_daily_usage: string;
  lead_time_days: number;
  supplier_name: string | null;
  shelf_life_days: number | null;
  target_qty: string;
  recommended_qty: string;
  capped_by_shelf_life: boolean;
  /** No outbound movement in the window; the quantity only closes the min-stock gap. */
  insufficient_history: boolean;
  last_cost: string;
  estimated_cost: string;
};

export type RecommendationList = {
  window_days: number;
  items: PurchaseRecommendation[];
};

export type ProductionSuggestion = {
  product_id: string;
  product_name: string;
  average_daily_sales: string;
  /** Stock that can still be sold; expired batches are already excluded. */
  sellable_stock: string;
  /** Sales the run is meant to cover, before the shelf-life cap. */
  target_qty: string;
  recommended_qty: string;
  /** True when shelf life, not the sales target, decided the quantity. */
  capped_by_shelf_life: boolean;
  /** No sales in the window, so there is no rate to plan from. */
  insufficient_history: boolean;
};

export type ProductionSuggestions = {
  days_ahead: number;
  window_days: number;
  products: ProductionSuggestion[];
};

export type PurchasePrice = {
  id: string;
  received_at: string;
  qty: string;
  unit_cost: string;
  /** `qty * unit_cost`, computed by the server with `Decimal`. */
  total: string;
};

export type SupplierPriceHistory = {
  supplier_id: string | null;
  supplier_name: string | null;
  last_price: string;
  lowest_price: string;
  highest_price: string;
  total_qty: string;
  purchases: PurchasePrice[];
};

export type PriceHistory = {
  material_id: string;
  material_name: string;
  unit_code: string;
  suppliers: SupplierPriceHistory[];
};

export type SaleItem = {
  product_id: string;
  product_name: string;
  qty: string;
  price: string;
  subtotal: string;
};

export type Sale = {
  id: string;
  sold_at: string;
  channel: string;
  total: string;
  note: string | null;
  client_ref: string | null;
  created_at: string;
  items: SaleItem[];
};

export type SaleList = {
  items: Sale[];
};

export type SaleLineRequest = {
  product_id: string;
  qty: string;
  /** Omitted uses the product's current sell price. */
  price?: string;
};

export type SaleImportRow = {
  /** Spreadsheet row number, header included. */
  row: number;
  sale_ref: string | null;
  status: "created" | "duplicate" | "rejected";
  sale_id: string | null;
  /** The API error code when the row was rejected. */
  code: string | null;
};

export type SaleImportSummary = {
  /** Sales written, not rows. */
  created: number;
  duplicate: number;
  rejected: number;
  rows: SaleImportRow[];
};

export type SaleRequest = {
  sold_at?: string;
  channel?: string;
  note?: string;
  /** Idempotency key: replaying the same value returns the first sale. */
  client_ref?: string;
  items: SaleLineRequest[];
};

export type StockValue = {
  /** Material batches at the cost they were received at. */
  materials: string;
  /** Finished goods at their most recent production cost. */
  products: string;
  total: string;
};

export type WasteRow = {
  material_id: string;
  material_name: string;
  unit_code: string;
  qty: string;
  value: string;
};

export type WasteValue = {
  value: string;
  movements: number;
  by_material: WasteRow[];
};

export type ProductMargin = {
  product_id: string;
  name: string;
  sell_price: string;
  /** Latest production cost per unit, null when never produced. */
  unit_cost: string | null;
  margin_per_unit: string | null;
  margin_pct: string | null;
  sold_qty: string;
  revenue: string;
  cost_of_goods: string | null;
  margin: string | null;
  /** At least one sold line had no cost snapshot, so the totals are partial. */
  cost_incomplete: boolean;
};

export type ProfitLoss = {
  /** Revenue from every sale in the window. */
  revenue: string;
  /**
   * Cost of the goods sold at the cost snapshotted on each sale. Null when any
   * line in the window has no cost, because an unknown cost is not a zero
   * cost.
   */
  cost_of_goods: string | null;
  /** Revenue minus cost of goods, null while the cost is incomplete. */
  gross_profit: string | null;
  /** At least one sold line had no cost snapshot, so the totals are partial. */
  cost_incomplete: boolean;
};

export type BusinessReport = {
  window_days: number;
  /** Whether the plan includes taking the report away as a file. */
  can_export: boolean;
  profit_loss: ProfitLoss;
  stock: StockValue;
  waste: WasteValue;
  products: ProductMargin[];
};

export type SubscriptionLimits = {
  users: number | null;
  materials: number | null;
  recipes: number | null;
  warehouses: number | null;
};

export type SubscriptionUsage = {
  users: number;
  materials: number;
  recipes: number;
  warehouses: number;
};

/** Whether the tenant may still write. Derived from its dates on the server. */
export type Access = "active" | "grace" | "read_only";

export type Subscription = {
  plan: string;
  /** Reads stay open; only writes are refused once this is `read_only`. */
  access: Access;
  trial_ends_at: string | null;
  /** End of the last verified paid period. */
  paid_until: string | null;
  /** When access runs out, grace included. */
  ends_at: string | null;
  /** Null means the plan does not cap that resource. */
  limits: SubscriptionLimits;
  usage: SubscriptionUsage;
};

export type Payment = {
  id: string;
  amount: string;
  method: string;
  reference: string | null;
  plan: string;
  period_start: string;
  period_end: string;
  status: "pending" | "verified" | "rejected";
  note: string | null;
  verified_at: string | null;
  created_at: string;
};

export type Billing = {
  plan: string;
  access: Access;
  trial_ends_at: string | null;
  paid_until: string | null;
  ends_at: string | null;
  /** Negative once the entitlement has passed. */
  days_left: number | null;
  /** Monthly price of the current plan. Null on the trial. */
  price_per_month: number | null;
  /** Every plan the tenant can buy, so the screen never hardcodes a price. */
  plans: { id: string; price: number }[];
  payments: Payment[];
};

/** The operator still has to check the transfer before it activates anything. */
export type NewPayment = {
  plan: string;
  method: string;
  /** One to twelve. Defaults to one. */
  months?: number;
  /** Defaults to the plan price times the months. */
  amount?: string;
  reference?: string;
  note?: string;
};

export type TenantSettings = {
  labor_rate_per_hour: string;
  /** Priced on the same recipe minutes as labor. */
  overhead_rate_per_hour: string;
  /** Days of movement history purchase recommendations average over. */
  usage_window_days: number;
};

/** Omitted fields keep their stored value. */
export type TenantSettingsUpdate = {
  labor_rate_per_hour?: string;
  overhead_rate_per_hour?: string;
  usage_window_days?: number;
};

export type ProductionConsumption = {
  batch_id: string;
  material_id: string;
  material_name: string;
  qty: string;
  unit_cost: string;
};

export type ProductionOrder = {
  id: string;
  product_id: string;
  product_name: string;
  recipe_id: string;
  recipe_version: number;
  planned_qty: string;
  actual_qty: string;
  material_cost: string;
  labor_cost: string;
  overhead_cost: string;
  unit_cost: string;
  produced_on: string;
  status: string;
  created_at: string;
  consumptions: ProductionConsumption[];
};

export type StockLevel = {
  material_id: string;
  name: string;
  base_unit: UnitRef;
  on_hand: string;
  min_stock: string;
  below_min: boolean;
  active_batches: number;
  nearest_expiry: string | null;
  last_cost: string;
};

export type Batch = {
  id: string;
  material_id: string;
  supplier_id: string | null;
  received_at: string;
  expires_at: string | null;
  qty_received: string;
  qty_remaining: string;
  unit_cost: string;
  note: string | null;
  created_at: string;
};

export type ProductBatch = {
  id: string;
  product_id: string;
  product_name: string;
  production_order_id: string | null;
  produced_on: string;
  expires_at: string | null;
  qty_received: string;
  qty_remaining: string;
  unit_cost: string | null;
  created_at: string;
};

export type ProductCorrectionType = "waste" | "expired" | "adjustment";

export type ProductCorrectionRequest = {
  product_batch_id: string;
  type: ProductCorrectionType;
  /** Signed like every movement: outbound is negative, inbound positive. */
  qty: string;
  note?: string | null;
};

export type ProductCorrection = {
  id: string;
  product_batch_id: string;
  product_id: string;
  type: ProductCorrectionType;
  qty: string;
  unit_cost: string | null;
  note: string | null;
  created_by: string | null;
  created_at: string;
  batch_qty_remaining: string;
};

export type MovementType =
  | "purchase"
  | "production"
  | "production_use"
  | "sale"
  | "waste"
  | "expired"
  | "adjustment";

export type Movement = {
  id: string;
  /** Material batch; null for a finished-goods movement. */
  batch_id: string | null;
  material_id: string | null;
  material_name: string | null;
  unit_code: string | null;
  /** Finished-goods batch; null for a material movement. */
  product_batch_id: string | null;
  product_id: string | null;
  product_name: string | null;
  type: MovementType;
  qty: string;
  unit_cost: string | null;
  note: string | null;
  created_by: string | null;
  created_at: string;
};

export type AlertRule = {
  id: string;
  type: "expiry" | "low_stock";
  /** Days before expiry; null for a low-stock rule. */
  days_before: number | null;
  channel: "whatsapp" | "email";
  is_active: boolean;
  created_at: string;
};

/** Omitted fields keep their stored value. */
export type AlertRuleUpdate = {
  days_before?: number;
  channel?: "whatsapp" | "email";
  is_active?: boolean;
};

export type Alert = {
  id: string;
  type: "expiry" | "low_stock";
  /** `open` until the condition stops holding, then `resolved`. */
  status: "open" | "resolved";
  rule_id: string;
  material_id: string | null;
  material_name: string | null;
  batch_id: string | null;
  product_id: string | null;
  product_name: string | null;
  product_batch_id: string | null;
  qty_remaining: string | null;
  expires_at: string | null;
  days_left: number | null;
  /** Usable stock for a low-stock alert; null for an expiry alert. */
  on_hand: string | null;
  min_stock: string | null;
  created_at: string;
  resolved_at: string | null;
  sent_at: string | null;
  delivery_status: "pending" | "sent" | "failed" | "skipped";
  attempts: number;
};

/**
 * The message the next alert run would send, composed by the server so the
 * preview cannot drift from the text that actually goes out.
 */
export type AlertPreview = {
  subject: string;
  body: string;
  /** How many alerts the digest covers; zero means nothing is waiting. */
  count: number;
};

/** One entry of the tenant's audit log. Owner-only on the server. */
export type AuditEntry = {
  id: string;
  /** Null when the actor is not a member, such as the operator verifying a payment. */
  actor_user_id: string | null;
  actor_name: string | null;
  /** Dotted action name, e.g. `member.role_changed`. */
  action: string;
  entity_type: string;
  entity_id: string | null;
  /** Schemaless per-action detail; before/after values, the operator label, the amount. */
  metadata: Record<string, unknown>;
  created_at: string;
};

/** One membership of the signed-in tenant. */
export type Member = {
  user_id: string;
  name: string;
  email: string | null;
  phone: string | null;
  role: Role;
  /** `active` or `suspended`. */
  status: string;
  joined_at: string;
};

/** A pending invitation. The list endpoint returns pending rows only. */
export type Invitation = {
  id: string;
  email: string | null;
  phone: string | null;
  role: Role;
  /** Always `pending` from the list endpoint. */
  status: string;
  expires_at: string;
  created_at: string;
};

/**
 * The create response. `token` is returned once, the way an OTP code is: only
 * its hash is stored, so the caller has to show it now or it is lost.
 */
export type InvitationCreated = Invitation & {
  token: string;
};

export type CreateInvitationRequest = {
  /** An email address or a phone number, matched against the invitee's contact. */
  contact: string;
  role: Role;
};

export type AcceptInvitationRequest = {
  /** The 64-character code from the invitation link. */
  token: string;
};

export type StockInRequest = {
  material_id: string;
  supplier_id?: string | null;
  received_at?: string;
  expires_at?: string | null;
  qty_received: string;
  unit_cost: string;
  note?: string | null;
  /** Idempotency key for offline replay; the server returns the first batch. */
  client_ref?: string | null;
};

export type StockCountSheet = {
  note?: string | null;
  counts: {
    batch_id: string;
    /** What was counted, an absolute quantity and never a difference. */
    qty: string;
  }[];
};

export type StockCountResult = {
  applied: number;
  unchanged: number;
};

export type SyncResult = {
  client_ref: string | null;
  status: "created" | "duplicate" | "rejected";
  batch_id: string | null;
  code: string | null;
};

export type SyncResponse = {
  results: SyncResult[];
};

export type CreateMaterialRequest = {
  name: string;
  base_unit_id: string;
  min_stock?: string;
  shelf_life_days?: number | null;
  default_supplier_id?: string | null;
};

export type DashboardSummary = {
  materials: number;
  below_min: number;
  out_of_stock: number;
  expiring: number;
  expired: number;
  active_batches: number;
  stock_value: string;
  horizon_days: number;
};

export type DashboardCriticalRow = {
  material_id: string;
  name: string;
  base_unit: UnitRef;
  on_hand: string;
  min_stock: string;
  shortfall: string;
  active_batches: number;
  nearest_expiry: string | null;
};

export type DashboardExpiringRow = {
  batch_id: string;
  material_id: string;
  material_name: string;
  base_unit: UnitRef;
  qty_remaining: string;
  unit_cost: string;
  expires_at: string;
  days_left: number;
};

export type DashboardBestSeller = {
  product_id: string;
  name: string;
  qty: string;
  revenue: string;
};

export type DashboardSales = {
  revenue: string;
  cost_of_goods: string | null;
  profit: string | null;
  cost_incomplete: boolean;
  yesterday_revenue: string;
  yesterday_profit: string | null;
  best_sellers: DashboardBestSeller[];
};

export type Dashboard = {
  summary: DashboardSummary;
  critical: DashboardCriticalRow[];
  expiring: DashboardExpiringRow[];
  sales: DashboardSales | null;
};

export type OnboardingTemplateItem = {
  name: string;
  unit: string;
  min_stock: string;
  shelf_life_days: number | null;
};

export type OnboardingTemplate = {
  id: string;
  item_count: number;
  product_count: number;
  items: OnboardingTemplateItem[];
};

export type OnboardingRowError = {
  row: number;
  code: string;
};

export type OnboardingResult = {
  created: number;
  skipped: number;
  errors: OnboardingRowError[];
  products: number;
  recipes: number;
};

export type AuthResponse = {
  access_token: string;
  token_type: string;
  expires_in: number;
  requires_tenant_selection: boolean;
  pending_invitation: boolean;
  tenants: TenantSummary[];
};

export type MeResponse = {
  user: {
    id: string;
    name: string;
    email: string | null;
    phone: string | null;
  };
  tenant: {
    id: string;
    name: string;
  };
  role: Role;
  permissions: string[];
};

export type OtpRequestResponse = {
  /** Seconds the code is valid for, or the remainder when one is pending. */
  expires_in: number;
  /**
   * True when a still-valid code already existed and no new one was sent. The
   * caller should go to the code-entry screen rather than wait for a message.
   */
  pending?: boolean;
};

export type VerifyOtpRequest = {
  contact: string;
  code: string;
  name?: string;
  tenant_name?: string;
};

export type ApiClientOptions = {
  baseUrl: string;
  fetchImpl?: typeof fetch;
};

type ErrorBody = {
  error?: {
    code?: string;
    message?: string;
    details?: unknown;
  };
};

/** A non-2xx response. Carries the API error code so the UI can map it. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  /** Structured payload for conflicts, such as the shortage list. */
  readonly details: unknown;

  constructor(status: number, code: string, message: string, details?: unknown) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.details = details;
  }
}

type RequestOptions = {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  /** Raw upload, sent as-is instead of JSON. */
  raw?: BodyInit;
  token?: string;
  signal?: AbortSignal;
};

export class ApiClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;

  constructor({ baseUrl, fetchImpl = fetch }: ApiClientOptions) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    // Native fetch throws "Illegal invocation" when called as a method of
    // another object, so keep it bound to the global scope.
    this.fetchImpl = fetchImpl.bind(globalThis);
  }

  private async request<T>(path: string, options: RequestOptions = {}): Promise<T> {
    const headers = new Headers();

    if (options.body !== undefined) {
      headers.set("content-type", "application/json");
    }

    if (options.raw !== undefined) {
      headers.set("content-type", "application/octet-stream");
    }

    if (options.token) {
      headers.set("authorization", `Bearer ${options.token}`);
    }

    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method: options.method ?? "GET",
      headers,
      body:
        options.raw ??
        (options.body === undefined ? undefined : JSON.stringify(options.body)),
      signal: options.signal,
    });

    if (response.status === 204) {
      return undefined as T;
    }

    const payload = (await response.json().catch(() => null)) as (T & ErrorBody) | null;

    if (!response.ok) {
      const error = payload?.error;
      throw new ApiError(
        response.status,
        error?.code ?? "request_failed",
        error?.message ?? `Request failed with status ${response.status}`,
        error?.details,
      );
    }

    return payload as T;
  }

  health(signal?: AbortSignal): Promise<HealthResponse> {
    return this.request<HealthResponse>("/healthz", { signal });
  }

  requestOtp(contact: string): Promise<OtpRequestResponse> {
    return this.request<OtpRequestResponse>("/auth/otp/request", {
      method: "POST",
      body: { contact },
    });
  }

  verifyOtp(body: VerifyOtpRequest): Promise<AuthResponse> {
    return this.request<AuthResponse>("/auth/otp/verify", {
      method: "POST",
      body,
    });
  }

  listTenants(token: string): Promise<TenantSummary[]> {
    return this.request<TenantSummary[]>("/auth/tenants", { token });
  }

  selectTenant(token: string, tenantId: string): Promise<AuthResponse> {
    return this.request<AuthResponse>("/auth/select-tenant", {
      method: "POST",
      token,
      body: { tenant_id: tenantId },
    });
  }

  me(token: string): Promise<MeResponse> {
    return this.request<MeResponse>("/me", { token });
  }

  listMembers(token: string): Promise<Member[]> {
    return this.request<Member[]>("/members", { token });
  }

  changeMemberRole(
    token: string,
    userId: string,
    role: Role,
  ): Promise<Member> {
    return this.request<Member>(`/members/${userId}/role`, {
      method: "PATCH",
      token,
      body: { role },
    });
  }

  changeMemberStatus(
    token: string,
    userId: string,
    status: "active" | "suspended",
  ): Promise<Member> {
    return this.request<Member>(`/members/${userId}/status`, {
      method: "PATCH",
      token,
      body: { status },
    });
  }

  removeMember(token: string, userId: string): Promise<void> {
    return this.request<void>(`/members/${userId}`, {
      method: "DELETE",
      token,
    });
  }

  listInvitations(token: string): Promise<Invitation[]> {
    return this.request<Invitation[]>("/members/invitations", { token });
  }

  createInvitation(
    token: string,
    body: CreateInvitationRequest,
  ): Promise<InvitationCreated> {
    return this.request<InvitationCreated>("/members/invitations", {
      method: "POST",
      token,
      body,
    });
  }

  revokeInvitation(token: string, id: string): Promise<void> {
    return this.request<void>(`/members/invitations/${id}`, {
      method: "DELETE",
      token,
    });
  }

  /**
   * Joins the tenant that sent the invitation. Any bearer token works, because
   * an invitee may still hold a tenant-selection token with no membership. The
   * response carries a new session token for the joined tenant, so the caller
   * has to store it.
   */
  acceptInvitation(
    token: string,
    body: AcceptInvitationRequest,
  ): Promise<AuthResponse> {
    return this.request<AuthResponse>("/auth/accept-invitation", {
      method: "POST",
      token,
      body,
    });
  }

  listSales(token: string, limit?: number): Promise<SaleList> {
    const query = limit ? `?limit=${limit}` : "";

    return this.request<SaleList>(`/sales${query}`, { token });
  }

  createSale(token: string, body: SaleRequest): Promise<Sale> {
    return this.request<Sale>("/sales", {
      method: "POST",
      token,
      body,
    });
  }

  /**
   * Imports a CSV. `tzOffset` is minutes east of UTC, used for timestamps
   * written without an offset.
   */
  importSales(
    token: string,
    csv: string,
    tzOffset: number,
  ): Promise<SaleImportSummary> {
    return this.request<SaleImportSummary>(
      `/sales/import?format=csv&tz_offset=${tzOffset}`,
      { method: "POST", token, raw: csv },
    );
  }

  getSubscription(token: string): Promise<Subscription> {
    return this.request<Subscription>("/subscription", { token });
  }

  getBilling(token: string): Promise<Billing> {
    return this.request<Billing>("/billing", { token });
  }

  createPayment(token: string, body: NewPayment): Promise<Payment> {
    return this.request<Payment>("/billing/payments", {
      method: "POST",
      token,
      body,
    });
  }

  deletePayment(token: string, id: string): Promise<void> {
    return this.request<void>(`/billing/payments/${id}`, {
      method: "DELETE",
      token,
    });
  }

  getBusinessReport(token: string): Promise<BusinessReport> {
    return this.request<BusinessReport>("/reports", { token });
  }

  listRecommendations(token: string): Promise<RecommendationList> {
    return this.request<RecommendationList>("/purchasing/recommendations", {
      token,
    });
  }

  priceHistory(token: string, materialId: string): Promise<PriceHistory> {
    return this.request<PriceHistory>(
      `/purchasing/price-history?material_id=${encodeURIComponent(materialId)}`,
      { token },
    );
  }

  getSettings(token: string): Promise<TenantSettings> {
    return this.request<TenantSettings>("/settings", { token });
  }

  updateSettings(
    token: string,
    body: TenantSettingsUpdate,
  ): Promise<TenantSettings> {
    return this.request<TenantSettings>("/settings", {
      method: "PUT",
      token,
      body,
    });
  }

  listUnits(token: string): Promise<Unit[]> {
    return this.request<Unit[]>("/units", { token });
  }

  listSuppliers(token: string): Promise<Supplier[]> {
    return this.request<Supplier[]>("/suppliers", { token });
  }

  listWarehouses(token: string): Promise<Warehouse[]> {
    return this.request<Warehouse[]>("/warehouses", { token });
  }

  createWarehouse(token: string, body: WarehouseRequest): Promise<Warehouse> {
    return this.request<Warehouse>("/warehouses", {
      method: "POST",
      token,
      body,
    });
  }

  updateWarehouse(
    token: string,
    id: string,
    body: WarehouseRequest,
  ): Promise<Warehouse> {
    return this.request<Warehouse>(`/warehouses/${id}`, {
      method: "PUT",
      token,
      body,
    });
  }

  removeWarehouse(token: string, id: string): Promise<void> {
    return this.request<void>(`/warehouses/${id}`, { method: "DELETE", token });
  }

  listMaterials(token: string): Promise<Material[]> {
    return this.request<Material[]>("/materials", { token });
  }

  createMaterial(token: string, body: CreateMaterialRequest): Promise<Material> {
    return this.request<Material>("/materials", {
      method: "POST",
      token,
      body,
    });
  }

  listProducts(token: string): Promise<Product[]> {
    return this.request<Product[]>("/products", { token });
  }

  createProduct(token: string, body: ProductRequest): Promise<Product> {
    return this.request<Product>("/products", {
      method: "POST",
      token,
      body,
    });
  }

  updateProduct(token: string, id: string, body: ProductRequest): Promise<Product> {
    return this.request<Product>(`/products/${id}`, {
      method: "PUT",
      token,
      body,
    });
  }

  removeProduct(token: string, id: string): Promise<void> {
    return this.request<void>(`/products/${id}`, { method: "DELETE", token });
  }

  listRecipes(token: string, productId: string): Promise<Recipe[]> {
    return this.request<Recipe[]>(`/products/${productId}/recipes`, { token });
  }

  createRecipe(
    token: string,
    productId: string,
    body: CreateRecipeRequest,
  ): Promise<Recipe> {
    return this.request<Recipe>(`/products/${productId}/recipes`, {
      method: "POST",
      token,
      body,
    });
  }

  getRecipe(token: string, id: string): Promise<Recipe> {
    return this.request<Recipe>(`/recipes/${id}`, { token });
  }

  previewProduction(
    token: string,
    body: ProductionRequest,
  ): Promise<ProductionPreview> {
    return this.request<ProductionPreview>("/production-orders/preview", {
      method: "POST",
      token,
      body,
    });
  }

  createProduction(
    token: string,
    body: ProductionRequest,
  ): Promise<ProductionOrder> {
    return this.request<ProductionOrder>("/production-orders", {
      method: "POST",
      token,
      body,
    });
  }

  productionSuggestions(
    token: string,
    daysAhead: number,
  ): Promise<ProductionSuggestions> {
    return this.request<ProductionSuggestions>(
      `/production/suggestions?days=${encodeURIComponent(daysAhead)}`,
      { token },
    );
  }

  listOnboardingTemplates(token: string): Promise<OnboardingTemplate[]> {
    return this.request<OnboardingTemplate[]>("/onboarding/templates", { token });
  }

  applyOnboardingTemplate(token: string, id: string): Promise<OnboardingResult> {
    return this.request<OnboardingResult>(
      `/onboarding/templates/${encodeURIComponent(id)}/apply`,
      { method: "POST", token },
    );
  }

  importMaterials(
    token: string,
    bytes: BodyInit,
    format: "xlsx" | "csv",
  ): Promise<OnboardingResult> {
    return this.request<OnboardingResult>(
      `/onboarding/import?format=${format}`,
      { method: "POST", raw: bytes, token },
    );
  }

  getDashboard(token: string): Promise<Dashboard> {
    return this.request<Dashboard>("/dashboard", { token });
  }

  listStock(token: string): Promise<StockLevel[]> {
    return this.request<StockLevel[]>("/stock", { token });
  }

  listBatches(token: string, materialId?: string): Promise<Batch[]> {
    const query = materialId ? `?material_id=${encodeURIComponent(materialId)}` : "";

    return this.request<Batch[]>(`/stock/batches${query}`, { token });
  }

  listMovements(
    token: string,
    filter: {
      materialId?: string;
      batchId?: string;
      productId?: string;
      productBatchId?: string;
      limit?: number;
    } = {},
  ): Promise<Movement[]> {
    const query = new URLSearchParams();

    if (filter.materialId) query.set("material_id", filter.materialId);
    if (filter.batchId) query.set("batch_id", filter.batchId);
    if (filter.productId) query.set("product_id", filter.productId);
    if (filter.productBatchId) query.set("product_batch_id", filter.productBatchId);
    if (filter.limit) query.set("limit", String(filter.limit));

    const suffix = query.size > 0 ? `?${query.toString()}` : "";

    return this.request<Movement[]>(`/stock/movements${suffix}`, { token });
  }

  stockIn(token: string, body: StockInRequest): Promise<Batch> {
    return this.request<Batch>("/stock/batches", {
      method: "POST",
      token,
      body,
    });
  }

  applyStockCount(token: string, body: StockCountSheet): Promise<StockCountResult> {
    return this.request<StockCountResult>("/stock/opname", {
      method: "POST",
      token,
      body,
    });
  }

  listProductBatches(token: string, productId?: string): Promise<ProductBatch[]> {
    const query = productId ? `?product_id=${encodeURIComponent(productId)}` : "";

    return this.request<ProductBatch[]>(`/stock/product-batches${query}`, { token });
  }

  correctProduct(
    token: string,
    body: ProductCorrectionRequest,
  ): Promise<ProductCorrection> {
    return this.request<ProductCorrection>("/stock/product-corrections", {
      method: "POST",
      token,
      body,
    });
  }

  syncStockIn(
    token: string,
    entries: StockInRequest[],
  ): Promise<SyncResponse> {
    return this.request<SyncResponse>("/sync/stock-in", {
      method: "POST",
      token,
      body: { entries },
    });
  }

  listAlerts(
    token: string,
    filter: { status?: "open" | "resolved"; limit?: number } = {},
  ): Promise<Alert[]> {
    const query = new URLSearchParams();

    if (filter.status) query.set("status", filter.status);
    if (filter.limit) query.set("limit", String(filter.limit));

    const suffix = query.size > 0 ? `?${query.toString()}` : "";

    return this.request<Alert[]>(`/alerts${suffix}`, { token });
  }

  /** The digest the next run would send. `count` 0 means nothing is waiting. */
  previewAlerts(token: string): Promise<AlertPreview> {
    return this.request<AlertPreview>("/alerts/preview", { token });
  }

  listAlertRules(token: string): Promise<AlertRule[]> {
    return this.request<AlertRule[]>("/alert-rules", { token });
  }

  updateAlertRule(
    token: string,
    ruleId: string,
    body: AlertRuleUpdate,
  ): Promise<AlertRule> {
    return this.request<AlertRule>(`/alert-rules/${ruleId}`, {
      method: "PATCH",
      token,
      body,
    });
  }

  listAuditLogs(token: string, limit?: number): Promise<AuditEntry[]> {
    const query = new URLSearchParams();
    if (limit) query.set("limit", String(limit));

    const suffix = query.size > 0 ? `?${query.toString()}` : "";

    return this.request<AuditEntry[]>(`/audit-logs${suffix}`, { token });
  }
}
