import { FormEvent, useEffect, useState } from "react";
import "./styles.css";

type Tab = "files" | "log" | "proposals" | "issues" | "environments" | "account";
type Browse = {
  repo: string;
  public: boolean;
  refs: { name: string; target: string }[];
  files: { path: string; kind: string; id: string }[];
  log: { revision: string; change_id: string; message: string; parents: string[] }[];
  environments: { name: string; opaque: boolean }[];
  profiles: { username: string; member_id: string; age_recipient: string; signing_public: string }[];
};
type Proposal = { id: number; title: string; status: string; source: string; target: string; source_head: string; updated_at: number };
type ProposalDetail = {
  proposal: Proposal;
  target_current: string | null;
  target_stale: boolean;
  changes: { position: number; change_id: string; revision: string }[];
  comments: { id: number; author_id: number; body: string; created_at: number }[];
  threads: { id: number; revision: string; path: string; line: number | null; body: string; resolved: boolean; outdated: boolean }[];
  approvals: { user_id: number; revision: string | null }[];
  landable: boolean;
};
type Issue = { number: number; title: string; state: string; author_id: number };
type IssueDetail = { issue: Issue & { body: string }; comments: { id: number; author_id: number; body: string }[] };
type TokenRecord = { id: number; name: string; mode: string; revoked: boolean };
type RepoRecord = { name: string; owner: string; public: boolean };

const cx = (...values: (string | false | undefined)[]) => values.filter(Boolean).join(" ");
const short = (value: string, length = 8) => value.slice(0, length);
const date = (seconds: number) => new Date(seconds * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });

async function api<T>(path: string, token: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set("Accept", "application/json");
  if (init?.body) headers.set("Content-Type", "application/json");
  if (token) headers.set("Authorization", "Bearer " + token);
  const response = await fetch(path, { ...init, headers });
  const value = (await response.json().catch(() => ({}))) as { error?: string };
  if (!response.ok) throw new Error(value.error ?? "Request failed (" + response.status + ")");
  return value as T;
}

function App() {
  const params = new URLSearchParams(window.location.search);
  const [repo, setRepo] = useState(params.get("repo") ?? "demo");
  const [tab, setTab] = useState<Tab>("files");
  const [token, setToken] = useState(() => window.localStorage.getItem("graft-token") ?? "");
  const [browse, setBrowse] = useState<Browse | null>(null);
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [proposalId, setProposalId] = useState<number | null>(null);
  const [proposal, setProposal] = useState<ProposalDetail | null>(null);
  const [issues, setIssues] = useState<Issue[]>([]);
  const [issueNumber, setIssueNumber] = useState<number | null>(null);
  const [issue, setIssue] = useState<IssueDetail | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [refresh, setRefresh] = useState(0);

  useEffect(() => {
    if (!["files", "log", "environments"].includes(tab)) return;
    setLoading(true);
    setError("");
    void api<Browse>("/api/repos/" + repo + "/browse", token)
      .then(setBrowse)
      .catch((reason: unknown) => setError(reason instanceof Error ? reason.message : "Unable to load repository"))
      .finally(() => setLoading(false));
  }, [repo, tab, token, refresh]);

  useEffect(() => {
    if (tab !== "proposals") return;
    setLoading(true);
    void api<Proposal[]>("/api/repos/" + repo + "/proposals", token)
      .then((value) => {
        setProposals(value);
        setProposalId((current) => current && value.some((item) => item.id === current) ? current : value[0]?.id ?? null);
      })
      .catch((reason: unknown) => setError(reason instanceof Error ? reason.message : "Unable to load proposals"))
      .finally(() => setLoading(false));
  }, [repo, tab, token, refresh]);

  useEffect(() => {
    if (tab !== "proposals" || proposalId === null) {
      setProposal(null);
      return;
    }
    void api<ProposalDetail>("/api/repos/" + repo + "/proposals/" + proposalId, token)
      .then(setProposal)
      .catch((reason: unknown) => setError(reason instanceof Error ? reason.message : "Unable to load proposal"));
  }, [repo, tab, token, proposalId, refresh]);

  useEffect(() => {
    if (tab !== "issues") return;
    setLoading(true);
    void api<Issue[]>("/api/repos/" + repo + "/issues", token)
      .then((value) => {
        setIssues(value);
        setIssueNumber((current) => current && value.some((item) => item.number === current) ? current : value[0]?.number ?? null);
      })
      .catch((reason: unknown) => setError(reason instanceof Error ? reason.message : "Unable to load issues"))
      .finally(() => setLoading(false));
  }, [repo, tab, token, refresh]);

  useEffect(() => {
    if (tab !== "issues" || issueNumber === null) {
      setIssue(null);
      return;
    }
    void api<IssueDetail>("/api/repos/" + repo + "/issues/" + issueNumber, token)
      .then(setIssue)
      .catch((reason: unknown) => setError(reason instanceof Error ? reason.message : "Unable to load issue"));
  }, [repo, tab, token, issueNumber, refresh]);

  const changed = () => setRefresh((value) => value + 1);
  const saveToken = (value: string) => {
    setToken(value);
    if (value) window.localStorage.setItem("graft-token", value);
    else window.localStorage.removeItem("graft-token");
  };

  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="wordmark" href="/"><span className="wordmark-mark">G</span><span>graft</span></a>
        <div className="repo-path"><span className="muted">hub</span><span className="slash">/</span><input aria-label="Repository" value={repo} onChange={(event) => setRepo(event.target.value)} /><span className={cx("visibility", browse?.public && "public")}>{browse?.public ? "public" : "private"}</span></div>
        <div className="topbar-actions"><button className="ghost-button" onClick={() => setTab("account")}>Account</button><a className="icon-button" href="/settings/tokens/new">↗</a></div>
      </header>
      <div className="layout">
        <aside className="sidebar">
          <div className="sidebar-label">Repository</div>
          <nav className="nav-stack">
            <NavButton active={tab === "files"} onClick={() => setTab("files")} icon="⌘">Files</NavButton>
            <NavButton active={tab === "log"} onClick={() => setTab("log")} icon="◌">Log</NavButton>
            <NavButton active={tab === "proposals"} onClick={() => setTab("proposals")} icon="✦" count={proposals.length || undefined}>Proposals</NavButton>
            <NavButton active={tab === "issues"} onClick={() => setTab("issues")} icon="□" count={issues.filter((item) => item.state === "open").length || undefined}>Issues</NavButton>
            <NavButton active={tab === "environments"} onClick={() => setTab("environments")} icon="◇">Environments</NavButton>
          </nav>
          <div className="sidebar-spacer" />
          <div className="sidebar-card"><div className="eyebrow">working model</div><p>Changes are stable. Revisions are immutable.</p><div className="legend"><span className="change-chip">letters</span><span className="revision-chip">hex</span></div></div>
          <button className="account-link" onClick={() => setTab("account")}><span className="avatar">D</span><span><strong>{token ? "Authenticated" : "Public view"}</strong><small>{token ? "PAT loaded locally" : "Add a PAT to review"}</small></span></button>
        </aside>
        <main className="main-content">
          <div className="content-header"><div><div className="eyebrow">graft hub</div><h1>{label(tab)}</h1></div><div className="header-actions">{tab === "issues" && token && <NewIssue repo={repo} token={token} onCreated={changed} />}{["files", "log", "environments"].includes(tab) && <button className="ghost-button" onClick={changed}>Refresh</button>}</div></div>
          {error && <div className="notice error"><span>!</span><span>{error}</span><button onClick={() => setError("")}>Dismiss</button></div>}
          {loading && <div className="loading-line"><span /> loading repository state</div>}
          {!loading && tab === "files" && <FilesView browse={browse} />}
          {!loading && tab === "log" && <LogView browse={browse} />}
          {!loading && tab === "environments" && <EnvironmentView browse={browse} />}
          {!loading && tab === "proposals" && <ProposalView repo={repo} token={token} list={proposals} selected={proposalId} detail={proposal} onSelect={setProposalId} onChanged={changed} />}
          {!loading && tab === "issues" && <IssueView repo={repo} token={token} list={issues} selected={issueNumber} detail={issue} onSelect={setIssueNumber} onChanged={changed} />}
          {!loading && tab === "account" && <AccountView token={token} onToken={saveToken} repo={repo} onRepo={setRepo} />}
        </main>
      </div>
    </div>
  );
}

function NavButton({ active, onClick, icon, count, children }: { active: boolean; onClick: () => void; icon: string; count?: number; children: string }) {
  return <button className={cx("nav-button", active && "active")} onClick={onClick}><span className="nav-icon">{icon}</span><span>{children}</span>{count !== undefined && <span className="nav-count">{count}</span>}</button>;
}
function label(tab: Tab): string {
  return { files: "Repository files", log: "Change log", proposals: "Proposals", issues: "Issues", environments: "Environments", account: "Account" }[tab];
}
function Heading({ eyebrow, title, suffix }: { eyebrow: string; title: string; suffix?: string }) {
  return <div className="panel-heading"><div><span className="eyebrow">{eyebrow}</span><h2>{title}</h2></div>{suffix && <span className="neutral-pill">{suffix}</span>}</div>;
}
function Status({ status }: { status: string }) {
  return <span className={cx("status-pill", status)}>{status}</span>;
}
function Empty({ title, body, compact = false }: { title: string; body: string; compact?: boolean }) {
  return <div className={cx("empty-state", compact && "compact")}><span className="empty-mark">·</span><h3>{title}</h3><p>{body}</p></div>;
}

function FilesView({ browse }: { browse: Browse | null }) {
  if (!browse) return <Empty title="No repository loaded" body="Enter a repository name to browse it." />;
  return <div className="view-grid"><section className="panel"><Heading eyebrow="main" title="Working tree" suffix={browse.files.length + " paths"} />{browse.files.length === 0 ? <Empty compact title="Empty repository" body="Push a snapshot from the Graft CLI to see files here." /> : <div className="file-list">{browse.files.map((file) => <div className="file-row" key={file.path}><span className="file-symbol">{file.kind === "Executable" ? "◆" : "·"}</span><span className="file-name">{file.path}</span><span className="file-kind">{file.kind.toLowerCase()}</span><code>{short(file.id)}</code></div>)}</div>}</section><section className="panel"><Heading eyebrow="view" title="Refs" />{browse.refs.map((ref) => <div className="ref-row" key={ref.name}><span>{ref.name}</span><code>{short(ref.target, 10)}</code></div>)}{browse.profiles.length > 0 && <><div className="profile-heading"><span className="eyebrow">published identities</span></div><div className="profile-list">{browse.profiles.map((profile) => <div className="profile-row" key={profile.username}><span className="avatar small">{profile.username.slice(0, 1).toUpperCase()}</span><span><strong>{profile.username}</strong><small>{short(profile.member_id, 12)} · discovery only</small></span></div>)}</div></>}</section></div>;
}
function LogView({ browse }: { browse: Browse | null }) {
  if (!browse || browse.log.length === 0) return <Empty title="No Changes yet" body="A snapped Change will appear here with its immutable Revision." />;
  return <section className="panel log-panel"><Heading eyebrow="branches / main" title="History" suffix={browse.log.length + " revisions"} /><div className="timeline">{browse.log.map((revision, index) => <article className="timeline-row" key={revision.revision}><div className="timeline-line"><span className={cx("timeline-dot", index === 0 && "current")} />{index !== browse.log.length - 1 && <span className="timeline-stem" />}</div><div className="revision-card"><div className="revision-topline"><span className="change-chip">{revision.change_id}</span><span className="revision-label">Revision {short(revision.revision)}</span><span className="date-label">{index === 0 ? "tip" : "ancestor"}</span></div><h3>{revision.message || "Untitled Change"}</h3><div className="revision-meta"><code>{revision.revision}</code><span>{revision.parents.length} parent{revision.parents.length === 1 ? "" : "s"}</span></div></div></article>)}</div></section>;
}
function EnvironmentView({ browse }: { browse: Browse | null }) {
  const environments = browse?.environments ?? [];
  return <section className="panel environment-panel"><div className="lock-banner"><span className="lock-icon">◇</span><div><strong>Secrets stay opaque to the Hub</strong><p>Environment mutation and decryption are CLI-only. This screen shows keyring metadata, never names or values.</p></div></div>{environments.length === 0 ? <Empty compact title="No environment refs" body="Initialize an environment locally with graft env init." /> : <div className="environment-list">{environments.map((environment) => <div className="environment-row" key={environment.name}><span className="env-lock">◇</span><div><strong>{environment.name}</strong><small>opaque ciphertext bundle</small></div><span className="generation-pill">keyring ref</span></div>)}</div>}</section>;
}

function ProposalView({ repo, token, list, selected, detail, onSelect, onChanged }: { repo: string; token: string; list: Proposal[]; selected: number | null; detail: ProposalDetail | null; onSelect: (id: number) => void; onChanged: () => void }) {
  return <div className="review-layout"><section className="panel proposal-list"><Heading eyebrow="review queue" title="Open proposals" suffix={String(list.length)} />{list.length === 0 ? <Empty compact title="Nothing to review" body="Open a proposal with graft propose, then push it to this Hub." /> : list.map((item) => <button className={cx("proposal-list-item", selected === item.id && "selected")} key={item.id} onClick={() => onSelect(item.id)}><div className="proposal-item-top"><span className="proposal-number">#{item.id}</span><Status status={item.status} /></div><strong>{item.title}</strong><small>{item.source} → {item.target}</small><span className="proposal-item-date">{date(item.updated_at)}</span></button>)}</section><section className="proposal-detail">{detail ? <ProposalDetailView repo={repo} token={token} detail={detail} onChanged={onChanged} /> : <Empty title="Select a proposal" body="The stack, conversation, and land verdict will appear here." />}</section></div>;
}
function ProposalDetailView({ repo, token, detail, onChanged }: { repo: string; token: string; detail: ProposalDetail; onChanged: () => void }) {
  const [comment, setComment] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!comment.trim()) return;
    setBusy(true);
    try {
      await api("/api/repos/" + repo + "/proposals/" + detail.proposal.id + "/comments", token, { method: "POST", body: JSON.stringify({ body: comment }) });
      setComment("");
      onChanged();
    } finally {
      setBusy(false);
    }
  };
  const land = async () => {
    setBusy(true);
    try {
      await api("/api/repos/" + repo + "/proposals/" + detail.proposal.id + "/land", token, { method: "POST" });
      onChanged();
    } finally {
      setBusy(false);
    }
  };
  return <div className="detail-stack"><section className="panel proposal-hero"><div className="proposal-hero-top"><div><span className="eyebrow">proposal #{detail.proposal.id}</span><h2>{detail.proposal.title}</h2><p className="subtle">{detail.proposal.source} → {detail.proposal.target}</p></div><Status status={detail.proposal.status} /></div><div className="stack-strip">{detail.changes.map((change, index) => <div className="stack-change" key={change.revision}><span className="stack-index">{index + 1}</span><span className="change-chip">{change.change_id}</span><code>{short(change.revision, 10)}</code></div>)}</div></section><div className="review-columns"><section className="panel conversation"><Heading eyebrow="conversation" title="Review discussion" />{detail.comments.map((item) => <div className="comment" key={item.id}><span className="avatar small">U</span><div><div className="comment-meta"><strong>reviewer {item.author_id}</strong><span>{date(item.created_at)}</span></div><p>{item.body}</p></div></div>)}{detail.threads.map((thread) => <div className={cx("thread", thread.outdated && "outdated")} key={thread.id}><div className="thread-top"><span className="thread-anchor">{thread.path}:{thread.line ?? "—"}</span><span className="revision-label">Revision {short(thread.revision)}</span>{thread.outdated && <span className="warning-pill">outdated</span>}</div><p>{thread.body}</p><div className="thread-bottom">{thread.resolved ? "Resolved" : "Open"} · anchored to immutable hex</div></div>)}{detail.comments.length === 0 && detail.threads.length === 0 && <Empty compact title="Start the review" body="Leave a top-level comment or anchor a thread to a Revision." />}{token && <form className="comment-form" onSubmit={(event) => void submit(event)}><textarea value={comment} onChange={(event) => setComment(event.target.value)} placeholder="Leave a top-level comment…" rows={3} /><button className="primary-button" disabled={busy || !comment.trim()}>Comment</button></form>}</section><aside className="review-sidebar"><section className="panel land-panel"><Heading eyebrow="land verdict" title={detail.target_stale ? "Target moved" : "Ready to trial"} suffix={detail.landable ? "✓" : "!"} /><ul className="checklist"><li className={detail.target_stale ? "bad" : "good"}><span>{detail.target_stale ? "×" : "✓"}</span> target ref is unchanged</li><li className="good"><span>✓</span> stack order is preserved</li><li className="good"><span>✓</span> trial rebase runs on the Hub</li></ul><p className="land-copy">The Hub rebases the whole stack and refuses a conflicted result. Main moves once, after the trial succeeds.</p>{token && <button className="primary-button wide" disabled={!detail.landable || busy} onClick={() => void land()}>Land stack</button>}</section><section className="panel approval-panel"><Heading eyebrow="reviewers" title="Approvals" suffix={String(detail.approvals.length)} />{detail.approvals.length === 0 ? <p className="empty-note">No approvals yet.</p> : detail.approvals.map((approval) => <div className="approval-row" key={approval.user_id}><span className="avatar small">A</span><span>reviewer {approval.user_id}</span><span className="approved">approved</span></div>)}</section></aside></div></div>;
}

function IssueView({ repo, token, list, selected, detail, onSelect, onChanged }: { repo: string; token: string; list: Issue[]; selected: number | null; detail: IssueDetail | null; onSelect: (number: number) => void; onChanged: () => void }) {
  return <div className="review-layout"><section className="panel proposal-list"><Heading eyebrow="tracker" title="Issues" suffix={String(list.length)} />{list.length === 0 ? <Empty compact title="No issues" body="Create the first numbered issue for this repository." /> : list.map((item) => <button className={cx("proposal-list-item", selected === item.number && "selected")} key={item.number} onClick={() => onSelect(item.number)}><div className="proposal-item-top"><span className="proposal-number">#{item.number}</span><Status status={item.state} /></div><strong>{item.title}</strong><small>opened by reviewer {item.author_id}</small></button>)}</section><section className="proposal-detail">{detail ? <IssueDetailView repo={repo} token={token} detail={detail} onChanged={onChanged} /> : <Empty title="Select an issue" body="Issue body and flat comments appear here." />}</section></div>;
}
function IssueDetailView({ repo, token, detail, onChanged }: { repo: string; token: string; detail: IssueDetail; onChanged: () => void }) {
  const [comment, setComment] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!comment.trim()) return;
    await api("/api/repos/" + repo + "/issues/" + detail.issue.number + "/comments", token, { method: "POST", body: JSON.stringify({ body: comment }) });
    setComment("");
    onChanged();
  };
  return <section className="panel issue-detail"><div className="proposal-hero-top"><div><span className="eyebrow">issue #{detail.issue.number}</span><h2>{detail.issue.title}</h2></div><Status status={detail.issue.state} /></div><p className="issue-body">{detail.issue.body}</p>{detail.comments.map((item) => <div className="comment" key={item.id}><span className="avatar small">U</span><div><div className="comment-meta"><strong>reviewer {item.author_id}</strong></div><p>{item.body}</p></div></div>)}{token && <form className="comment-form" onSubmit={(event) => void submit(event)}><textarea value={comment} onChange={(event) => setComment(event.target.value)} placeholder="Add a comment…" rows={3} /><button className="primary-button" disabled={!comment.trim()}>Comment</button></form>}</section>;
}
function NewIssue({ repo, token, onCreated }: { repo: string; token: string; onCreated: () => void }) {
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    await api("/api/repos/" + repo + "/issues", token, { method: "POST", body: JSON.stringify({ title, body }) });
    setTitle("");
    setBody("");
    setOpen(false);
    onCreated();
  };
  if (!open) return <button className="primary-button" onClick={() => setOpen(true)}>New issue</button>;
  return <form className="inline-create" onSubmit={(event) => void submit(event)}><input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Issue title" /><input value={body} onChange={(event) => setBody(event.target.value)} placeholder="Short description" /><button className="primary-button">Create</button><button type="button" className="ghost-button" onClick={() => setOpen(false)}>Cancel</button></form>;
}
function AccountView({ token, onToken, repo, onRepo }: { token: string; onToken: (value: string) => void; repo: string; onRepo: (value: string) => void }) {
  const [value, setValue] = useState(token);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [tokenName, setTokenName] = useState("browser");
  const [tokenMode, setTokenMode] = useState("rw");
  const [newToken, setNewToken] = useState("");
  const [activeTokenId, setActiveTokenId] = useState<number | null>(null);
  const [tokens, setTokens] = useState<TokenRecord[]>([]);
  const [repos, setRepos] = useState<RepoRecord[]>([]);
  const [repoName, setRepoName] = useState("");
  const [isPublic, setIsPublic] = useState(false);
  const [collaborator, setCollaborator] = useState("");
  const [collaboratorMode, setCollaboratorMode] = useState("rw");
  const [sessionActive, setSessionActive] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const authenticated = Boolean(token) || sessionActive;

  const loadAdministration = async () => {
    if (!authenticated) {
      setTokens([]);
      setRepos([]);
      return;
    }
    try {
      const [tokenRows, repoRows] = await Promise.all([
        api<TokenRecord[]>("/api/tokens", token),
        api<RepoRecord[]>("/api/repos", token),
      ]);
      setTokens(tokenRows);
      setRepos(repoRows);
    } catch (reason: unknown) {
      setError(reason instanceof Error ? reason.message : "Unable to load account data");
    }
  };

  useEffect(() => { void loadAdministration(); }, [token, sessionActive]);

  const action = async (work: () => Promise<void>) => {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await work();
    } catch (reason: unknown) {
      setError(reason instanceof Error ? reason.message : "Request failed");
    } finally {
      setBusy(false);
    }
  };
  const saveBrowserToken = (next: string) => {
    setSessionActive(false);
    setValue(next);
    onToken(next);
  };
  const register = () => {
    void action(async () => {
      await api("/api/register", "", { method: "POST", body: JSON.stringify({ username, password }) });
      setMessage("Account registered. Create a PAT below to connect the browser.");
    });
  };
  const login = () => {
    void action(async () => {
      await api("/api/session", "", { method: "POST", body: JSON.stringify({ username, password }) });
      saveBrowserToken("");
      setSessionActive(true);
      setMessage("Signed in with a secure browser session.");
    });
  };
  const createToken = (event: FormEvent) => {
    event.preventDefault();
    void action(async () => {
      const created = await api<{ id: number; token: string }>("/api/tokens", token, { method: "POST", body: JSON.stringify({ username, password, name: tokenName, mode: tokenMode }) });
      setNewToken(created.token);
      setActiveTokenId(created.id);
      saveBrowserToken(created.token);
      setMessage("PAT created. Copy it now; the Hub will not show it again.");
      setPassword("");
      await loadAdministration();
    });
  };
  const revokeToken = (id: number) => {
    void action(async () => {
      await api(`/api/tokens/${id}/revoke`, token, { method: "POST" });
      if (id === activeTokenId) {
        setNewToken("");
        setActiveTokenId(null);
        saveBrowserToken("");
      }
      await loadAdministration();
    });
  };
  const createRepo = (event: FormEvent) => {
    event.preventDefault();
    void action(async () => {
      await api("/api/repos", token, { method: "POST", body: JSON.stringify({ name: repoName, public: isPublic }) });
      onRepo(repoName);
      setRepoName("");
      setMessage("Repository created.");
      await loadAdministration();
    });
  };
  const addCollaborator = (event: FormEvent) => {
    event.preventDefault();
    void action(async () => {
      await api(`/api/repos/${encodeURIComponent(repo)}/collaborators`, token, { method: "POST", body: JSON.stringify({ username: collaborator, mode: collaboratorMode }) });
      setCollaborator("");
      setMessage(`Collaborator added to ${repo}.`);
    });
  };
  const logout = () => {
    void action(async () => {
      await api("/api/session", "", { method: "DELETE" });
      setSessionActive(false);
      setMessage("Browser session ended.");
      await loadAdministration();
    });
  };

  return <div className="account-grid">
    <section className="panel account-panel">
      <span className="eyebrow">account access</span>
      <h2>Register or sign in</h2>
      <p className="subtle">Sessions use an HttpOnly cookie. PATs remain the CLI’s portable bootstrap credential.</p>
      <form className="admin-form" onSubmit={(event) => { event.preventDefault(); login(); }}>
        <input aria-label="Username" value={username} onChange={(event) => setUsername(event.target.value)} placeholder="Username" autoComplete="username" />
        <input aria-label="Password" type="password" value={password} onChange={(event) => setPassword(event.target.value)} placeholder="Password" autoComplete="current-password" />
        <div className="account-actions"><button className="primary-button" disabled={busy || !username || !password}>Sign in</button><button className="ghost-button" type="button" disabled={busy || !username || !password} onClick={register}>Register</button>{sessionActive && <button className="ghost-button" type="button" disabled={busy} onClick={logout}>Sign out</button>}</div>
      </form>
      {message && <p className="account-message">{message}</p>}
      {error && <p className="account-error">{error}</p>}
    </section>
    <section className="panel account-panel">
      <span className="eyebrow">personal access tokens</span>
      <h2>Create and revoke</h2>
      <form className="admin-form" onSubmit={createToken}>
        <input aria-label="Token name" value={tokenName} onChange={(event) => setTokenName(event.target.value)} placeholder="Token name" />
        <select aria-label="Token mode" value={tokenMode} onChange={(event) => setTokenMode(event.target.value)}><option value="rw">read / write</option><option value="ro">read only</option></select>
        <button className="primary-button" disabled={busy || !username || !password}>Create PAT</button>
      </form>
      {newToken && <div className="token-reveal"><span>one-time token</span><code>{newToken}</code></div>}
      <div className="admin-list">{tokens.map((item) => <div className="admin-row" key={item.id}><span><strong>{item.name}</strong><small>{item.mode} · {item.revoked ? "revoked" : "active"}</small></span>{!item.revoked && <button className="ghost-button" disabled={busy} onClick={() => revokeToken(item.id)}>Revoke</button>}</div>)}{authenticated && tokens.length === 0 && <p className="empty-note">No named PATs yet.</p>}</div>
    </section>
    <section className="panel account-panel">
      <span className="eyebrow">repository administration</span>
      <h2>Create a repository</h2>
      <form className="admin-form" onSubmit={createRepo}><input aria-label="New repository name" value={repoName} onChange={(event) => setRepoName(event.target.value)} placeholder="Repository name" /><label className="check-label"><input type="checkbox" checked={isPublic} onChange={(event) => setIsPublic(event.target.checked)} /> Public repository</label><button className="primary-button" disabled={busy || !repoName || !authenticated}>Create repository</button></form>
      <div className="admin-list">{repos.map((item) => <button className="admin-row repo-admin-row" key={item.name} onClick={() => onRepo(item.name)}><span><strong>{item.name}</strong><small>{item.owner} · {item.public ? "public" : "private"}</small></span><span className="neutral-pill">open</span></button>)}</div>
    </section>
    <section className="panel account-panel">
      <span className="eyebrow">access control</span>
      <h2>Manage collaborators</h2>
      <p className="subtle">Selected repository: <code>{repo}</code></p>
      <form className="admin-form" onSubmit={addCollaborator}><input aria-label="Collaborator username" value={collaborator} onChange={(event) => setCollaborator(event.target.value)} placeholder="Username" /><select aria-label="Collaborator mode" value={collaboratorMode} onChange={(event) => setCollaboratorMode(event.target.value)}><option value="rw">read / write</option><option value="ro">read only</option></select><button className="primary-button" disabled={busy || !collaborator || !authenticated}>Add collaborator</button></form>
    </section>
    <section className="panel account-panel accent-panel">
      <span className="eyebrow">cli-first identity</span>
      <h2>Secrets stay local</h2>
      <p>Run <code>graft auth login &lt;hub-url&gt;</code> to paste a named PAT and publish the discovery-only Member Identity. Environment mutation never happens here.</p>
      <a className="text-link" href="/settings/tokens/new">Open token settings ↗</a>
    </section>
  </div>;
}

export default App;
