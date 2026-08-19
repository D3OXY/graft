// PROTOTYPE — throwaway. Fake data for ticket 015 "Prototype the hub UI".
// One repo (acme/api), continuing the day from the CLI prototype (ticket 011).
// Everything here obeys the ratified model: change IDs are letters, revisions are
// hex, proposals are stacks reviewed change-by-change, and the hub only ever sees
// keyring *metadata* for secrets — never names, values, or counts.

export type Revision = {
  hash: string; // hex revision id (content hash)
  label: string; // r1, r2 …
  when: string;
  note?: string; // one-line gist of what this amend changed
};

export type Change = {
  id: string; // stable letter change ID
  author: string;
  when: string;
  message: string;
  revisions: Revision[]; // oldest → newest; last one is current
  branches?: string[]; // branch heads pointing at this change
  inProposal?: number; // proposal id this change is part of, if any
};

// ── log (hub view: main + payments) ─────────────────────────────────────────

export const changes: Change[] = [
  {
    id: "znvatko",
    author: "ishan",
    when: "3h",
    message: "merge main",
    revisions: [{ hash: "5d90ab", label: "r1", when: "3h" }],
    branches: ["payments"],
    inProposal: 218,
  },
  {
    id: "tzunmyk",
    author: "ishan",
    when: "7h",
    message: "payments: add invoice webhook",
    revisions: [
      { hash: "b02f77", label: "r1", when: "7h" },
      { hash: "c7e1b9", label: "r2", when: "5h", note: "added the retry header" },
    ],
    inProposal: 218,
  },
  {
    id: "povwkil",
    author: "mia",
    when: "6h",
    message: "rate limiter: burst metrics",
    revisions: [{ hash: "e41c02", label: "r1", when: "6h" }],
    branches: ["main"],
  },
  {
    id: "mqkosuwt",
    author: "ishan",
    when: "8h",
    message: "limiter: fix burst off-by-one",
    revisions: [{ hash: "91d4e0", label: "r1", when: "8h" }],
  },
  {
    id: "xtpolzn",
    author: "mia",
    when: "10h",
    message: "rate limiter: clamp burst window",
    revisions: [{ hash: "8f3a2c", label: "r1", when: "10h" }],
  },
  {
    id: "rwqnves",
    author: "devon",
    when: "1d",
    message: "invoices: idempotency keys",
    revisions: [{ hash: "44aa19", label: "r1", when: "1d" }],
  },
  {
    id: "kplomzu",
    author: "mia",
    when: "2d",
    message: "invoices: schema for line items",
    revisions: [{ hash: "0be3d7", label: "r1", when: "2d" }],
  },
];

export const changeById = (id: string) => changes.find((c) => c.id === id)!;

// ── tree + files ─────────────────────────────────────────────────────────────

export type TreeNode = {
  name: string;
  path: string;
  kind: "dir" | "file";
  children?: TreeNode[];
  lastChange?: string; // change id that last touched this path
};

export const tree: TreeNode[] = [
  {
    name: "src",
    path: "src",
    kind: "dir",
    children: [
      {
        name: "invoices",
        path: "src/invoices",
        kind: "dir",
        children: [
          { name: "mod.rs", path: "src/invoices/mod.rs", kind: "file", lastChange: "kplomzu" },
          { name: "idempotency.rs", path: "src/invoices/idempotency.rs", kind: "file", lastChange: "rwqnves" },
        ],
      },
      { name: "limiter.rs", path: "src/limiter.rs", kind: "file", lastChange: "znvatko" },
      { name: "webhooks.rs", path: "src/webhooks.rs", kind: "file", lastChange: "tzunmyk" },
      { name: "main.rs", path: "src/main.rs", kind: "file", lastChange: "kplomzu" },
    ],
  },
  { name: "Cargo.toml", path: "Cargo.toml", kind: "file", lastChange: "kplomzu" },
  { name: "README.md", path: "README.md", kind: "file", lastChange: "xtpolzn" },
];

export const fileContents: Record<string, string[]> = {
  "src/limiter.rs": [
    "use std::time::Instant;",
    "",
    "const WINDOW: u32 = 60;",
    "",
    "pub struct Limiter {",
    "    burst: u32,",
    "    seen: Vec<Instant>,",
    "}",
    "",
    "impl Limiter {",
    "    pub fn new(cfg: &Config) -> Self {",
    "        let burst = clamp_burst(cfg.burst, WINDOW);",
    "        Self { burst, seen: Vec::new() }",
    "    }",
    "",
    "    pub fn allow(&mut self, now: Instant) -> bool {",
    "        self.seen.retain(|t| now.duration_since(*t).as_secs() < WINDOW as u64);",
    "        (self.seen.len() as u32) < self.burst && { self.seen.push(now); true }",
    "    }",
    "}",
    "",
    "fn clamp_burst(burst: u32, window: u32) -> u32 {",
    "    burst.min(window - 1) // off-by-one fixed in mqkosuwt",
    "}",
  ],
  "README.md": [
    "# acme/api",
    "",
    "Billing + rate limiting service.",
    "",
    "Secrets are managed with `graft env` — there is no .env file to copy.",
    "Run locally with `graft env run dev -- cargo run`.",
  ],
};

// ── proposals ────────────────────────────────────────────────────────────────

export type Thread = {
  id: number;
  changeId: string;
  revision: string; // revision hash the comment was anchored on
  path: string;
  line: number;
  resolved: boolean;
  comments: { author: string; when: string; body: string }[];
};

export type ProposalEvent =
  | { kind: "opened"; who: string; when: string }
  | { kind: "comment"; who: string; when: string; body: string }
  | { kind: "amend"; who: string; when: string; changeId: string; from: string; to: string; note: string }
  | { kind: "approve"; who: string; when: string; revision: string };

export type Proposal = {
  id: number;
  title: string;
  author: string;
  when: string;
  target: string;
  status: "open" | "landed" | "closed";
  stack: string[]; // change ids, bottom → top
  trialRebase: "clean" | "would-conflict";
  conflictFiles?: string[];
  approvals: { who: string; revision: string }[];
  events: ProposalEvent[];
  threads: Thread[];
};

export const proposals: Proposal[] = [
  {
    id: 218,
    title: "payments: add invoice webhook",
    author: "ishan",
    when: "3h",
    target: "main",
    status: "open",
    stack: ["tzunmyk", "znvatko"],
    trialRebase: "clean",
    approvals: [{ who: "mia", revision: "c7e1b9" }],
    events: [
      { kind: "opened", who: "ishan", when: "3h" },
      {
        kind: "comment",
        who: "devon",
        when: "2h",
        body: "Do we retry on 5xx from the billing provider, or is one delivery attempt the contract?",
      },
      {
        kind: "amend",
        who: "ishan",
        when: "2h",
        changeId: "tzunmyk",
        from: "b02f77",
        to: "c7e1b9",
        note: "added the retry header",
      },
      { kind: "approve", who: "mia", when: "1h", revision: "c7e1b9" },
    ],
    threads: [
      {
        id: 1,
        changeId: "tzunmyk",
        revision: "b02f77",
        path: "src/webhooks.rs",
        line: 42,
        resolved: true,
        comments: [
          { author: "devon", when: "6h", body: "This needs the Retry-After header or the queue will hammer us." },
          { author: "ishan", when: "5h", body: "Good catch — amended, see the interdiff." },
        ],
      },
      {
        id: 2,
        changeId: "tzunmyk",
        revision: "c7e1b9",
        path: "src/webhooks.rs",
        line: 57,
        resolved: false,
        comments: [
          { author: "mia", when: "1h", body: "Idempotency key per invoice or per delivery attempt?" },
          { author: "ishan", when: "45m", body: "Per invoice — an attempt that half-succeeded must not double-bill." },
        ],
      },
    ],
  },
  {
    id: 214,
    title: "invoices: proration edge cases",
    author: "devon",
    when: "2d",
    target: "main",
    status: "open",
    stack: ["rwqnves"],
    trialRebase: "would-conflict",
    conflictFiles: ["src/invoices/mod.rs"],
    approvals: [],
    events: [{ kind: "opened", who: "devon", when: "2d" }],
    threads: [],
  },
  {
    id: 209,
    title: "rate limiter: clamp burst window",
    author: "mia",
    when: "10h",
    target: "main",
    status: "landed",
    stack: ["xtpolzn"],
    trialRebase: "clean",
    approvals: [{ who: "ishan", revision: "8f3a2c" }],
    events: [{ kind: "opened", who: "mia", when: "12h" }],
    threads: [],
  },
];

export const proposalById = (id: number) => proposals.find((p) => p.id === id)!;

// ── diffs ────────────────────────────────────────────────────────────────────

export type DiffLine = { kind: "ctx" | "add" | "del" | "hunk"; text: string; old?: number; new?: number };
export type FileDiff = { path: string; status: "added" | "modified"; lines: DiffLine[] };

// diff of each change vs its parent (current revision)
export const changeDiffs: Record<string, FileDiff[]> = {
  tzunmyk: [
    {
      path: "src/webhooks.rs",
      status: "added",
      lines: [
        { kind: "hunk", text: "@@ -0,0 +38,24 @@ pub fn deliver_invoice_webhook" },
        { kind: "add", new: 38, text: "pub fn deliver_invoice_webhook(inv: &Invoice, ep: &Endpoint) -> Result<()> {" },
        { kind: "add", new: 39, text: "    let payload = serde_json::to_vec(&WebhookBody::from(inv))?;" },
        { kind: "add", new: 40, text: "    let sig = sign(&payload, &ep.signing_key);" },
        { kind: "add", new: 41, text: "    let resp = post(ep.url.clone())" },
        { kind: "add", new: 42, text: "        .header(\"Retry-After\", backoff_secs(inv.attempts).to_string())" },
        { kind: "add", new: 43, text: "        .header(\"X-Acme-Signature\", sig)" },
        { kind: "add", new: 44, text: "        .body(payload)" },
        { kind: "add", new: 45, text: "        .send()?;" },
        { kind: "add", new: 46, text: "    ensure!(resp.status().is_success(), DeliveryFailed(resp.status()));" },
        { kind: "add", new: 47, text: "    Ok(())" },
        { kind: "add", new: 48, text: "}" },
        { kind: "hunk", text: "@@ -0,0 +55,6 @@ fn idempotency_key" },
        { kind: "add", new: 55, text: "/// One key per invoice: a half-delivered attempt must not double-bill." },
        { kind: "add", new: 56, text: "fn idempotency_key(inv: &Invoice) -> String {" },
        { kind: "add", new: 57, text: "    format!(\"inv_{}\", inv.id)" },
        { kind: "add", new: 58, text: "}" },
      ],
    },
  ],
  znvatko: [
    {
      path: "src/limiter.rs",
      status: "modified",
      lines: [
        { kind: "hunk", text: "@@ -10,7 +10,7 @@ impl Limiter" },
        { kind: "ctx", old: 10, new: 10, text: "    pub fn new(cfg: &Config) -> Self {" },
        { kind: "del", old: 11, text: "        let burst = cfg.burst.min(WINDOW - 1);" },
        { kind: "add", new: 11, text: "        let burst = clamp_burst(cfg.burst, WINDOW);" },
        { kind: "ctx", old: 12, new: 12, text: "        Self { burst, seen: Vec::new() }" },
        { kind: "ctx", old: 13, new: 13, text: "    }" },
        { kind: "hunk", text: "@@ +22,3 @@" },
        { kind: "add", new: 22, text: "fn clamp_burst(burst: u32, window: u32) -> u32 {" },
        { kind: "add", new: 23, text: "    burst.min(window - 1) // off-by-one fixed in mqkosuwt" },
        { kind: "add", new: 24, text: "}" },
      ],
    },
  ],
};

// interdiff: what the r1 → r2 amend of tzunmyk actually changed
export const interdiffs: Record<string, FileDiff[]> = {
  tzunmyk: [
    {
      path: "src/webhooks.rs",
      status: "modified",
      lines: [
        { kind: "hunk", text: "@@ b02f77 → c7e1b9 @@ deliver_invoice_webhook" },
        { kind: "ctx", old: 41, new: 41, text: "    let resp = post(ep.url.clone())" },
        { kind: "add", new: 42, text: "        .header(\"Retry-After\", backoff_secs(inv.attempts).to_string())" },
        { kind: "ctx", old: 42, new: 43, text: "        .header(\"X-Acme-Signature\", sig)" },
        { kind: "ctx", old: 43, new: 44, text: "        .body(payload)" },
      ],
    },
  ],
};

// ── environments (hub-visible keyring metadata ONLY) ─────────────────────────

export type Env = {
  name: string;
  members: number;
  generation: number;
  bundleUpdated: string; // when the ciphertext bundle last changed
  bundleUpdatedBy: string; // pusher identity (hub-side fact, not decrypted)
  note?: string;
};

export const envs: Env[] = [
  { name: "dev", members: 12, generation: 3, bundleUpdated: "2d", bundleUpdatedBy: "mia" },
  {
    name: "staging",
    members: 5,
    generation: 3,
    bundleUpdated: "2h",
    bundleUpdatedBy: "ishan",
    note: "rotated 2h ago — @devon removed",
  },
  { name: "prod", members: 6, generation: 5, bundleUpdated: "3w", bundleUpdatedBy: "mia" },
];

export type Member = { handle: string; fingerprint: string; envs: string[] };

export const members: Member[] = [
  { handle: "mia", fingerprint: "age1qm7…7xk4", envs: ["dev", "staging", "prod"] },
  { handle: "ishan", fingerprint: "age1z0d…p9r2", envs: ["dev", "staging"] },
  { handle: "devon", fingerprint: "age1k4n…m3w8", envs: ["dev"] },
  { handle: "sana", fingerprint: "age1vv2…q6h1", envs: ["dev", "staging", "prod"] },
  { handle: "leo", fingerprint: "age1x8p…t2c5", envs: ["dev", "prod"] },
];

export type KeyringEvent = {
  when: string;
  what: string;
  signedBy: string; // member signature (or "pushed by" for bundle replacements)
  kind: "rotate" | "grant" | "remove" | "bundle" | "root";
};

export const keyringLog: KeyringEvent[] = [
  { when: "2h", what: "staging → gen 3 · @devon removed · rotation forced", signedBy: "ishan", kind: "remove" },
  { when: "3h", what: "@mia granted staging (history included)", signedBy: "ishan", kind: "grant" },
  { when: "2d", what: "dev bundle replaced (gen 3, ciphertext)", signedBy: "mia", kind: "bundle" },
  { when: "3w", what: "prod → gen 5 · scheduled rotation", signedBy: "mia", kind: "rotate" },
  { when: "6mo", what: "keyring created · trust root", signedBy: "mia (self-signed)", kind: "root" },
];

export const honestBanner =
  "This hub stores ciphertext. Secret names, values — even how many secrets an environment holds — are " +
  "encrypted inside the bundle and unreadable here. Everything below is keyring metadata: who is a member, " +
  "which generation each environment is on, and who signed each update. Reading or editing values happens " +
  "in the CLI: graft env.";

export const repoMeta = {
  owner: "acme",
  name: "api",
  branches: ["main", "payments", "release-2.4"],
  defaultBranch: "main",
  artifactCount: 18383,
  size: "34.6 MiB",
};
