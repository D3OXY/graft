// Variant A — "Familiar": GitHub-shaped, polished. Top nav, repo tabs, PR-style
// review — every control where a GitHub user's hands expect it. Graft's model
// shows up as vocabulary: violet letter chips for changes, mono hex for
// revisions, interdiffs, land-not-merge, keyring-not-settings.
import { useMemo, useState, type ReactNode } from "react";
import type { Screen, VariantProps } from "../App";
import {
  changeById,
  changeDiffs,
  changes,
  envs,
  fileContents,
  honestBanner,
  interdiffs,
  keyringLog,
  members,
  proposalById,
  proposals,
  repoMeta,
  tree,
  type FileDiff,
  type Thread,
  type TreeNode,
} from "../data";

// ── icons (lucide-style, stroke) ─────────────────────────────────────────────

const iconPaths = {
  branch: (
    <>
      <line x1="6" y1="3" x2="6" y2="15" />
      <circle cx="18" cy="6" r="3" />
      <circle cx="6" cy="18" r="3" />
      <path d="M18 9a9 9 0 0 1-9 9" />
    </>
  ),
  proposal: (
    <>
      <circle cx="18" cy="18" r="3" />
      <circle cx="6" cy="6" r="3" />
      <path d="M13 6h3a2 2 0 0 1 2 2v7" />
      <line x1="6" y1="9" x2="6" y2="21" />
    </>
  ),
  landed: (
    <>
      <circle cx="18" cy="18" r="3" />
      <circle cx="6" cy="6" r="3" />
      <path d="M6 21V9a9 9 0 0 0 9 9" />
    </>
  ),
  code: (
    <>
      <polyline points="16 18 22 12 16 6" />
      <polyline points="8 6 2 12 8 18" />
    </>
  ),
  check: <polyline points="20 6 9 17 4 12" />,
  chevronRight: <polyline points="9 18 15 12 9 6" />,
  chevronDown: <polyline points="6 9 12 15 18 9" />,
  folder: <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />,
  file: (
    <>
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
      <polyline points="14 2 14 8 20 8" />
    </>
  ),
  history: (
    <>
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </>
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="8" />
      <line x1="21" y1="21" x2="16.65" y2="16.65" />
    </>
  ),
  lock: (
    <>
      <rect x="3" y="11" width="18" height="11" rx="2" />
      <path d="M7 11V7a5 5 0 0 1 10 0v4" />
    </>
  ),
  comment: <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />,
  users: (
    <>
      <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
      <path d="M16 3.13a4 4 0 0 1 0 7.75" />
    </>
  ),
  rotate: (
    <>
      <polyline points="23 4 23 10 17 10" />
      <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
    </>
  ),
  warn: (
    <>
      <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
      <line x1="12" y1="9" x2="12" y2="13" />
      <line x1="12" y1="17" x2="12.01" y2="17" />
    </>
  ),
  back: (
    <>
      <line x1="19" y1="12" x2="5" y2="12" />
      <polyline points="12 19 5 12 12 5" />
    </>
  ),
  copy: (
    <>
      <rect x="9" y="9" width="13" height="13" rx="2" />
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
    </>
  ),
  stack: (
    <>
      <polygon points="12 2 2 7 12 12 22 7 12 2" />
      <polyline points="2 17 12 22 22 17" />
      <polyline points="2 12 12 17 22 12" />
    </>
  ),
  sign: (
    <>
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
    </>
  ),
  key: <path d="m21 2-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0 3 3L22 7l-3-3m-3.5 3.5L19 4" />,
} as const;

type IconName = keyof typeof iconPaths;

function Icon({ name, className = "size-4" }: { name: IconName; className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 ${className}`}
      aria-hidden
    >
      {iconPaths[name]}
    </svg>
  );
}

// ── shared atoms ─────────────────────────────────────────────────────────────

const avatarHue: Record<string, string> = {
  ishan: "bg-graft-600",
  mia: "bg-indigo-500",
  devon: "bg-amber-500",
  sana: "bg-rose-500",
  leo: "bg-cyan-600",
};

function Avatar({ who, size = "size-6" }: { who: string; size?: string }) {
  return (
    <span
      title={`@${who}`}
      className={`inline-flex ${size} shrink-0 items-center justify-center rounded-full text-[10px] font-semibold text-white ring-2 ring-white ${avatarHue[who] ?? "bg-ink-400"}`}
    >
      {who[0].toUpperCase()}
    </span>
  );
}

function AvatarStack({ who, extra = 0 }: { who: string[]; extra?: number }) {
  return (
    <span className="flex -space-x-1.5">
      {who.map((w) => (
        <Avatar key={w} who={w} size="size-5" />
      ))}
      {extra > 0 && (
        <span className="inline-flex size-5 items-center justify-center rounded-full bg-ink-100 text-[9px] font-semibold text-ink-500 ring-2 ring-white">
          +{extra}
        </span>
      )}
    </span>
  );
}

function Cid({ id }: { id: string }) {
  return (
    <code className="rounded-md bg-violet-500/10 px-1.5 py-px font-mono text-[11px] font-semibold tracking-tight text-violet-700">
      {id}
    </code>
  );
}

function Rev({ hash }: { hash: string }) {
  return <code className="rounded-md bg-ink-100 px-1.5 py-px font-mono text-[11px] text-ink-500">{hash}</code>;
}

function BranchPill({ name }: { name: string }) {
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-graft-100 px-2 py-px text-[11px] font-semibold text-graft-800">
      <Icon name="branch" className="size-3" />
      {name}
    </span>
  );
}

// ── shell ────────────────────────────────────────────────────────────────────

const tabs: { key: Screen; label: string; icon: IconName; count?: number }[] = [
  { key: "repo", label: "Code", icon: "code" },
  { key: "proposals", label: "Proposals", icon: "proposal", count: proposals.filter((p) => p.status === "open").length },
  { key: "envs", label: "Environments", icon: "lock", count: envs.length },
];

export function Familiar({ screen, setScreen }: VariantProps) {
  return (
    <div className="min-h-screen bg-ink-50 text-ink-800 antialiased">
      <div className="h-0.5 bg-gradient-to-r from-graft-600 via-graft-400 to-graft-600" />
      <header className="border-b border-ink-200 bg-white">
        <div className="mx-auto flex max-w-6xl items-center gap-4 px-6 pt-3.5">
          <a className="flex items-baseline gap-2" href="#">
            <span className="font-display text-lg font-bold tracking-tight text-graft-700">graft</span>
            <span className="text-[11px] font-medium text-ink-400">hub.acme.dev</span>
          </a>
          <nav className="flex items-center gap-1 text-sm">
            <span className="text-ink-400">acme</span>
            <span className="text-ink-300">/</span>
            <span className="font-semibold text-ink-900">api</span>
            <span className="ml-1.5 rounded-full border border-ink-200 px-2 py-px text-[10px] font-medium uppercase tracking-wide text-ink-400">
              private
            </span>
          </nav>
          <div className="ml-auto flex items-center gap-3">
            <label className="flex w-56 items-center gap-2 rounded-lg border border-ink-200 bg-ink-50 px-2.5 py-1.5 text-xs text-ink-400 transition-colors focus-within:border-graft-400 focus-within:bg-white">
              <Icon name="search" className="size-3.5" />
              <input placeholder="Search changes, files…" className="w-full bg-transparent outline-none placeholder:text-ink-400" />
              <kbd className="rounded border border-ink-200 bg-white px-1 font-mono text-[10px]">⌘K</kbd>
            </label>
            <button className="flex cursor-pointer items-center gap-1.5 rounded-lg bg-graft-700 px-3 py-1.5 text-xs font-semibold text-white shadow-sm transition-colors hover:bg-graft-800">
              <Icon name="copy" className="size-3.5" />
              Clone
            </button>
            <Avatar who="ishan" size="size-7" />
          </div>
        </div>
        <nav className="mx-auto flex max-w-6xl gap-1 px-6 pt-2">
          {tabs.map((t) => (
            <button
              key={t.key}
              onClick={() => setScreen(t.key)}
              className={`flex cursor-pointer items-center gap-1.5 border-b-2 px-3 pb-2.5 pt-1.5 text-sm transition-colors ${
                screen === t.key
                  ? "border-graft-600 font-semibold text-ink-900"
                  : "border-transparent text-ink-500 hover:border-ink-300 hover:text-ink-800"
              }`}
            >
              <Icon name={t.icon} className="size-4" />
              {t.label}
              {t.count != null && (
                <span className="rounded-full bg-ink-100 px-1.5 py-px text-[11px] font-medium text-ink-500">{t.count}</span>
              )}
            </button>
          ))}
        </nav>
      </header>
      <main key={screen} className="mx-auto max-w-6xl animate-[rise_0.25s_ease_both] px-6 py-6 pb-28">
        {screen === "repo" && <Code />}
        {screen === "proposals" && <Proposals />}
        {screen === "envs" && <Environments />}
      </main>
    </div>
  );
}

// ── code: tree + file + log ──────────────────────────────────────────────────

const flatTree = (nodes: TreeNode[]): TreeNode[] => nodes.flatMap((n) => [n, ...flatTree(n.children ?? [])]);

function Code() {
  const [path, setPath] = useState("src/limiter.rs");
  const [showLog, setShowLog] = useState(false);
  const tip = changes.find((c) => c.branches?.includes("main"))!;
  return (
    <div>
      <div className="mb-4 flex items-center gap-2.5">
        <button className="flex cursor-pointer items-center gap-1.5 rounded-lg border border-ink-200 bg-white px-3 py-1.5 text-sm font-semibold shadow-xs transition-colors hover:border-ink-300">
          <Icon name="branch" className="size-3.5 text-ink-400" />
          main
          <Icon name="chevronDown" className="size-3.5 text-ink-400" />
        </button>
        <span className="text-xs text-ink-400">{repoMeta.branches.length} branches</span>
        <button
          onClick={() => setShowLog(!showLog)}
          className={`ml-auto flex cursor-pointer items-center gap-1.5 rounded-lg border px-3 py-1.5 text-sm shadow-xs transition-colors ${
            showLog
              ? "border-graft-300 bg-graft-50 font-semibold text-graft-800"
              : "border-ink-200 bg-white text-ink-600 hover:border-ink-300"
          }`}
        >
          <Icon name="history" className="size-3.5" />
          {showLog ? "Back to files" : `${changes.length} changes`}
        </button>
      </div>

      <div className="mb-4 flex items-center gap-2.5 rounded-xl border border-ink-200 bg-white px-4 py-2.5 text-sm shadow-xs">
        <Avatar who={tip.author} />
        <span className="font-medium text-ink-900">{tip.message}</span>
        <span className="ml-auto flex items-center gap-2 text-xs text-ink-400">
          <Cid id={tip.id} />
          <Rev hash={tip.revisions.at(-1)!.hash} />
          {tip.when} ago
        </span>
      </div>

      {showLog ? (
        <LogList />
      ) : (
        <div className="flex items-start gap-4">
          <div className="w-60 shrink-0 rounded-xl border border-ink-200 bg-white py-1.5 shadow-xs">
            <Tree nodes={tree} depth={0} selected={path} onPick={setPath} />
          </div>
          <FileView path={path} />
        </div>
      )}
    </div>
  );
}

function Tree({ nodes, depth, selected, onPick }: { nodes: TreeNode[]; depth: number; selected: string; onPick: (p: string) => void }) {
  const [closed, setClosed] = useState<Set<string>>(new Set());
  return (
    <ul className="text-[13px]">
      {nodes.map((n) => {
        const isClosed = closed.has(n.path);
        return (
          <li key={n.path}>
            <button
              onClick={() =>
                n.kind === "dir"
                  ? setClosed((s) => {
                      const next = new Set(s);
                      if (next.has(n.path)) next.delete(n.path);
                      else next.add(n.path);
                      return next;
                    })
                  : onPick(n.path)
              }
              style={{ paddingLeft: 10 + depth * 14 }}
              className={`group flex w-full cursor-pointer items-center gap-1.5 py-1 pr-2 text-left transition-colors ${
                selected === n.path ? "bg-graft-50 font-medium text-graft-900" : "text-ink-700 hover:bg-ink-50"
              }`}
            >
              {n.kind === "dir" ? (
                <>
                  <Icon name={isClosed ? "chevronRight" : "chevronDown"} className="size-3 text-ink-400" />
                  <Icon name="folder" className="size-3.5 text-graft-500" />
                </>
              ) : (
                <>
                  <span className="w-3" />
                  <Icon name="file" className={`size-3.5 ${selected === n.path ? "text-graft-600" : "text-ink-300"}`} />
                </>
              )}
              <span className="truncate">{n.name}</span>
              {n.lastChange && (
                <code className="ml-auto font-mono text-[10px] text-ink-300 opacity-0 transition-opacity group-hover:opacity-100">
                  {n.lastChange}
                </code>
              )}
            </button>
            {n.children && !isClosed && <Tree nodes={n.children} depth={depth + 1} selected={selected} onPick={onPick} />}
          </li>
        );
      })}
    </ul>
  );
}

// cheap single-line Rust highlighter — fidelity for the eyes, not a parser
const rustToken =
  /("(?:[^"\\]|\\.)*"|\b(?:pub|fn|let|impl|struct|use|const|mut|return|match|if|else|for|in|while|true|false|as|self|Self)\b|\b[A-Z][A-Za-z0-9_]*\b|\b\d+\b)/g;

function highlightRust(line: string): ReactNode {
  const ci = line.indexOf("//");
  const code = ci >= 0 ? line.slice(0, ci) : line;
  const comment = ci >= 0 ? line.slice(ci) : "";
  const parts = code.split(rustToken).map((part, i) => {
    if (i % 2 === 0) return part;
    if (part.startsWith('"')) return <span key={i} className="text-graft-700">{part}</span>;
    if (/^\d+$/.test(part)) return <span key={i} className="text-amber-700">{part}</span>;
    if (/^[A-Z]/.test(part)) return <span key={i} className="text-sky-700">{part}</span>;
    return <span key={i} className="font-medium text-violet-700">{part}</span>;
  });
  return (
    <>
      {parts}
      {comment && <span className="text-ink-400 italic">{comment}</span>}
    </>
  );
}

function FileView({ path }: { path: string }) {
  const lines = fileContents[path];
  const isRust = path.endsWith(".rs");
  const last = flatTree(tree).find((n) => n.path === path)?.lastChange;
  const crumbs = path.split("/");
  return (
    <div className="min-w-0 flex-1 overflow-hidden rounded-xl border border-ink-200 bg-white shadow-xs">
      <div className="flex items-center gap-1 border-b border-ink-100 bg-ink-50/60 px-4 py-2.5 font-mono text-xs">
        {crumbs.map((c, i) => (
          <span key={i} className="flex items-center gap-1">
            {i > 0 && <span className="text-ink-300">/</span>}
            <span className={i === crumbs.length - 1 ? "font-semibold text-ink-900" : "text-ink-400"}>{c}</span>
          </span>
        ))}
        {last && (
          <span className="ml-auto flex items-center gap-1.5 font-sans text-ink-400">
            last touched by <Cid id={last} />
            <span className="hidden truncate lg:inline">— {changeById(last).message}</span>
          </span>
        )}
      </div>
      {lines ? (
        <pre className="overflow-x-auto py-3 font-mono text-xs leading-[1.7]">
          {lines.map((l, i) => (
            <div key={i} className="flex transition-colors hover:bg-graft-50/50">
              <span className="w-10 shrink-0 select-none pr-3 text-right text-ink-300">{i + 1}</span>
              <span className="pr-4 text-ink-800">{isRust ? highlightRust(l) : l || " "}</span>
            </div>
          ))}
        </pre>
      ) : (
        <p className="p-8 text-center text-sm text-ink-400">fake file — no content wired for {path}</p>
      )}
    </div>
  );
}

function LogList() {
  return (
    <ol className="divide-y divide-ink-100 overflow-hidden rounded-xl border border-ink-200 bg-white shadow-xs">
      {changes.map((c) => (
        <li key={c.id} className="flex items-center gap-3 px-4 py-3 text-sm transition-colors hover:bg-ink-50/60">
          <Avatar who={c.author} />
          <div className="min-w-0">
            <p className="truncate font-medium text-ink-900">
              {c.message}
              {c.branches?.map((b) => (
                <span key={b} className="ml-2 align-[1px]">
                  <BranchPill name={b} />
                </span>
              ))}
              {c.inProposal && (
                <span className="ml-2 inline-flex items-center gap-1 rounded-full bg-violet-500/10 px-2 py-px align-[1px] text-[11px] font-medium text-violet-700">
                  <Icon name="proposal" className="size-3" />#{c.inProposal}
                </span>
              )}
            </p>
            <p className="mt-0.5 text-xs text-ink-400">
              @{c.author} · {c.when} ago
              {c.revisions.length > 1 && <span className="text-violet-600"> · {c.revisions.length} revisions</span>}
            </p>
          </div>
          <span className="ml-auto flex shrink-0 items-center gap-2">
            <Cid id={c.id} />
            <Rev hash={c.revisions.at(-1)!.hash} />
          </span>
        </li>
      ))}
    </ol>
  );
}

// ── proposals ────────────────────────────────────────────────────────────────

function Proposals() {
  const [openId, setOpenId] = useState<number | null>(218);
  const [filter, setFilter] = useState<"open" | "all">("open");
  if (openId != null) return <ProposalDetail id={openId} onBack={() => setOpenId(null)} />;
  const list = proposals.filter((p) => filter === "all" || p.status === "open");
  return (
    <div className="overflow-hidden rounded-xl border border-ink-200 bg-white shadow-xs">
      <div className="flex gap-4 border-b border-ink-100 bg-ink-50/60 px-4 py-2.5 text-sm">
        {(["open", "all"] as const).map((f) => (
          <button
            key={f}
            onClick={() => setFilter(f)}
            className={`cursor-pointer capitalize transition-colors ${filter === f ? "font-semibold text-ink-900" : "text-ink-400 hover:text-ink-700"}`}
          >
            {f === "open" ? `${proposals.filter((p) => p.status === "open").length} open` : "all"}
          </button>
        ))}
      </div>
      <ol className="divide-y divide-ink-100">
        {list.map((p) => (
          <li key={p.id} className="flex items-center gap-3 px-4 py-3.5 transition-colors hover:bg-ink-50/60">
            <Icon
              name={p.status === "landed" ? "landed" : "proposal"}
              className={`size-4.5 ${p.status === "open" ? "text-graft-600" : p.status === "landed" ? "text-violet-600" : "text-ink-400"}`}
            />
            <div className="min-w-0">
              <p className="flex items-center gap-2">
                <button onClick={() => setOpenId(p.id)} className="cursor-pointer truncate text-sm font-semibold text-ink-900 hover:text-graft-700">
                  {p.title}
                </button>
                {p.trialRebase === "would-conflict" && (
                  <span className="inline-flex shrink-0 items-center gap-1 rounded-full bg-amber-500/10 px-2 py-px text-[11px] font-medium text-amber-700">
                    <Icon name="warn" className="size-3" />
                    would conflict
                  </span>
                )}
              </p>
              <p className="mt-0.5 text-xs text-ink-400">
                #{p.id} · @{p.author} · {p.stack.length} change{p.stack.length > 1 ? "s" : ""} onto{" "}
                <span className="font-mono">{p.target}</span> · {p.when} ago
              </p>
            </div>
            <span className="ml-auto flex shrink-0 items-center gap-3 text-xs text-ink-400">
              {p.threads.length > 0 && (
                <span className="flex items-center gap-1">
                  <Icon name="comment" className="size-3.5" />
                  {p.threads.length}
                </span>
              )}
              {p.approvals.length > 0 && (
                <span className="flex items-center gap-1 font-medium text-graft-700">
                  <Icon name="check" className="size-3.5" />
                  {p.approvals.length}
                </span>
              )}
              <Avatar who={p.author} size="size-5" />
            </span>
          </li>
        ))}
      </ol>
    </div>
  );
}

function ProposalDetail({ id, onBack }: { id: number; onBack: () => void }) {
  const p = proposalById(id);
  const [tab, setTab] = useState<"conversation" | "changes">("conversation");
  const [sel, setSel] = useState(p.stack[0]);
  const [view, setView] = useState<"diff" | "interdiff">("diff");

  const jumpToInterdiff = (changeId: string) => {
    setSel(changeId);
    setView("interdiff");
    setTab("changes");
  };

  return (
    <div>
      <button onClick={onBack} className="mb-4 flex cursor-pointer items-center gap-1.5 text-xs font-medium text-ink-400 transition-colors hover:text-ink-800">
        <Icon name="back" className="size-3.5" />
        all proposals
      </button>
      <div className="mb-1.5 flex items-baseline gap-2.5">
        <h1 className="font-display text-[22px] font-semibold tracking-tight text-ink-900">{p.title}</h1>
        <span className="text-lg text-ink-300">#{p.id}</span>
      </div>
      <div className="mb-5 flex items-center gap-2.5 text-sm text-ink-500">
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-semibold text-white ${
            { open: "bg-graft-600", landed: "bg-violet-600", closed: "bg-ink-400" }[p.status]
          }`}
        >
          <Icon name={p.status === "landed" ? "landed" : "proposal"} className="size-3" />
          {p.status}
        </span>
        <span>
          <Avatar who={p.author} size="size-5" /> <b className="text-ink-700">@{p.author}</b> proposes{" "}
          <b className="text-ink-700">{p.stack.length} changes</b> onto <BranchPill name={p.target} />
        </span>
      </div>

      <StackStrip p={p} sel={sel} onPick={(cid) => { setSel(cid); setView("diff"); setTab("changes"); }} />

      <div className="mb-4 mt-5 flex gap-0.5 border-b border-ink-200 text-sm">
        {(["conversation", "changes"] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`flex cursor-pointer items-center gap-1.5 border-b-2 px-3.5 pb-2 capitalize transition-colors ${
              tab === t ? "border-graft-600 font-semibold text-ink-900" : "border-transparent text-ink-400 hover:text-ink-700"
            }`}
          >
            <Icon name={t === "changes" ? "stack" : "comment"} className="size-3.5" />
            {t}
            {t === "changes" && <span className="rounded-full bg-ink-100 px-1.5 text-[11px] text-ink-500">{p.stack.length}</span>}
          </button>
        ))}
      </div>

      <div className="flex items-start gap-6">
        <div className="min-w-0 flex-1">
          {tab === "conversation" ? (
            <Conversation id={id} onInterdiff={jumpToInterdiff} />
          ) : (
            <ChangesTab id={id} sel={sel} setSel={(c) => { setSel(c); setView("diff"); }} view={view} setView={setView} />
          )}
        </div>
        <LandPanel id={id} />
      </div>
    </div>
  );
}

function StackStrip({ p, sel, onPick }: { p: ReturnType<typeof proposalById>; sel: string; onPick: (c: string) => void }) {
  return (
    <div className="flex items-stretch gap-0">
      <div className="flex flex-col items-center justify-center rounded-l-xl border border-r-0 border-ink-200 bg-ink-50/80 px-3 text-center">
        <span className="text-[10px] font-medium uppercase tracking-wider text-ink-400">stack</span>
        <BranchPill name={p.target} />
      </div>
      {p.stack.map((cid, i) => {
        const c = changeById(cid);
        const open = p.threads.filter((t) => t.changeId === cid && !t.resolved).length;
        return (
          <button
            key={cid}
            onClick={() => onPick(cid)}
            className={`group flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 border border-r-0 px-3.5 py-2.5 text-left transition-colors last:rounded-r-xl last:border-r ${
              sel === cid ? "border-graft-300 bg-graft-50/70" : "border-ink-200 bg-white hover:bg-ink-50/60"
            }`}
          >
            <span
              className={`flex size-5 shrink-0 items-center justify-center rounded-full text-[10px] font-bold ${
                sel === cid ? "bg-graft-600 text-white" : "bg-ink-100 text-ink-500"
              }`}
            >
              {i + 1}
            </span>
            <span className="min-w-0">
              <span className="block truncate text-[13px] font-medium text-ink-900">{c.message}</span>
              <span className="mt-px flex items-center gap-1.5 text-[11px] text-ink-400">
                <Cid id={cid} />
                {c.revisions.length > 1 && <span className="text-violet-600">{c.revisions.length} revisions</span>}
                {open > 0 && <span className="text-amber-700">{open} open thread{open > 1 ? "s" : ""}</span>}
              </span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

function Conversation({ id, onInterdiff }: { id: number; onInterdiff: (changeId: string) => void }) {
  const p = proposalById(id);
  return (
    <ol className="relative space-y-4 before:absolute before:bottom-4 before:left-[13px] before:top-2 before:w-px before:bg-ink-200">
      {p.events.map((e, i) => (
        <li key={i} className="relative flex gap-3 text-sm">
          <span className="z-10 mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full border border-ink-200 bg-white text-ink-400">
            <Icon
              name={e.kind === "comment" ? "comment" : e.kind === "approve" ? "check" : e.kind === "amend" ? "rotate" : "proposal"}
              className={`size-3.5 ${e.kind === "approve" ? "text-graft-600" : e.kind === "amend" ? "text-violet-600" : ""}`}
            />
          </span>
          {e.kind === "comment" ? (
            <div className="min-w-0 flex-1 overflow-hidden rounded-xl border border-ink-200 bg-white shadow-xs">
              <p className="flex items-center gap-2 border-b border-ink-100 bg-ink-50/60 px-3.5 py-2 text-xs">
                <Avatar who={e.who} size="size-5" />
                <b className="text-ink-800">@{e.who}</b>
                <span className="text-ink-400">{e.when} ago</span>
              </p>
              <p className="px-3.5 py-3 text-[13px] leading-relaxed text-ink-800">{e.body}</p>
            </div>
          ) : (
            <p className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1 pt-1 text-[13px] text-ink-500">
              <b className="text-ink-800">@{e.who}</b>
              {e.kind === "opened" && <>opened this proposal · {e.when} ago</>}
              {e.kind === "approve" && (
                <>
                  <b className="text-graft-700">approved</b> at <Rev hash={e.revision} /> · {e.when} ago
                </>
              )}
              {e.kind === "amend" && (
                <>
                  amended <Cid id={e.changeId} /> <Rev hash={e.from} /> → <Rev hash={e.to} /> · {e.note} ·
                  <button
                    onClick={() => onInterdiff(e.changeId)}
                    className="cursor-pointer font-semibold text-violet-700 transition-colors hover:text-violet-900"
                  >
                    view interdiff →
                  </button>
                </>
              )}
            </p>
          )}
        </li>
      ))}
      <li className="relative flex gap-3">
        <span className="z-10 mt-1"><Avatar who="ishan" size="size-7" /></span>
        <div className="flex-1 overflow-hidden rounded-xl border border-ink-200 bg-white shadow-xs transition-colors focus-within:border-graft-400">
          <textarea placeholder="Leave a comment…" rows={3} className="w-full resize-none px-3.5 py-3 text-sm outline-none placeholder:text-ink-300" />
          <div className="flex justify-end border-t border-ink-100 bg-ink-50/60 px-3 py-2">
            <button className="cursor-pointer rounded-lg bg-ink-800 px-3.5 py-1.5 text-xs font-semibold text-white transition-colors hover:bg-ink-900">
              Comment
            </button>
          </div>
        </div>
      </li>
    </ol>
  );
}

function ChangesTab({
  id,
  sel,
  setSel,
  view,
  setView,
}: {
  id: number;
  sel: string;
  setSel: (c: string) => void;
  view: "diff" | "interdiff";
  setView: (v: "diff" | "interdiff") => void;
}) {
  const p = proposalById(id);
  const c = changeById(sel);
  const diffs = (view === "interdiff" ? interdiffs[sel] : changeDiffs[sel]) ?? [];
  return (
    <div>
      <div className="mb-3 flex flex-wrap items-center gap-2 text-xs">
        <span className="inline-flex overflow-hidden rounded-lg border border-ink-200 bg-white shadow-xs">
          {p.stack.map((cid, i) => (
            <button
              key={cid}
              onClick={() => setSel(cid)}
              className={`cursor-pointer px-3 py-1.5 font-mono transition-colors ${
                sel === cid ? "bg-ink-800 font-semibold text-white" : "text-ink-500 hover:bg-ink-50"
              }`}
            >
              {i + 1} · {cid}
            </button>
          ))}
        </span>
        <span className="mx-1 h-4 w-px bg-ink-200" />
        {c.revisions.map((r) => (
          <span key={r.hash} className="font-mono text-ink-400">
            {r.label} <Rev hash={r.hash} />
          </span>
        ))}
        {c.revisions.length > 1 && (
          <button
            onClick={() => setView(view === "diff" ? "interdiff" : "diff")}
            className={`cursor-pointer rounded-full px-3 py-1 font-medium transition-colors ${
              view === "interdiff"
                ? "bg-violet-600 text-white"
                : "bg-violet-500/10 text-violet-700 hover:bg-violet-500/20"
            }`}
          >
            {view === "interdiff" ? "showing interdiff r1 → r2 · back to diff" : "what changed since r1?"}
          </button>
        )}
      </div>
      {diffs.map((d) => (
        <DiffBlock key={d.path} diff={d} threads={p.threads.filter((t) => t.changeId === sel && t.path === d.path)} />
      ))}
      {diffs.length === 0 && (
        <p className="rounded-xl border border-ink-200 bg-white p-8 text-center text-sm text-ink-400">no diff wired for this change</p>
      )}
    </div>
  );
}

function DiffBlock({ diff, threads }: { diff: FileDiff; threads: Thread[] }) {
  const counts = useMemo(
    () => ({
      add: diff.lines.filter((l) => l.kind === "add").length,
      del: diff.lines.filter((l) => l.kind === "del").length,
    }),
    [diff],
  );
  const rowBg = { add: "bg-graft-50", del: "bg-rose-50", ctx: "", hunk: "bg-sky-50/70" };
  const gutter = { add: "text-graft-700", del: "text-rose-600", ctx: "text-ink-300", hunk: "" };
  return (
    <div className="mb-4 overflow-hidden rounded-xl border border-ink-200 bg-white shadow-xs">
      <div className="flex items-center gap-2.5 border-b border-ink-100 bg-ink-50/60 px-4 py-2.5 text-xs">
        <Icon name="file" className="size-3.5 text-ink-400" />
        <span className="font-mono font-semibold text-ink-900">{diff.path}</span>
        <span
          className={`rounded-full px-2 py-px font-medium ${
            diff.status === "added" ? "bg-graft-100 text-graft-800" : "bg-amber-500/10 text-amber-700"
          }`}
        >
          {diff.status}
        </span>
        <span className="ml-auto font-mono">
          <span className="font-semibold text-graft-700">+{counts.add}</span>{" "}
          <span className="font-semibold text-rose-600">−{counts.del}</span>
        </span>
      </div>
      <div className="font-mono text-xs leading-[1.7]">
        {diff.lines.map((l, i) => {
          const lineNo = l.new ?? l.old;
          const thread = threads.find((t) => t.line === lineNo && (l.kind === "add" || l.kind === "ctx"));
          return (
            <div key={i}>
              {l.kind === "hunk" ? (
                <p className={`px-4 py-1 text-sky-700/80 ${rowBg.hunk}`}>{l.text}</p>
              ) : (
                <div className={`flex ${rowBg[l.kind]}`}>
                  <span className="w-10 shrink-0 select-none pr-2 text-right text-ink-300">{l.old ?? ""}</span>
                  <span className="w-10 shrink-0 select-none pr-2 text-right text-ink-300">{l.new ?? ""}</span>
                  <span className={`w-5 shrink-0 select-none text-center font-semibold ${gutter[l.kind]}`}>
                    {l.kind === "add" ? "+" : l.kind === "del" ? "−" : ""}
                  </span>
                  <span className="whitespace-pre pr-4 text-ink-800">{diff.path.endsWith(".rs") ? highlightRust(l.text) : l.text}</span>
                </div>
              )}
              {thread && <ThreadCard thread={thread} />}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ThreadCard({ thread }: { thread: Thread }) {
  const c = changeById(thread.changeId);
  const outdated = thread.revision !== c.revisions.at(-1)!.hash;
  return (
    <div className="border-y border-ink-100 bg-ink-50/50 px-4 py-3 font-sans">
      <div className="overflow-hidden rounded-xl border border-ink-200 bg-white shadow-sm">
        <p className="flex items-center gap-2 border-b border-ink-100 px-3.5 py-2 text-[11px] text-ink-400">
          thread on <Rev hash={thread.revision} />
          {outdated && (
            <span className="rounded-full bg-amber-500/10 px-2 py-px font-medium text-amber-700">outdated — written on r1</span>
          )}
          <span className={`ml-auto flex items-center gap-1 font-medium ${thread.resolved ? "text-graft-700" : "text-ink-400"}`}>
            {thread.resolved && <Icon name="check" className="size-3" />}
            {thread.resolved ? "resolved" : "open"}
          </span>
        </p>
        {thread.comments.map((cm, i) => (
          <div key={i} className="flex gap-2.5 px-3.5 py-2.5">
            <Avatar who={cm.author} size="size-5" />
            <p className="text-xs leading-relaxed text-ink-800">
              <b>@{cm.author}</b> <span className="text-ink-400">{cm.when} ago</span>
              <span className="mt-0.5 block">{cm.body}</span>
            </p>
          </div>
        ))}
        <div className="flex gap-3 border-t border-ink-100 bg-ink-50/60 px-3.5 py-2 text-[11px] font-medium">
          <button className="cursor-pointer text-ink-500 transition-colors hover:text-ink-900">Reply</button>
          <button className="cursor-pointer text-ink-500 transition-colors hover:text-ink-900">
            {thread.resolved ? "Unresolve" : "Resolve"}
          </button>
        </div>
      </div>
    </div>
  );
}

function LandPanel({ id }: { id: number }) {
  const p = proposalById(id);
  const openThreads = p.threads.filter((t) => !t.resolved).length;
  const canLand = p.trialRebase === "clean" && p.status === "open";
  return (
    <aside className="w-72 shrink-0 space-y-3 text-sm">
      <section className="rounded-xl border border-ink-200 bg-white p-4 shadow-xs">
        <h3 className="mb-2.5 text-xs font-semibold uppercase tracking-wider text-ink-400">Review</h3>
        {p.approvals.map((a) => (
          <p key={a.who} className="mb-1 flex items-center gap-2 text-[13px]">
            <Avatar who={a.who} size="size-5" />
            <span className="font-medium text-graft-800">@{a.who} approved</span>
            <span className="ml-auto"><Rev hash={a.revision} /></span>
          </p>
        ))}
        {p.approvals.length === 0 && <p className="text-[13px] text-ink-400">no approvals yet</p>}
        <p className={`mt-1.5 flex items-center gap-1.5 text-xs ${openThreads ? "text-amber-700" : "text-ink-400"}`}>
          <Icon name="comment" className="size-3.5" />
          {openThreads} open thread{openThreads === 1 ? "" : "s"}
        </p>
        <button className="mt-3 w-full cursor-pointer rounded-lg border border-graft-300 bg-graft-50 py-2 text-[13px] font-semibold text-graft-800 transition-colors hover:bg-graft-100">
          Approve
        </button>
      </section>

      <section className="rounded-xl border border-ink-200 bg-white p-4 shadow-xs">
        <h3 className="mb-2.5 text-xs font-semibold uppercase tracking-wider text-ink-400">Land</h3>
        {p.trialRebase === "clean" ? (
          <p className="flex items-start gap-2 text-[13px] leading-relaxed text-ink-600">
            <Icon name="check" className="mt-0.5 size-4 text-graft-600" />
            <span>
              Rebases cleanly onto <BranchPill name={p.target} />. Landing rebases the stack, then fast-forwards —{" "}
              <b className="text-ink-800">{p.target} stays linear.</b>
            </span>
          </p>
        ) : (
          <p className="flex items-start gap-2 text-[13px] leading-relaxed text-amber-800">
            <Icon name="warn" className="mt-0.5 size-4" />
            <span>
              Trial rebase conflicts in <code className="font-mono text-xs">{p.conflictFiles?.join(", ")}</code>. The hub
              refuses to land a conflicted result — pull, resolve, amend. The proposal updates itself.
            </span>
          </p>
        )}
        <button
          disabled={!canLand}
          className={`mt-3 w-full rounded-lg py-2.5 text-[13px] font-semibold text-white shadow-sm transition-colors ${
            canLand ? "cursor-pointer bg-graft-700 hover:bg-graft-800" : "cursor-not-allowed bg-ink-300"
          }`}
        >
          {p.status === "landed" ? "Landed" : `Land ${p.stack.length} changes`}
        </button>
      </section>

      <p className="px-1 text-xs leading-relaxed text-ink-400">
        Amends track automatically — push a rewrite and this proposal advances. Comments stay anchored to the revision they
        were written on.
      </p>
    </aside>
  );
}

// ── environments ─────────────────────────────────────────────────────────────

function Environments() {
  return (
    <div className="space-y-6">
      <div className="flex gap-3.5 rounded-xl border border-amber-200 bg-gradient-to-r from-amber-50 to-white px-5 py-4 shadow-xs">
        <span className="mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-lg bg-amber-100 text-amber-700">
          <Icon name="lock" className="size-4.5" />
        </span>
        <div>
          <p className="text-sm font-semibold text-ink-900">Ciphertext only</p>
          <p className="mt-0.5 max-w-3xl text-[13px] leading-relaxed text-ink-600">{honestBanner}</p>
        </div>
      </div>

      <div className="grid grid-cols-3 gap-4">
        {envs.map((e) => {
          const roster = members.filter((m) => m.envs.includes(e.name)).map((m) => m.handle);
          return (
            <section key={e.name} className="rounded-xl border border-ink-200 bg-white p-4 shadow-xs transition-shadow hover:shadow-sm">
              <div className="mb-3 flex items-center gap-2">
                <h3 className="font-mono text-[15px] font-semibold text-ink-900">{e.name}</h3>
                <span className="rounded-full bg-ink-100 px-2 py-px font-mono text-[10px] font-medium text-ink-500">
                  gen {e.generation}
                </span>
                <span className="ml-auto"><AvatarStack who={roster.slice(0, 3)} extra={e.members - Math.min(roster.length, 3)} /></span>
              </div>
              <dl className="space-y-1 text-xs text-ink-500">
                <div className="flex justify-between">
                  <dt>members</dt>
                  <dd className="font-medium text-ink-800">{e.members}</dd>
                </div>
                <div className="flex justify-between">
                  <dt>bundle replaced</dt>
                  <dd className="font-medium text-ink-800">
                    {e.bundleUpdated} ago <span className="font-normal text-ink-400">by @{e.bundleUpdatedBy}</span>
                  </dd>
                </div>
                <div className="flex justify-between">
                  <dt>contents</dt>
                  <dd className="select-none font-mono tracking-tighter text-ink-300">████████ opaque</dd>
                </div>
              </dl>
              {e.note && (
                <p className="mt-3 flex items-center gap-1.5 rounded-lg bg-amber-500/10 px-2.5 py-1.5 text-[11px] font-medium text-amber-800">
                  <Icon name="rotate" className="size-3" />
                  {e.note}
                </p>
              )}
            </section>
          );
        })}
      </div>

      <div className="grid grid-cols-2 gap-6">
        <section>
          <h3 className="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-ink-400">
            <Icon name="users" className="size-3.5" />
            Members & keys
          </h3>
          <div className="overflow-hidden rounded-xl border border-ink-200 bg-white shadow-xs">
            <table className="w-full text-[13px]">
              <tbody>
                {members.map((m) => (
                  <tr key={m.handle} className="border-b border-ink-100 transition-colors last:border-0 hover:bg-ink-50/60">
                    <td className="px-3.5 py-2.5">
                      <span className="flex items-center gap-2">
                        <Avatar who={m.handle} size="size-5" />
                        <b className="text-ink-900">@{m.handle}</b>
                      </span>
                    </td>
                    <td className="px-3 py-2.5 font-mono text-[11px] text-ink-400">{m.fingerprint}</td>
                    <td className="px-3.5 py-2.5 text-right">
                      {m.envs.map((e) => (
                        <span key={e} className="ml-1 rounded-full bg-ink-100 px-2 py-px font-mono text-[10px] text-ink-500">
                          {e}
                        </span>
                      ))}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="border-t border-ink-100 bg-ink-50/60 px-3.5 py-2 text-[11px] text-ink-400">
              Verify fingerprints out-of-band — this hub is not a trust root. Granting and revoking happen in the CLI,
              signed by a member key.
            </p>
          </div>
        </section>

        <section>
          <h3 className="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-ink-400">
            <Icon name="key" className="size-3.5" />
            Keyring history
          </h3>
          <ol className="relative space-y-2.5 before:absolute before:bottom-3 before:left-[13px] before:top-3 before:w-px before:bg-ink-200">
            {keyringLog.map((e, i) => (
              <li key={i} className="relative flex items-start gap-3">
                <span
                  className={`z-10 mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full border bg-white ${
                    {
                      rotate: "border-sky-200 text-sky-600",
                      grant: "border-graft-200 text-graft-600",
                      remove: "border-amber-200 text-amber-600",
                      bundle: "border-ink-200 text-ink-400",
                      root: "border-violet-200 text-violet-600",
                    }[e.kind]
                  }`}
                >
                  <Icon name={e.kind === "grant" ? "check" : e.kind === "bundle" ? "lock" : e.kind === "root" ? "key" : "rotate"} className="size-3.5" />
                </span>
                <div className="min-w-0 flex-1 rounded-xl border border-ink-200 bg-white px-3.5 py-2 shadow-xs">
                  <p className="text-[13px] text-ink-800">{e.what}</p>
                  <p className="mt-0.5 flex items-center gap-1 text-[11px] text-ink-400">
                    {e.when} ago ·
                    <Icon name="sign" className="size-3" />
                    signed by @{e.signedBy}
                  </p>
                </div>
              </li>
            ))}
          </ol>
        </section>
      </div>
    </div>
  );
}
