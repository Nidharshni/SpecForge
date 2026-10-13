// ─── Problem slides ─────────────────────────────────────────────────────────

export const cursorRulesExample = `# .cursorrules (847 lines, manually maintained)
# Last updated: 3 months ago — nobody knows what's stale

## Authentication Rules
- Always check rate limits before auth
- Token expiry should be 15 minutes       # ← contradicts below
- Use the UserStore port, not direct DB access

## Domain Model
- Orders: draft, submitted, approved, shipped
  # "fulfilled" added 2 months ago, missing here
- User roles: admin, editor, viewer
  # "billing_admin" added in Q3, missing here

## Session Management
- JWT tokens expire in 24 hours           # ← CONTRADICTS auth!
  # No validation. No graph. No cross-references.`;

// ─── DSL Syntax Teaching ────────────────────────────────────────────────────

export const specSyntaxBasics = `// Every .spec file:  keyword  name  [title]  { fields }

behavior create_user "Create User" {
  invariants [data_persistence, email_uniqueness]   // reference lists
  ports      [UserRepository, EmailService]         // compiler-resolved
  produces   [user_created]
  category   command                                // scalar string

  contract """
    When a valid CreateUserCommand is received,
    the system MUST create a user record with unique email.
  """

  verify unit        "insert user, retrieve by ID"  // test intent
  verify integration "survives process restart"
  tests ["tests/user_test.go::TestCreateUser"]      // file references
}`;

export const specUseImports = `// File: spec/behaviors/auth.spec
// ─────────────────────────────────────
// Imports bring symbols into scope for reference resolution.
// The compiler validates EVERY reference — typos are errors.

use "invariants/security"                // import all from file
use { Credentials } from "types/auth"    // selective import

behavior rate_limited_auth "Rate-Limited Auth" {
  invariants [auth_token_expiry]   // ← resolved from import
  types      [Credentials]         // ← selective import

  // If "auth_token_expiry" doesn't exist in that file:
  //   error[E001]: unresolved reference
  //     help: did you mean "auth_token_ttl"?
}`;

// ─── Core concept examples ──────────────────────────────────────────────────

export const specBehaviorExample = `behavior rate_limited_auth "Rate-Limited Authentication" {
  invariants [auth_token_expiry, rate_limit_per_ip]
  ports      [UserStore, TokenService, RateLimiter]
  produces   [auth_attempted, auth_succeeded, auth_failed]

  requires { rate_limiter_configured "RateLimiter has valid config" }
  ensures  { rate_limit_enforced "Exceeding threshold returns 429" }

  contract """
    MUST check rate limiter before authenticating.
    If exceeded, return 429 without touching UserStore.
  """

  verify unit        "valid credentials return JWT"
  verify unit        "rate limit exceeded returns 429"
  verify integration "failed attempts emit auth_failed event"
}`;

export const specInvariantExample = `// Invariants declare constraints the system MUST satisfy.
// Behaviors reference invariants — the compiler checks both ends.

invariant auth_token_expiry "Auth Token Expiry" {
  guarantee "JWT tokens MUST expire within 15 minutes"
  risk      critical

  // If no behavior references this invariant → W003
  // If two invariants contradict each other on the same behavior:
  //   "auth_token_expiry" says 15m, "session_duration" says 24h
  //   Both referenced by "user_session" → compiler catches the conflict
}

invariant rate_limit_per_ip "Rate Limit Per IP" {
  guarantee "Max 5 auth attempts per IP per 60 seconds"
  risk      high
}`;

export const specEventExample = `event auth_failed "Authentication Failed" {
  channel      "auth.events"
  channel_type queue              // queue, topic, stream
  category     domain             // domain, integration, system
  payload      AuthFailedPayload  // type ref — compiler-validated
  contract """
    Delivery: at-least-once.
    Consumers must be idempotent on event ID.
  """
}

event order_placed "Order Placed" {
  channel      "orders.events"
  channel_type topic
  category     domain
  payload      OrderPlacedPayload      // Orphan → W007
}`;

export const specPortExample = `port UserStore "User Storage Interface" {
  direction outbound

  method find_by_email(email: string) -> Result<User, NotFound>
  method find_by_id(id: string) -> Result<User, NotFound>
  method create(cmd: CreateUserCommand) -> Result<User, DuplicateEmail> {
    requires { email_available "email not already in use" }
    ensures  { persisted "user exists in storage" }
  }
}

port RateLimiter "Rate Limiting Service" {
  direction outbound
  method check(ip: string) -> Result<bool, ServiceUnavailable>
  method reset(ip: string) -> Result<void, NotFound>
}
// Unused port (no behavior references it) → W005`;

export const specTypeExample = `type OrderItem "Item in an Order" {
  fields {
    product_id  uuid
    quantity    integer
    unit_price  decimal
    discount    decimal?          // ? = optional
  }
}
type User "Application User" {
  fields {
    id       string  @readonly   // annotations
    email    string  @unique
    name     string
    role     string
    tags     string[] @optional
  }
}`;

// ─── Product extension examples ─────────────────────────────────────────────

export const specFeatureJourneyExample = `feature cart_management "Shopping Cart" {
  problem    "Users need to collect items before purchasing"
  solution   "Persistent cart with real-time price updates"
  status     in_progress   // draft → in_progress → done → deprecated (W087)
  priority   high          // critical, high, medium, low
  effort     m             // Fibonacci: xs, s, m, l, xl
  depends_on [rate_limiting]
}
journey checkout_flow "User Checkout" {
  persona  returning_customer
  channels [web_app, mobile]
  features [cart_management, payment_processing, order_confirmation]
  flow [
    "Reviews items in cart",   "Enters shipping address",
    "Selects payment method",  "Reviews and confirms order",
  ]
}`;

export const specPlanningExample = `milestone mvp "MVP Launch" {
  status      in_progress  // planned → in_progress → completed → blocked
  target_date "2026-06-30"
  features    [user_authentication, cart_management, basic_reporting]
  exit_criteria ["All features done", "Coverage >= 90%"]
  depends_on  [infrastructure_setup]
}
deliverable web_app "Web Application" {
  artifact_type webapp
  journeys  [checkout_flow, admin_dashboard_flow]
  modules   [core_auth, web_framework, payment_service]
}
module core_auth "Core Auth" {
  family     platform  // core, platform, extension, integration
  features   [user_authentication, token_management]
  depends_on [crypto_mod]  // module DAG → E007 on cycles
}`;

export const specContextEntitiesExample = `persona returning_customer "Returning Customer" {
  description "Registered user with at least one purchase"
  goals       ["Quick re-order", "Track order status", "Manage payments"]
  pain_points ["Re-entering shipping details", "No delivery estimates"]
}
channel web_app "Web Application" {
  description "Primary browser-based interface"
  platform    web
}
channel mobile "Mobile App" { platform mobile }

release v1_0 "Initial Launch" {
  version      "1.0.0"       // semver — W093 if invalid
  status       planned       // planned → in_progress → released → rolled_back
  deliverables [web_app, mobile_app]
  milestones   [mvp]
}`;

export const specTermExample = `// Terms define the ubiquitous language — the shared vocabulary.
// Agents use terms to generate consistent naming across code.

term cart "Shopping Cart" {
  definition "Temporary collection of items a user intends to purchase"
  aliases    ["basket", "bag"]
  see_also   [order, checkout]
}

term checkout "Checkout Process" {
  definition "The sequence of steps converting a cart into a confirmed order"
  aliases    ["purchase flow"]
  see_also   [cart, order, payment]
}

term order "Customer Order" {
  definition "Confirmed purchase with payment, shipping, and fulfillment tracking"
  see_also   [cart, checkout]
}`;

export const specManifestExample = `// ManifestV2 — how extensions declare capabilities to the compiler.
// This is not a config file — it's a typed protocol.
// The compiler uses this to register keywords, edges, and validators.

{
  "name": "@specforge/software",
  "version": "1.0.0",
  "host_api_version": "^1.0",

  "entity_kinds": [
    {
      "name": "behavior",
      "fields": [
        { "name": "invariants",  "type": "reference_list" },
        { "name": "ports",       "type": "reference_list" },
        { "name": "produces",    "type": "reference_list" },
        { "name": "contract",    "type": "triple_quoted_string" },
        { "name": "category",    "type": "string" }
      ],
      "testable": true,
      "supports_verify": true
    },
    {
      "name": "invariant",
      "fields": [
        { "name": "guarantee", "type": "string" },
        { "name": "risk",      "type": "enum", "values": ["critical","high","medium","low"] }
      ]
    }
    // ... event, type, port similarly declared
  ],

  "edge_types": [
    { "label": "BehaviorEnforcesInvariant", "source": "behavior", "target": "invariant" },
    { "label": "BehaviorUsesPort",          "source": "behavior", "target": "port" },
    { "label": "BehaviorProducesEvent",     "source": "behavior", "target": "event" },
    { "label": "EventCarriesPayloadType",   "source": "event",    "target": "type" }
  ],

  "validation_rules": [
    { "id": "E001", "pattern": "unresolved_reference" },
    { "id": "W003", "pattern": "no_incoming_edges", "target": "invariant" },
    { "id": "W005", "pattern": "no_incoming_edges", "target": "port" },
    { "id": "W007", "pattern": "no_incoming_edges", "target": "event" }
  ],

  "verify_kinds": ["unit", "integration", "property", "e2e", "load"]
}`;

export const specEntityEnhancementExample = `// @specforge/formal's manifest — enhances @specforge/software:
"entity_enhancements": [
  { "target_kind": "behavior",  // owned by @specforge/software
    "added_fields": [
      { "name": "requires", "type": "condition_block" },
      { "name": "ensures",  "type": "condition_block" },
      { "name": "maintains","type": "condition_block" }
    ] },
  { "target_kind": "event",     // owned by @specforge/software
    "added_fields": [
      { "name": "sync", "type": "sync_block" }
    ] }
]
// Result: behaviors now accept requires/ensures/maintains
behavior rate_limited_auth {
  requires { rate_limiter_configured "RateLimiter has valid config" }
}`;

// ─── Governance extension examples ──────────────────────────────────────────

export const specGovernanceExample = `decision use_event_sourcing "Event Sourcing for Orders" {
  status       accepted  // proposed → accepted → deprecated → superseded
  context      """Order state changes must be auditable."""
  decision     """Event sourcing for order aggregate. CQRS for reads."""
  consequences ["Full audit trail", "Replay past state", "Complex reads"]
  invariants   [data_persistence, order_state_integrity]
}
constraint api_latency "API Response Time" {
  category  performance     // performance, security, reliability
  priority  must            // must, should, may (RFC 2119)
  metric    "p99_latency_ms < 200"
  behaviors [create_user, read_user, authenticate]
}
failure_mode write_ack_lost "Write Ack Lost" {
  invariant data_persistence
  severity 8  occurrence 2  detection 3  rpn 48  mitigation "WAL with fsync"
}`;

// ─── Formal extension examples ──────────────────────────────────────────────

export const specFormalExample = `property no_deadlock "No Deadlock" {
  kind safety  description "No processes can be mutually blocked"
}
axiom unique_emails "Email Uniqueness" {
  description "No two active users share the same email"
}
protocol payment_sequence "Payment Processing Order" {
  ordering [payment_initiated, payment_validated, payment_settled]
  timeout "30s"  delivery at_least_once
}
refinement auth_concrete "Auth Refinement" {
  abstract_behavior rate_limited_auth
  concrete_behavior rate_limited_auth_v2
}
process order_lifecycle "Order State Machine" {
  alphabet [order_placed, payment_confirmed, order_shipped]
}`;

// ─── Compiler output ────────────────────────────────────────────────────────

export const compilerErrorsExample = `$ specforge check --path spec/
error[E001]: unresolved reference
 ╭─[spec/auth/login.spec:3:17]
 │ invariants [auth_token_expiry, rate_limit_per_ip]
 │             ╰── "auth_token_expiry" does not exist
 │ help: did you mean "auth_token_ttl"?
 ╰─
error[E006]: event has no producer
 ╭─[spec/payments/events.spec:1:7]
 │ event payment_confirmed — no behavior produces it
 ╰─
warning[W003]: unreferenced invariant
 ╭─[spec/auth/invariants.spec:8:11]
 │ invariant session_duration — not enforced by any behavior
 ╰─
warning[W005]: unreferenced port — W007: orphan event
Checked 14 files · 12 entities, 9 edges · 2 errors, 3 warnings`;

export const graphProtocolJsonExample = `// 8 entities, 7 edges — 2,400 tokens (vs 45,000 from source)
{ "format": "context", "scope": "rate_limited_auth",
  "entities": [
    { "id": "rate_limited_auth", "kind": "behavior",
      "invariants": ["auth_token_expiry", "rate_limit_per_ip"],
      "ports": ["UserStore", "TokenService", "RateLimiter"],
      "produces": ["auth_succeeded", "auth_failed"],
      "contract": "MUST check rate limiter before authenticating." },
    { "id": "auth_token_expiry", "kind": "invariant",
      "guarantee": "JWT tokens MUST expire within 15 minutes" },
    { "id": "rate_limit_per_ip", "kind": "invariant",
      "guarantee": "Max 5 auth attempts per IP per 60 seconds" },
    { "id": "auth_failed", "kind": "event",
      "payload": { "reason": "string", "ip": "string" } } ],
  "edges": [
    { "source": "rate_limited_auth", "target": "auth_token_expiry", "type": "BehaviorEnforcesInvariant" },
    { "source": "rate_limited_auth", "target": "auth_failed", "type": "BehaviorProducesEvent" } ] }`;

// ─── Traceability ───────────────────────────────────────────────────────────

export const traceabilityExample = `$ specforge trace create_user
  feature user_management
    └─ behavior create_user
         ├─ invariant data_persistence ......... ✓ covered
         │    └─ verify property "concurrent writes"
         │         └─ tests/data.prop.ts ....... PASS
         ├─ invariant email_uniqueness ......... ✓ covered
         │    └─ verify unit "duplicate email"
         │         └─ tests/email.unit.ts ...... PASS
         ├─ port UserRepository ................ referenced
         ├─ port EmailService .................. referenced
         ├─ event user_created ................. ✓ covered
         └─ verify
              ├─ unit "insert, retrieve" ....... PASS
              ├─ integration "survives restart"  PASS
              └─ property "email unique" ....... PASS
  Coverage: 3/3 (100%) · All tests passing`;

