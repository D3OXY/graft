// Variant C — "Console": dense, dark, monospace. The hub as an operator surface
// that keeps CLI muscle memory: multi-pane layouts, a statusline, bracketed
// verbs. Bets that graft users live in terminals and the hub should feel like one.
import { useState, type ReactNode } from "react";
import type { Screen, VariantProps } from "../App";
import {
  changeById,
  changeDiffs,
  changes,
  envs,
  fileContents,
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

export function Console({ screen, setScreen }: VariantProps) {
  const items: { key: Screen; label: string }[] = [
    { key: "repo", label: "1:browse" },
    { key: "proposals", label: "2:proposals" },
    { key: "envs", label: "3:env" },
  ];
  return (
    <div className="flex h-screen flex-col bg-zinc-950 font-mono text-[13px] text-zinc-300">
      <header className="flex items-center gap-4 border-b border-zinc-800 px-4 py-2">
        <span className="font-bold text-lime-400">graft-hub</span>
        <span className="text-zinc-500">{repoMeta.owner}/{repoMeta.name}</span>
        <nav className="flex gap-1">
          {items.map((i) => (
            <button
              key={i.key}
              onClick={() => setScreen(i.key)}
              className={`cursor-pointer px-2 py-0.5 ${screen === i.key ? "bg-lime-400 font-bold text-zinc-950" : "text-zinc-400 hover:text-zinc-100"}`}
            >
              {i.label}
            </button>
          ))}
        </nav>
        <span className="ml-auto text-zinc-600">@ishan · hub.acme.dev</span>
      </header>
      <div className="min-h-0 flex-1 overflow-hidden p-2">
        {screen === "repo" && <Browse />}
        {screen === "proposals" && <ProposalPane />}
        {screen === "envs" && <EnvPane />}
      </div>
      <footer className="flex gap-5 border-t border-zinc-800 px-4 py-1.5 text-xs text-zinc-500">
        <span><b className="text-zinc-300">j/k</b> move</span>
        <span><b className="text-zinc-300">enter</b> open</span>
        <span><b className="text-zinc-300">i</b> interdiff</span>
        <span><b className="text-zinc-300">a</b> approve</span>
        <span><b className="text-zinc-300">L</b> land</span>
        <span className="ml-auto">main @ povwkil e41c02 · {repoMeta.artifactCount.toLocaleString()} artifacts · sync 2m ago</span>
      </footer>
    </div>
  );
}

function Pane({ title, children, className = "" }: { title: string; children: ReactNode; className?: string }) {
  return (
    <section className={`flex min-h-0 flex-col border border-zinc-800 ${className}`}>
      <p className="border-b border-zinc-800 bg-zinc-900 px-2 py-1 text-xs text-zinc-500">── {title} ──</p>
      <div className="min-h-0 flex-1 overflow-auto">{children}</div>
    </section>
  );
}

const cid = (id: string) => <span className="text-fuchsia-400">{id}</span>;

// ── browse: log | tree | file ────────────────────────────────────────────────

function Browse() {
  const [selChange, setSelChange] = useState("povwkil");
  const [path, setPath] = useState("src/limiter.rs");
  const c = changeById(selChange);
  return (
    <div className="grid h-full grid-cols-[1.3fr_0.8fr_1.6fr] gap-2">
      <Pane title={`log · ${changes.length} changes`}>
        {changes.map((ch) => (
          <button
            key={ch.id}
            onClick={() => setSelChange(ch.id)}
            className={`flex w-full cursor-pointer gap-2 px-2 py-0.5 text-left ${selChange === ch.id ? "bg-zinc-800 text-zinc-100" : "hover:bg-zinc-900"}`}
          >
            <span className="text-zinc-600">○</span>
            {cid(ch.id)}
            <span className="w-11 shrink-0 text-zinc-500">{ch.author}</span>
            <span className="truncate">{ch.message}</span>
            <span className="ml-auto flex shrink-0 gap-1">
              {ch.inProposal && <span className="text-sky-400">#{ch.inProposal}</span>}
              {ch.branches?.map((b) => (
                <span key={b} className="bg-emerald-500 px-1 font-bold text-zinc-950">{b}</span>
              ))}
            </span>
          </button>
        ))}
        <div className="mt-2 border-t border-zinc-800 px-2 py-1.5 text-xs text-zinc-500">
          <p>{cid(c.id)} — {c.message}</p>
          <p>revisions: {c.revisions.map((r) => `${r.label}:${r.hash}`).join("  ")}</p>
        </div>
      </Pane>
      <Pane title={`tree @ ${c.revisions.at(-1)!.hash}`}>
        <FlatTree nodes={tree} depth={0} selected={path} onPick={setPath} />
      </Pane>
      <Pane title={path}>
        <pre className="p-2 leading-5">
          {(fileContents[path] ?? ["(fake file — no content wired)"]).map((l, i) => (
            <div key={i} className="flex">
              <span className="w-7 shrink-0 select-none text-right text-zinc-700">{i + 1}</span>
              <span className="pl-3 text-zinc-200">{l || " "}</span>
            </div>
          ))}
        </pre>
      </Pane>
    </div>
  );
}

function FlatTree({ nodes, depth, selected, onPick }: { nodes: TreeNode[]; depth: number; selected: string; onPick: (p: string) => void }) {
  return (
    <>
      {nodes.map((n) => (
        <div key={n.path}>
          <button
            onClick={() => n.kind === "file" && onPick(n.path)}
            style={{ paddingLeft: 8 + depth * 14 }}
            className={`flex w-full cursor-pointer justify-between px-2 py-0.5 text-left ${
              selected === n.path ? "bg-zinc-800 text-lime-300" : "hover:bg-zinc-900"
            }`}
          >
            <span>{n.kind === "dir" ? `${n.name}/` : n.name}</span>
            {n.lastChange && <span className="pr-2 text-zinc-600">{n.lastChange}</span>}
          </button>
          {n.children && <FlatTree nodes={n.children} depth={depth + 1} selected={selected} onPick={onPick} />}
        </div>
      ))}
    </>
  );
}

// ── proposals: list | stack+diff | threads ───────────────────────────────────

function ProposalPane() {
  const [pid, setPid] = useState(218);
  const p = proposalById(pid);
  const [sel, setSel] = useState(p.stack[0]);
  const [interdiff, setInterdiff] = useState(false);
  const c = changeById(sel);
  const diffs = (interdiff ? interdiffs[sel] : changeDiffs[sel]) ?? [];
  return (
    <div className="grid h-full grid-cols-[0.9fr_2.2fr_1fr] gap-2">
      <Pane title="proposals">
        {proposals.map((pr) => (
          <button
            key={pr.id}
            onClick={() => { setPid(pr.id); setSel(pr.stack[0]); setInterdiff(false); }}
            className={`block w-full cursor-pointer px-2 py-1 text-left ${pid === pr.id ? "bg-zinc-800" : "hover:bg-zinc-900"}`}
          >
            <p>
              <span className="text-sky-400">#{pr.id}</span>{" "}
              <span className={pr.status === "landed" ? "text-zinc-500 line-through" : "text-zinc-100"}>{pr.title}</span>
            </p>
            <p className="text-xs text-zinc-500">
              {pr.stack.length}ch → {pr.target} · @{pr.author}
              {pr.status === "landed" && " · landed"}
              {pr.trialRebase === "would-conflict" && <span className="text-amber-400"> · would conflict</span>}
              {pr.approvals.length > 0 && <span className="text-emerald-400"> · ✓{pr.approvals.length}</span>}
            </p>
          </button>
        ))}
      </Pane>

      <Pane title={`#${p.id} · ${interdiff ? `interdiff of ${sel}` : `diff of ${sel}`}`}>
        <div className="flex flex-wrap items-center gap-1 border-b border-zinc-800 bg-zinc-900/50 px-2 py-1 text-xs">
          {p.stack.map((cidStr, i) => (
            <button
              key={cidStr}
              onClick={() => { setSel(cidStr); setInterdiff(false); }}
              className={`cursor-pointer px-1.5 py-0.5 ${sel === cidStr ? "bg-fuchsia-500 font-bold text-zinc-950" : "text-fuchsia-400 hover:bg-zinc-800"}`}
            >
              {i + 1}/{p.stack.length} {cidStr}
            </button>
          ))}
          <span className="mx-2 text-zinc-600">|</span>
          {c.revisions.map((r) => (
            <span key={r.hash} className="text-zinc-500">{r.label}:{r.hash}</span>
          ))}
          {c.revisions.length > 1 && (
            <button
              onClick={() => setInterdiff(!interdiff)}
              className={`cursor-pointer px-1.5 py-0.5 ${interdiff ? "bg-sky-500 font-bold text-zinc-950" : "text-sky-400 hover:bg-zinc-800"}`}
            >
              [i]nterdiff
            </button>
          )}
        </div>
        {diffs.map((d) => (
          <DarkDiff key={d.path} diff={d} />
        ))}
        <div className="border-t border-zinc-800 px-2 py-2 text-xs">
          {p.trialRebase === "clean" ? (
            <p className="text-emerald-400">trial rebase onto {p.target}: clean — landing fast-forwards, {p.target} stays linear</p>
          ) : (
            <p className="text-amber-400">
              trial rebase onto {p.target}: CONFLICT in {p.conflictFiles?.join(", ")} — hub refuses to land; pull, resolve, amend
            </p>
          )}
          <div className="mt-1.5 flex gap-2">
            <button className="cursor-pointer border border-zinc-700 px-2 py-0.5 hover:bg-zinc-800">[a]pprove</button>
            <button
              disabled={p.trialRebase !== "clean" || p.status !== "open"}
              className="cursor-pointer border border-emerald-600 px-2 py-0.5 text-emerald-400 hover:bg-emerald-950 disabled:cursor-not-allowed disabled:border-zinc-800 disabled:text-zinc-700"
            >
              [L]and {p.stack.length} changes → {p.target}
            </button>
          </div>
        </div>
      </Pane>

      <Pane title={`threads · ${sel}`}>
        {p.threads.filter((t) => t.changeId === sel).map((t) => {
          const outdated = t.revision !== c.revisions.at(-1)!.hash;
          return (
            <div key={t.id} className="border-b border-zinc-800 px-2 py-1.5 text-xs">
              <p className="text-zinc-500">
                {t.path}:{t.line} @ {t.revision}
                {outdated && <span className="text-amber-400"> (outdated)</span>}
                {t.resolved ? <span className="text-emerald-400"> ✓resolved</span> : <span className="text-zinc-300"> open</span>}
              </p>
              {t.comments.map((cm, i) => (
                <p key={i} className="mt-1"><span className="text-sky-400">@{cm.author}</span> {cm.body}</p>
              ))}
            </div>
          );
        })}
        {p.threads.filter((t) => t.changeId === sel).length === 0 && (
          <p className="px-2 py-2 text-xs text-zinc-600">no threads on this change</p>
        )}
        <div className="px-2 py-2">
          {p.approvals.map((a) => (
            <p key={a.who} className="text-xs text-emerald-400">✓ @{a.who} approved @ {a.revision}</p>
          ))}
        </div>
      </Pane>
    </div>
  );
}

function DarkDiff({ diff }: { diff: FileDiff }) {
  const style = {
    add: "bg-emerald-950/60 text-emerald-300",
    del: "bg-rose-950/60 text-rose-300",
    ctx: "text-zinc-400",
    hunk: "text-sky-500",
  };
  return (
    <div className="mb-1">
      <p className="bg-zinc-900 px-2 py-0.5 text-xs text-zinc-400">{diff.path} ({diff.status})</p>
      <pre className="leading-5">
        {diff.lines.map((l, i) => (
          <div key={i} className={`flex px-2 ${style[l.kind]}`}>
            <span className="w-7 shrink-0 select-none text-right text-zinc-700">{l.new ?? l.old ?? ""}</span>
            <span className="w-4 shrink-0 pl-2">{l.kind === "add" ? "+" : l.kind === "del" ? "-" : " "}</span>
            <span>{l.text}</span>
          </div>
        ))}
      </pre>
    </div>
  );
}

// ── env: table | roster | audit tail ─────────────────────────────────────────

function EnvPane() {
  return (
    <div className="grid h-full grid-rows-[auto_1fr] gap-2">
      <p className="border border-amber-900/60 bg-amber-950/30 px-3 py-1.5 text-xs text-amber-300">
        ciphertext only — names, values, even secret counts are encrypted in the bundle. this hub relays; it cannot read.
        edit with: <b>graft env</b>
      </p>
      <div className="grid min-h-0 grid-cols-[1.4fr_1fr_1.2fr] gap-2">
        <Pane title="environments">
          <table className="w-full">
            <thead>
              <tr className="text-left text-xs text-zinc-500">
                <th className="px-2 py-1">env</th>
                <th className="px-2 py-1">gen</th>
                <th className="px-2 py-1">members</th>
                <th className="px-2 py-1">bundle</th>
                <th className="px-2 py-1">contents</th>
              </tr>
            </thead>
            <tbody>
              {envs.map((e) => (
                <tr key={e.name} className="border-t border-zinc-800">
                  <td className="px-2 py-1.5 font-bold text-lime-300">{e.name}</td>
                  <td className="px-2 py-1.5">gen {e.generation}</td>
                  <td className="px-2 py-1.5">{e.members}</td>
                  <td className="px-2 py-1.5 text-zinc-500">{e.bundleUpdated} ago · @{e.bundleUpdatedBy}</td>
                  <td className="px-2 py-1.5 text-zinc-600">████████ opaque</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="px-2 py-2 text-xs text-zinc-600">
            staging rotated 2h ago (gen 2 → 3): @devon removed, key re-wrapped for 5 remaining members. removed members keep
            what they already synced — rotate real credentials if this was a trust boundary.
          </p>
        </Pane>
        <Pane title="keyring roster">
          {members.map((m) => (
            <div key={m.handle} className="border-b border-zinc-800 px-2 py-1.5">
              <p><span className="text-sky-400">@{m.handle}</span> <span className="text-zinc-600">{m.fingerprint}</span></p>
              <p className="text-xs text-zinc-500">{m.envs.join(" ")}</p>
            </div>
          ))}
          <p className="px-2 py-2 text-xs text-zinc-600">verify fingerprints out-of-band; the hub is not a trust root</p>
        </Pane>
        <Pane title="keyring log (signed)">
          {keyringLog.map((e, i) => (
            <p key={i} className="border-b border-zinc-800/60 px-2 py-1.5 text-xs">
              <span className="text-zinc-600">[{e.when}]</span> {e.what} <span className="text-zinc-500">— @{e.signedBy}</span>
            </p>
          ))}
        </Pane>
      </div>
    </div>
  );
}
