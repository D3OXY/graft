// Variant B — "Change-first": the model leads. The repo home IS the log (a
// rendered `graft log`); files are reached by drilling into a change's tree.
// Proposal review is Gerrit-shaped: stack rail, one change at a time, revision
// selector with interdiff. Environments read as a signed audit trail.
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

export function ChangeFirst({ screen, setScreen }: VariantProps) {
  const nav: { key: Screen; label: string; hint: string }[] = [
    { key: "repo", label: "Log", hint: "the repo is its changes" },
    { key: "proposals", label: "Proposals", hint: "stacks under review" },
    { key: "envs", label: "Keyring", hint: "environments & members" },
  ];
  return (
    <div className="flex min-h-screen bg-stone-100 text-stone-900">
      <aside className="flex w-52 shrink-0 flex-col gap-1 border-r border-stone-200 bg-white p-4">
        <div className="mb-4">
          <p className="text-xs uppercase tracking-wide text-stone-400">hub.acme.dev</p>
          <p className="font-semibold">{repoMeta.owner}/{repoMeta.name}</p>
        </div>
        {nav.map((n) => (
          <button
            key={n.key}
            onClick={() => setScreen(n.key)}
            className={`cursor-pointer rounded-lg px-3 py-2 text-left text-sm ${
              screen === n.key ? "bg-stone-900 text-white" : "hover:bg-stone-100"
            }`}
          >
            <span className="block font-medium">{n.label}</span>
            <span className={`block text-xs ${screen === n.key ? "text-stone-300" : "text-stone-400"}`}>{n.hint}</span>
          </button>
        ))}
        <div className="mt-auto text-xs text-stone-400">
          <p>{repoMeta.artifactCount.toLocaleString()} artifacts</p>
          <p>{repoMeta.size} · signed in as @ishan</p>
        </div>
      </aside>
      <main className="min-w-0 flex-1 p-6 pb-24">
        {screen === "repo" && <LogHome />}
        {screen === "proposals" && <Review />}
        {screen === "envs" && <Keyring />}
      </main>
    </div>
  );
}

function Cid({ id }: { id: string }) {
  return <code className="font-semibold text-violet-700">{id}</code>;
}
function Rev({ hash }: { hash: string }) {
  return <code className="text-stone-500">{hash}</code>;
}

// ── Log as home; a change drills into its revisions + tree ──────────────────

function LogHome() {
  const [sel, setSel] = useState<string | null>("tzunmyk");
  return (
    <div className="flex gap-6">
      <div className="min-w-0 flex-1">
        <h1 className="mb-1 text-lg font-semibold">Log</h1>
        <p className="mb-4 text-sm text-stone-500">
          Every row is a <b>change</b> — a stable identity that survives rewriting. Branches and proposals are labels on
          the graph, not places you go.
        </p>
        <ol className="font-mono text-sm">
          {changes.map((c, i) => (
            <li key={c.id}>
              <button
                onClick={() => setSel(sel === c.id ? null : c.id)}
                className={`flex w-full cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-left ${
                  sel === c.id ? "bg-white shadow-sm ring-1 ring-stone-200" : "hover:bg-stone-200/50"
                }`}
              >
                <span className="text-stone-400">{i === 0 ? "◉" : "○"}</span>
                <Cid id={c.id} />
                <span className="w-12 shrink-0 text-stone-500">{c.author}</span>
                <span className="w-8 shrink-0 text-stone-400">{c.when}</span>
                <span className="truncate font-sans">{c.message}</span>
                <span className="ml-auto flex shrink-0 gap-1.5">
                  {c.revisions.length > 1 && (
                    <span className="rounded bg-violet-100 px-1.5 font-sans text-xs text-violet-700">
                      {c.revisions.length} rev
                    </span>
                  )}
                  {c.inProposal && (
                    <span className="rounded bg-blue-100 px-1.5 font-sans text-xs text-blue-700">#{c.inProposal}</span>
                  )}
                  {c.branches?.map((b) => (
                    <span key={b} className="rounded bg-emerald-600 px-1.5 font-sans text-xs font-semibold text-white">{b}</span>
                  ))}
                </span>
              </button>
            </li>
          ))}
        </ol>
      </div>
      {sel && <ChangeInspector id={sel} onClose={() => setSel(null)} />}
    </div>
  );
}

function ChangeInspector({ id, onClose }: { id: string; onClose: () => void }) {
  const c = changeById(id);
  const [path, setPath] = useState<string | null>(null);
  const lines = path ? fileContents[path] : null;
  return (
    <aside className="w-96 shrink-0 rounded-xl bg-white p-4 text-sm shadow-sm ring-1 ring-stone-200">
      <div className="mb-2 flex items-start justify-between">
        <div>
          <p className="font-mono"><Cid id={c.id} /></p>
          <p className="font-medium">{c.message}</p>
          <p className="text-xs text-stone-500">@{c.author} · {c.when} ago {c.inProposal && <>· in proposal #{c.inProposal}</>}</p>
        </div>
        <button onClick={onClose} className="cursor-pointer text-stone-400 hover:text-stone-700">✕</button>
      </div>
      <div className="mb-3 rounded-lg bg-stone-50 p-2 text-xs">
        <p className="mb-1 font-semibold text-stone-500">revisions of this change</p>
        {c.revisions.map((r, i) => (
          <p key={r.hash} className="flex gap-2">
            <span className="text-stone-400">{r.label}</span>
            <Rev hash={r.hash} />
            <span className="text-stone-400">{r.when}</span>
            {r.note && <span className="text-stone-500">— {r.note}</span>}
            {i === c.revisions.length - 1 && <span className="ml-auto text-emerald-700">current</span>}
          </p>
        ))}
      </div>
      <p className="mb-1 text-xs font-semibold text-stone-500">tree at {c.revisions.at(-1)!.hash}</p>
      <MiniTree nodes={tree} depth={0} onPick={setPath} selected={path} />
      {path && (
        <pre className="mt-3 max-h-64 overflow-auto rounded-lg bg-stone-900 p-3 text-xs leading-5 text-stone-100">
          {(lines ?? ["(fake file — no content wired)"]).join("\n")}
        </pre>
      )}
    </aside>
  );
}

function MiniTree({ nodes, depth, onPick, selected }: { nodes: TreeNode[]; depth: number; onPick: (p: string) => void; selected: string | null }) {
  return (
    <ul className="text-xs">
      {nodes.map((n) => (
        <li key={n.path}>
          <button
            onClick={() => n.kind === "file" && onPick(n.path)}
            style={{ paddingLeft: depth * 12 }}
            className={`flex w-full cursor-pointer justify-between gap-2 rounded px-1 py-0.5 text-left hover:bg-stone-100 ${
              selected === n.path ? "bg-violet-50 text-violet-800" : ""
            }`}
          >
            <span className="font-mono">{n.kind === "dir" ? `${n.name}/` : n.name}</span>
            {n.lastChange && <span className="text-stone-400">{n.lastChange}</span>}
          </button>
          {n.children && <MiniTree nodes={n.children} depth={depth + 1} onPick={onPick} selected={selected} />}
        </li>
      ))}
    </ul>
  );
}

// ── Review: stack rail | diff | threads ──────────────────────────────────────

function Review() {
  const [pid, setPid] = useState(218);
  const p = proposalById(pid);
  const [sel, setSel] = useState(p.stack[0]);
  const [interdiff, setInterdiff] = useState(false);
  const c = changeById(sel);
  const diffs = interdiff ? interdiffs[sel] : changeDiffs[sel];
  const threads = p.threads.filter((t) => t.changeId === sel);

  return (
    <div>
      <div className="mb-4 flex items-center gap-2 text-sm">
        {proposals.map((pr) => (
          <button
            key={pr.id}
            onClick={() => { setPid(pr.id); setSel(pr.stack[0]); setInterdiff(false); }}
            className={`cursor-pointer rounded-full px-3 py-1 ${
              pid === pr.id ? "bg-stone-900 text-white" : "bg-white ring-1 ring-stone-200 hover:bg-stone-50"
            }`}
          >
            #{pr.id} {pr.title}
            {pr.status === "landed" && " · landed"}
            {pr.trialRebase === "would-conflict" && " ⚠"}
          </button>
        ))}
      </div>

      <div className="flex gap-4">
        <aside className="w-64 shrink-0">
          <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-stone-400">
            stack → {p.target}
          </p>
          <ol className="space-y-2">
            {p.stack.map((cid, i) => {
              const cc = changeById(cid);
              const n = p.threads.filter((t) => t.changeId === cid && !t.resolved).length;
              return (
                <li key={cid}>
                  <button
                    onClick={() => { setSel(cid); setInterdiff(false); }}
                    className={`w-full cursor-pointer rounded-lg p-3 text-left text-sm ${
                      sel === cid ? "bg-white shadow-sm ring-2 ring-violet-400" : "bg-white/60 ring-1 ring-stone-200 hover:bg-white"
                    }`}
                  >
                    <p className="font-mono text-xs text-stone-400">{i + 1} of {p.stack.length} · <Cid id={cid} /></p>
                    <p className="font-medium">{cc.message}</p>
                    <p className="mt-1 flex gap-2 text-xs text-stone-500">
                      <span>{cc.revisions.length} revision{cc.revisions.length > 1 ? "s" : ""}</span>
                      {n > 0 && <span className="text-amber-700">{n} open thread{n > 1 ? "s" : ""}</span>}
                    </p>
                  </button>
                </li>
              );
            })}
          </ol>
          <div className="mt-4 rounded-lg bg-white p-3 text-sm ring-1 ring-stone-200">
            {p.approvals.map((a) => (
              <p key={a.who} className="text-emerald-700">✓ @{a.who} approved at <Rev hash={a.revision} /></p>
            ))}
            {p.trialRebase === "clean" ? (
              <p className="mt-1 text-xs text-stone-500">rebases cleanly onto {p.target}</p>
            ) : (
              <p className="mt-1 text-xs text-amber-700">would conflict in {p.conflictFiles?.join(", ")} — landing refused until amended</p>
            )}
            <div className="mt-2 flex gap-2">
              <button className="flex-1 cursor-pointer rounded-md py-1.5 text-sm font-semibold ring-1 ring-stone-300 hover:bg-stone-50">Approve</button>
              <button
                disabled={p.trialRebase !== "clean" || p.status !== "open"}
                className="flex-1 cursor-pointer rounded-md bg-stone-900 py-1.5 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:bg-stone-300"
              >
                {p.status === "landed" ? "Landed" : "Land stack"}
              </button>
            </div>
          </div>
        </aside>

        <div className="min-w-0 flex-1">
          <div className="mb-2 flex items-center gap-2 text-xs">
            {c.revisions.map((r, i) => (
              <span key={r.hash} className={`rounded-full px-2 py-0.5 ${i === c.revisions.length - 1 ? "bg-stone-900 text-white" : "bg-white ring-1 ring-stone-200"}`}>
                {r.label} {r.hash}
              </span>
            ))}
            {c.revisions.length > 1 && (
              <button
                onClick={() => setInterdiff(!interdiff)}
                className={`cursor-pointer rounded-full px-2 py-0.5 ${interdiff ? "bg-violet-600 text-white" : "text-violet-700 ring-1 ring-violet-300"}`}
              >
                interdiff r1→r2
              </button>
            )}
          </div>
          {(diffs ?? []).map((d) => (
            <Diff key={d.path} diff={d} />
          ))}
        </div>

        <aside className="w-72 shrink-0">
          <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-stone-400">threads on <Cid id={sel} /></p>
          {threads.length === 0 && <p className="text-sm text-stone-400">none yet — click a diff line (fake)</p>}
          <ol className="space-y-3">
            {threads.map((t) => {
              const outdated = t.revision !== c.revisions.at(-1)!.hash;
              return (
                <li key={t.id} className="rounded-lg bg-white p-3 text-xs ring-1 ring-stone-200">
                  <p className="mb-1 font-mono text-stone-400">
                    {t.path}:{t.line} · on {t.revision}
                    {outdated && <span className="ml-1 rounded bg-amber-100 px-1 text-amber-800">outdated</span>}
                  </p>
                  {t.comments.map((cm, i) => (
                    <p key={i} className="mb-1"><b>@{cm.author}</b> — {cm.body}</p>
                  ))}
                  <button className="cursor-pointer text-stone-500 hover:text-stone-900">
                    {t.resolved ? "✓ resolved" : "resolve"}
                  </button>
                </li>
              );
            })}
          </ol>
        </aside>
      </div>
    </div>
  );
}

function Diff({ diff }: { diff: FileDiff }) {
  const bg = { add: "bg-emerald-50", del: "bg-rose-50", ctx: "bg-white", hunk: "bg-stone-100 text-stone-400" };
  return (
    <div className="mb-4 overflow-hidden rounded-lg ring-1 ring-stone-200">
      <p className="bg-stone-900 px-3 py-1.5 font-mono text-xs text-stone-100">{diff.path} <span className="text-stone-400">({diff.status})</span></p>
      <pre className="text-xs leading-5">
        {diff.lines.map((l, i) => (
          <div key={i} className={`flex px-3 ${bg[l.kind]}`}>
            <span className="w-7 shrink-0 select-none text-right text-stone-300">{l.new ?? l.old ?? ""}</span>
            <span className="w-5 shrink-0 pl-2 text-stone-400">{l.kind === "add" ? "+" : l.kind === "del" ? "−" : ""}</span>
            <span>{l.text}</span>
          </div>
        ))}
      </pre>
    </div>
  );
}

// ── Keyring: audit-first environments ────────────────────────────────────────

function Keyring() {
  return (
    <div className="max-w-4xl">
      <h1 className="mb-1 text-lg font-semibold">Keyring</h1>
      <p className="mb-5 max-w-2xl text-sm text-stone-500">{honestBanner}</p>

      <div className="mb-6 overflow-hidden rounded-xl bg-white ring-1 ring-stone-200">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-stone-200 text-left text-xs uppercase tracking-wide text-stone-400">
              <th className="px-4 py-2">member</th>
              <th className="px-4 py-2">key fingerprint</th>
              {envs.map((e) => (
                <th key={e.name} className="px-4 py-2 text-center font-mono normal-case">
                  {e.name}
                  <span className="block font-sans text-[10px] text-stone-400">gen {e.generation}</span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {members.map((m) => (
              <tr key={m.handle} className="border-b border-stone-100 last:border-0">
                <td className="px-4 py-2 font-medium">@{m.handle}</td>
                <td className="px-4 py-2 font-mono text-xs text-stone-500">{m.fingerprint}</td>
                {envs.map((e) => (
                  <td key={e.name} className="px-4 py-2 text-center">
                    {m.envs.includes(e.name) ? (
                      <span className="text-emerald-600">●</span>
                    ) : (
                      <span className="text-stone-200">○</span>
                    )}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        <p className="border-t border-stone-100 px-4 py-2 text-xs text-stone-400">
          ● = holds a wrapped key for that environment’s current generation. Membership changes are CLI operations signed
          by a member — the hub cannot grant access.
        </p>
      </div>

      <h2 className="mb-2 text-sm font-semibold">Signed history</h2>
      <ol className="relative space-y-0 border-l-2 border-stone-200 pl-5">
        {keyringLog.map((e, i) => (
          <li key={i} className="relative pb-4">
            <span className={`absolute -left-[26px] top-1 size-3 rounded-full ring-4 ring-stone-100 ${
              { rotate: "bg-blue-500", grant: "bg-emerald-500", remove: "bg-amber-500", bundle: "bg-stone-400", root: "bg-violet-600" }[e.kind]
            }`} />
            <p className="text-sm">{e.what}</p>
            <p className="text-xs text-stone-400">{e.when} ago · signed by @{e.signedBy}</p>
          </li>
        ))}
      </ol>
    </div>
  );
}
