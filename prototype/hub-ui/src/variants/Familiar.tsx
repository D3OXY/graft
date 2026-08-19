// Variant A — "Familiar": GitHub-shaped. Top nav, repo tabs, everything where a
// GitHub user's hands expect it. Graft's model shows up as vocabulary inside a
// conventional frame: change-ID chips, interdiff links, keyring metadata.
import { useState } from "react";
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
  type TreeNode,
} from "../data";

const tabs: { key: Screen; label: string; count?: number }[] = [
  { key: "repo", label: "Code" },
  { key: "proposals", label: "Proposals", count: proposals.filter((p) => p.status === "open").length },
  { key: "envs", label: "Environments", count: envs.length },
];

export function Familiar({ screen, setScreen }: VariantProps) {
  return (
    <div className="min-h-screen bg-white text-slate-900">
      <header className="border-b border-slate-200 bg-slate-50 px-6 pt-4">
        <div className="flex items-center gap-3 text-sm">
          <span className="text-lg">⌥</span>
          <span className="font-semibold">
            {repoMeta.owner} / <span className="text-emerald-700">{repoMeta.name}</span>
          </span>
          <span className="rounded-full border border-slate-300 px-2 text-xs text-slate-500">private</span>
          <div className="ml-auto flex items-center gap-3 text-slate-500">
            <span className="rounded border border-slate-300 bg-white px-2 py-1 text-xs">graft clone https://hub.acme.dev/acme/api</span>
            <Avatar who="ishan" />
          </div>
        </div>
        <nav className="mt-3 flex gap-1">
          {tabs.map((t) => (
            <button
              key={t.key}
              onClick={() => setScreen(t.key)}
              className={`cursor-pointer rounded-t-md px-4 py-2 text-sm ${
                screen === t.key
                  ? "border border-b-0 border-slate-200 bg-white font-semibold"
                  : "text-slate-600 hover:bg-slate-100"
              }`}
            >
              {t.label}
              {t.count != null && (
                <span className="ml-2 rounded-full bg-slate-200 px-2 text-xs">{t.count}</span>
              )}
            </button>
          ))}
        </nav>
      </header>
      <main className="mx-auto max-w-6xl px-6 py-6 pb-24">
        {screen === "repo" && <Code />}
        {screen === "proposals" && <Proposals />}
        {screen === "envs" && <Environments />}
      </main>
    </div>
  );
}

function Avatar({ who }: { who: string }) {
  const hue: Record<string, string> = { ishan: "bg-emerald-600", mia: "bg-indigo-600", devon: "bg-amber-600", sana: "bg-rose-600", leo: "bg-cyan-600" };
  return (
    <span className={`inline-flex size-6 items-center justify-center rounded-full text-xs font-bold text-white ${hue[who] ?? "bg-slate-500"}`}>
      {who[0].toUpperCase()}
    </span>
  );
}

function ChangeChip({ id }: { id: string }) {
  return <code className="rounded bg-violet-50 px-1.5 py-0.5 text-xs font-semibold text-violet-700">{id}</code>;
}
function RevChip({ hash }: { hash: string }) {
  return <code className="rounded bg-slate-100 px-1.5 py-0.5 text-xs text-slate-600">{hash}</code>;
}

// ── Code tab: tree + file + history ─────────────────────────────────────────

function Code() {
  const [path, setPath] = useState("src/limiter.rs");
  const [showLog, setShowLog] = useState(false);
  const tip = changes.find((c) => c.branches?.includes("main"))!;
  return (
    <div>
      <div className="mb-4 flex items-center gap-3 text-sm">
        <select className="rounded-md border border-slate-300 bg-white px-2 py-1.5 font-semibold">
          {repoMeta.branches.map((b) => (
            <option key={b}>{b}</option>
          ))}
        </select>
        <span className="text-slate-500">
          {repoMeta.artifactCount.toLocaleString()} artifacts · {repoMeta.size}
        </span>
        <button
          onClick={() => setShowLog(!showLog)}
          className="ml-auto cursor-pointer rounded-md border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-50"
        >
          {showLog ? "Files" : `Log — ${changes.length} changes`}
        </button>
      </div>

      <div className="mb-4 flex items-center gap-2 rounded-md border border-slate-200 bg-slate-50 px-4 py-2.5 text-sm">
        <Avatar who={tip.author} />
        <span className="font-medium">{tip.message}</span>
        <span className="ml-auto flex items-center gap-2 text-slate-500">
          <ChangeChip id={tip.id} /> <RevChip hash={tip.revisions.at(-1)!.hash} /> {tip.when} ago
        </span>
      </div>

      {showLog ? (
        <LogList />
      ) : (
        <div className="flex gap-4">
          <div className="w-64 shrink-0 rounded-md border border-slate-200">
            <Tree nodes={tree} depth={0} selected={path} onPick={setPath} />
          </div>
          <FileView path={path} />
        </div>
      )}
    </div>
  );
}

function Tree({ nodes, depth, selected, onPick }: { nodes: TreeNode[]; depth: number; selected: string; onPick: (p: string) => void }) {
  return (
    <ul className="py-1 text-sm">
      {nodes.map((n) => (
        <li key={n.path}>
          <button
            onClick={() => n.kind === "file" && onPick(n.path)}
            style={{ paddingLeft: 12 + depth * 16 }}
            className={`flex w-full cursor-pointer items-center gap-2 py-1 text-left hover:bg-slate-50 ${
              selected === n.path ? "bg-emerald-50 font-medium text-emerald-800" : ""
            }`}
          >
            <span className="text-slate-400">{n.kind === "dir" ? "▾" : "·"}</span>
            {n.name}
          </button>
          {n.children && <Tree nodes={n.children} depth={depth + 1} selected={selected} onPick={onPick} />}
        </li>
      ))}
    </ul>
  );
}

function FileView({ path }: { path: string }) {
  const lines = fileContents[path];
  const last = tree
    .flatMap(function flat(n: TreeNode): TreeNode[] {
      return [n, ...(n.children?.flatMap(flat) ?? [])];
    })
    .find((n) => n.path === path)?.lastChange;
  return (
    <div className="min-w-0 flex-1 rounded-md border border-slate-200">
      <div className="flex items-center gap-2 border-b border-slate-200 bg-slate-50 px-4 py-2 text-sm">
        <span className="font-mono">{path}</span>
        {last && (
          <span className="ml-auto text-slate-500">
            last touched by <ChangeChip id={last} /> — {changeById(last).message}
          </span>
        )}
      </div>
      {lines ? (
        <pre className="overflow-x-auto p-4 text-xs leading-5">
          {lines.map((l, i) => (
            <div key={i} className="flex">
              <span className="w-8 shrink-0 select-none text-right text-slate-300">{i + 1}</span>
              <span className="pl-4">{l || " "}</span>
            </div>
          ))}
        </pre>
      ) : (
        <p className="p-6 text-sm text-slate-400">(fake file — no content wired for {path})</p>
      )}
    </div>
  );
}

function LogList() {
  return (
    <ol className="divide-y divide-slate-100 rounded-md border border-slate-200">
      {changes.map((c) => (
        <li key={c.id} className="flex items-center gap-3 px-4 py-2.5 text-sm">
          <Avatar who={c.author} />
          <span className="font-medium">{c.message}</span>
          {c.branches?.map((b) => (
            <span key={b} className="rounded-full bg-emerald-100 px-2 text-xs font-semibold text-emerald-800">{b}</span>
          ))}
          {c.inProposal && (
            <span className="rounded-full bg-violet-100 px-2 text-xs text-violet-700">proposal #{c.inProposal}</span>
          )}
          <span className="ml-auto flex items-center gap-2 text-slate-500">
            {c.revisions.length > 1 && <span className="text-xs">{c.revisions.length} revisions</span>}
            <ChangeChip id={c.id} />
            <RevChip hash={c.revisions.at(-1)!.hash} />
            {c.when}
          </span>
        </li>
      ))}
    </ol>
  );
}

// ── Proposals tab: list → detail (Conversation / Changes) ───────────────────

function Proposals() {
  const [openId, setOpenId] = useState<number | null>(218);
  if (openId != null) return <ProposalDetail id={openId} onBack={() => setOpenId(null)} />;
  return (
    <ol className="divide-y divide-slate-100 rounded-md border border-slate-200">
      {proposals.map((p) => (
        <li key={p.id} className="flex items-center gap-3 px-4 py-3 text-sm">
          <StatusPill status={p.status} />
          <button onClick={() => setOpenId(p.id)} className="cursor-pointer font-semibold hover:text-emerald-700">
            {p.title}
          </button>
          <span className="text-slate-500">#{p.id} · {p.stack.length} change{p.stack.length > 1 ? "s" : ""} → {p.target}</span>
          {p.trialRebase === "would-conflict" && (
            <span className="rounded-full bg-amber-100 px-2 text-xs font-medium text-amber-800">would conflict</span>
          )}
          <span className="ml-auto flex items-center gap-2 text-slate-500">
            {p.approvals.length > 0 && <span className="text-emerald-700">✓ {p.approvals.length}</span>}
            <Avatar who={p.author} /> {p.when}
          </span>
        </li>
      ))}
    </ol>
  );
}

function StatusPill({ status }: { status: string }) {
  const style = { open: "bg-emerald-600", landed: "bg-violet-600", closed: "bg-slate-500" }[status];
  return <span className={`rounded-full px-2.5 py-0.5 text-xs font-semibold text-white ${style}`}>{status}</span>;
}

function ProposalDetail({ id, onBack }: { id: number; onBack: () => void }) {
  const p = proposalById(id);
  const [tab, setTab] = useState<"conversation" | "changes">("conversation");
  return (
    <div>
      <button onClick={onBack} className="mb-3 cursor-pointer text-sm text-slate-500 hover:text-slate-800">← all proposals</button>
      <div className="mb-1 flex items-center gap-3">
        <h1 className="text-xl font-semibold">{p.title} <span className="font-normal text-slate-400">#{p.id}</span></h1>
      </div>
      <div className="mb-4 flex items-center gap-2 text-sm text-slate-600">
        <StatusPill status={p.status} />
        <span>
          @{p.author} proposes <b>{p.stack.length} changes</b> onto <code className="rounded bg-slate-100 px-1">{p.target}</code>
        </span>
      </div>

      <div className="mb-4 flex gap-1 border-b border-slate-200 text-sm">
        {(["conversation", "changes"] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`cursor-pointer px-4 py-2 capitalize ${tab === t ? "border-b-2 border-emerald-600 font-semibold" : "text-slate-500"}`}
          >
            {t === "changes" ? `changes (${p.stack.length})` : t}
          </button>
        ))}
      </div>

      <div className="flex gap-6">
        <div className="min-w-0 flex-1">{tab === "conversation" ? <Conversation id={id} /> : <ChangesTab id={id} />}</div>
        <LandPanel id={id} />
      </div>
    </div>
  );
}

function Conversation({ id }: { id: number }) {
  const p = proposalById(id);
  return (
    <ol className="space-y-3">
      {p.events.map((e, i) => (
        <li key={i} className="text-sm">
          {e.kind === "comment" ? (
            <div className="rounded-md border border-slate-200">
              <div className="flex items-center gap-2 border-b border-slate-100 bg-slate-50 px-3 py-2">
                <Avatar who={e.who} /> <b>@{e.who}</b> <span className="text-slate-400">{e.when} ago</span>
              </div>
              <p className="px-3 py-2.5">{e.body}</p>
            </div>
          ) : (
            <div className="flex items-center gap-2 px-1 text-slate-600">
              <Avatar who={e.who} />
              {e.kind === "opened" && <span>opened this proposal · {e.when} ago</span>}
              {e.kind === "approve" && (
                <span>
                  <b className="text-emerald-700">approved</b> at revision <RevChip hash={e.revision} /> · {e.when} ago
                </span>
              )}
              {e.kind === "amend" && (
                <span>
                  amended <ChangeChip id={e.changeId} /> <RevChip hash={e.from} /> → <RevChip hash={e.to} /> ({e.note}) ·{" "}
                  <button className="cursor-pointer font-medium text-violet-700 hover:underline">view interdiff</button> · {e.when} ago
                </span>
              )}
            </div>
          )}
        </li>
      ))}
      <li>
        <textarea placeholder="Leave a comment…" className="w-full rounded-md border border-slate-300 p-3 text-sm" rows={3} />
      </li>
    </ol>
  );
}

function ChangesTab({ id }: { id: number }) {
  const p = proposalById(id);
  const [sel, setSel] = useState(p.stack[0]);
  const [view, setView] = useState<"diff" | "interdiff">("diff");
  const c = changeById(sel);
  const diffs = view === "interdiff" ? interdiffs[sel] : changeDiffs[sel];
  return (
    <div>
      <div className="mb-3 flex items-center gap-2 text-sm">
        {p.stack.map((cid, i) => (
          <button
            key={cid}
            onClick={() => { setSel(cid); setView("diff"); }}
            className={`cursor-pointer rounded-md border px-3 py-1.5 ${sel === cid ? "border-violet-400 bg-violet-50 font-semibold" : "border-slate-200 hover:bg-slate-50"}`}
          >
            {i + 1}. <ChangeChip id={cid} /> {changeById(cid).message}
          </button>
        ))}
      </div>
      <div className="mb-3 flex items-center gap-2 text-xs text-slate-600">
        revisions:
        {c.revisions.map((r) => (
          <RevChip key={r.hash} hash={`${r.label} ${r.hash}`} />
        ))}
        {c.revisions.length > 1 && (
          <button
            onClick={() => setView(view === "diff" ? "interdiff" : "diff")}
            className={`cursor-pointer rounded-full px-2.5 py-0.5 ${view === "interdiff" ? "bg-violet-600 text-white" : "bg-violet-100 text-violet-700"}`}
          >
            {view === "interdiff" ? "showing interdiff r1 → r2" : "what changed since r1?"}
          </button>
        )}
      </div>
      {(diffs ?? []).map((d) => (
        <DiffBlock key={d.path} diff={d} threads={p.threads.filter((t) => t.changeId === sel && t.path === d.path)} />
      ))}
      {!diffs?.length && <p className="rounded-md border border-slate-200 p-6 text-sm text-slate-400">no diff wired for this change</p>}
    </div>
  );
}

function DiffBlock({ diff, threads }: { diff: FileDiff; threads: import("../data").Thread[] }) {
  const color = { add: "bg-emerald-50", del: "bg-rose-50", ctx: "", hunk: "bg-slate-100 text-slate-500" };
  return (
    <div className="mb-4 overflow-hidden rounded-md border border-slate-200">
      <div className="flex items-center gap-2 border-b border-slate-200 bg-slate-50 px-3 py-2 text-sm">
        <span className="font-mono">{diff.path}</span>
        <span className={`rounded-full px-2 text-xs ${diff.status === "added" ? "bg-emerald-100 text-emerald-800" : "bg-amber-100 text-amber-800"}`}>{diff.status}</span>
      </div>
      <pre className="text-xs leading-5">
        {diff.lines.map((l, i) => {
          const lineNo = l.new ?? l.old;
          const thread = threads.find((t) => t.line === lineNo && (l.kind === "add" || l.kind === "ctx"));
          return (
            <div key={i}>
              <div className={`flex px-3 ${color[l.kind]}`}>
                <span className="w-8 shrink-0 select-none text-right text-slate-300">{lineNo ?? ""}</span>
                <span className="w-5 shrink-0 pl-2 text-slate-400">{l.kind === "add" ? "+" : l.kind === "del" ? "−" : ""}</span>
                <span>{l.text}</span>
              </div>
              {thread && <ThreadCard thread={thread} />}
            </div>
          );
        })}
      </pre>
    </div>
  );
}

function ThreadCard({ thread }: { thread: import("../data").Thread }) {
  const c = changeById(thread.changeId);
  const outdated = thread.revision !== c.revisions.at(-1)!.hash;
  return (
    <div className="mx-10 my-2 rounded-md border border-slate-300 bg-white font-sans shadow-sm">
      <div className="flex items-center gap-2 border-b border-slate-100 px-3 py-1.5 text-xs text-slate-500">
        thread on <RevChip hash={thread.revision} />
        {outdated && <span className="rounded-full bg-amber-100 px-2 text-amber-800">outdated — written on r1</span>}
        <span className={`ml-auto ${thread.resolved ? "text-emerald-700" : "text-slate-400"}`}>{thread.resolved ? "✓ resolved" : "open"}</span>
      </div>
      {thread.comments.map((cm, i) => (
        <p key={i} className="px-3 py-1.5 text-xs">
          <b>@{cm.author}</b> <span className="text-slate-400">{cm.when}</span> — {cm.body}
        </p>
      ))}
      <div className="flex gap-2 border-t border-slate-100 px-3 py-1.5 text-xs">
        <button className="cursor-pointer text-slate-500 hover:text-slate-800">reply</button>
        <button className="cursor-pointer text-slate-500 hover:text-slate-800">{thread.resolved ? "unresolve" : "resolve"}</button>
      </div>
    </div>
  );
}

function LandPanel({ id }: { id: number }) {
  const p = proposalById(id);
  const openThreads = p.threads.filter((t) => !t.resolved).length;
  return (
    <aside className="w-72 shrink-0 space-y-4 text-sm">
      <section className="rounded-md border border-slate-200 p-4">
        <h3 className="mb-2 font-semibold">Review</h3>
        {p.approvals.map((a) => (
          <p key={a.who} className="flex items-center gap-2 text-emerald-700">
            ✓ @{a.who} approved <span className="text-slate-400">at <RevChip hash={a.revision} /></span>
          </p>
        ))}
        {p.approvals.length === 0 && <p className="text-slate-400">no approvals yet</p>}
        <p className="mt-1 text-slate-500">{openThreads} open thread{openThreads === 1 ? "" : "s"}</p>
        <button className="mt-3 w-full cursor-pointer rounded-md border border-emerald-600 py-1.5 font-semibold text-emerald-700 hover:bg-emerald-50">
          Approve
        </button>
      </section>
      <section className="rounded-md border border-slate-200 p-4">
        <h3 className="mb-2 font-semibold">Land</h3>
        {p.trialRebase === "clean" ? (
          <p className="text-slate-600">
            Rebases cleanly onto <code className="rounded bg-slate-100 px-1">{p.target}</code>. Landing rebases the stack and
            fast-forwards — <b>{p.target} stays linear</b>.
          </p>
        ) : (
          <p className="text-amber-800">
            Trial rebase conflicts in <code>{p.conflictFiles?.join(", ")}</code>. The hub will refuse to land a conflicted
            result — pull, resolve, amend; the proposal updates itself.
          </p>
        )}
        <button
          disabled={p.trialRebase !== "clean" || p.status !== "open"}
          className="mt-3 w-full cursor-pointer rounded-md bg-emerald-600 py-2 font-semibold text-white hover:bg-emerald-700 disabled:cursor-not-allowed disabled:bg-slate-300"
        >
          {p.status === "landed" ? "Landed" : `Land ${p.stack.length} changes`}
        </button>
      </section>
      <section className="rounded-md border border-slate-200 p-4 text-xs text-slate-500">
        Amends track automatically: push a rewrite and this proposal advances. Comments stay anchored to the revision they
        were written on.
      </section>
    </aside>
  );
}

// ── Environments tab ─────────────────────────────────────────────────────────

function Environments() {
  return (
    <div className="space-y-6">
      <p className="rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-900">
        <b>Ciphertext only.</b> {honestBanner}
      </p>
      <div className="grid grid-cols-3 gap-4">
        {envs.map((e) => (
          <section key={e.name} className="rounded-md border border-slate-200 p-4 text-sm">
            <div className="mb-2 flex items-center gap-2">
              <h3 className="font-mono text-base font-semibold">{e.name}</h3>
              <span className="rounded-full bg-slate-100 px-2 text-xs text-slate-600">gen {e.generation}</span>
            </div>
            <p className="text-slate-600">{e.members} members</p>
            <p className="text-slate-600">
              bundle replaced {e.bundleUpdated} ago <span className="text-slate-400">by @{e.bundleUpdatedBy}</span>
            </p>
            <p className="mt-1 font-mono text-xs text-slate-400">contents: ■■■■■■■■ (opaque)</p>
            {e.note && <p className="mt-2 rounded bg-amber-50 px-2 py-1 text-xs text-amber-800">{e.note}</p>}
          </section>
        ))}
      </div>
      <div className="grid grid-cols-2 gap-6">
        <section>
          <h3 className="mb-2 text-sm font-semibold">Members & keys</h3>
          <table className="w-full rounded-md border border-slate-200 text-sm">
            <tbody>
              {members.map((m) => (
                <tr key={m.handle} className="border-b border-slate-100 last:border-0">
                  <td className="px-3 py-2"><Avatar who={m.handle} /> <b className="ml-1">@{m.handle}</b></td>
                  <td className="px-3 py-2 font-mono text-xs text-slate-500">{m.fingerprint}</td>
                  <td className="px-3 py-2 text-xs text-slate-600">{m.envs.join(" · ")}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="mt-2 text-xs text-slate-400">
            Verify fingerprints out-of-band — this hub is not a trust root. Granting and revoking happen in the CLI, signed
            by a member key.
          </p>
        </section>
        <section>
          <h3 className="mb-2 text-sm font-semibold">Keyring history</h3>
          <ol className="space-y-2 text-sm">
            {keyringLog.map((e, i) => (
              <li key={i} className="flex gap-3 rounded-md border border-slate-200 px-3 py-2">
                <span className="w-10 shrink-0 text-xs text-slate-400">{e.when}</span>
                <span>{e.what}</span>
                <span className="ml-auto shrink-0 text-xs text-slate-500">signed @{e.signedBy}</span>
              </li>
            ))}
          </ol>
        </section>
      </div>
    </div>
  );
}
