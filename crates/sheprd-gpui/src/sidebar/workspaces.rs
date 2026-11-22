//! Which workspaces the tree shows and how each one is named. A repository and
//! its linked worktrees form one group, folded or unfolded locally: collapsing
//! changes this client's view only, never the daemon's state.
//!
//! Grouping follows the TUI: every checkout that is not a linked worktree is a
//! parent, never another parent's child, and a repository only forms a group
//! once it has both a parent and at least one linked worktree open. Membership
//! comes from the daemon alone, so a checkout the daemon restored while it was
//! unavailable keeps its group.

use super::{
    RowBadge,
    agents::status_priority,
    row::{Counts, PrBadge, first_text},
};
use crate::config::{ChangeCounts, Theme};
use sheprd_client::protocol::AgentStatus;
use sheprd_session::Workspace;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug)]
struct FolderGroup {
    head: usize,
    children: Vec<usize>,
    key: String,
}

#[derive(Clone)]
enum FolderRule {
    All,
    Git,
    Prefix(String),
}

fn expand_group_root(root: &str) -> Option<String> {
    let root = root.trim().trim_end_matches(['/', '\\']);
    let expanded = if root == "~" || root.starts_with("~/") {
        let home = sheprd_client::local_path::home_dir()?;
        if root == "~" {
            home
        } else {
            home.join(&root[2..])
        }
    } else {
        Path::new(root).to_path_buf()
    };
    Some(
        expanded
            .to_string_lossy()
            .trim_end_matches(['/', '\\'])
            .to_owned(),
    )
}

fn folder_group_key(root: &str) -> String {
    format!("folder:{root}")
}

fn trim_group_path(path: &str) -> &str {
    let trimmed = path.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() && path.starts_with('/') {
        "/"
    } else {
        trimmed
    }
}

/// Open workspaces directly beneath configured folders, grouped beneath the
/// workspace opened on each folder itself. The folder workspace is the
/// selectable, foldable head; the children are ordinary workspaces and keep
/// their own daemon ids. A linked worktree is never a folder child: it stays
/// under its own repository's row, inside the hub or out of it.
fn folder_groups(workspaces: &[Workspace], roots: &[String]) -> Vec<FolderGroup> {
    let grouped = grouped_keys(workspaces);
    // A checkout some repository's row claims as its linked worktree.
    let repository_linked = |workspace: &Workspace| {
        workspace
            .worktree
            .as_ref()
            .is_some_and(|tree| tree.linked && grouped.contains(tree.key.as_str()))
    };
    let mut candidates = HashMap::<String, (usize, FolderRule)>::new();
    for root in roots.iter().filter_map(|root| expand_group_root(root)) {
        if let Some(head) = workspaces
            .iter()
            .position(|workspace| trim_group_path(&workspace.cwd) == root)
        {
            candidates.insert(root, (head, FolderRule::All));
        }
    }
    for (head, parent) in workspaces.iter().enumerate() {
        let root = trim_group_path(&parent.cwd);
        let children: Vec<_> = workspaces
            .iter()
            .filter(|workspace| {
                !repository_linked(workspace)
                    && Path::new(&workspace.cwd)
                        .parent()
                        .is_some_and(|path| trim_group_path(&path.to_string_lossy()) == root)
            })
            .collect();
        // Herdr attaches worktree metadata only once it made a worktree, so
        // a repository opened as a plain folder has none to count here. The
        // rule reads the daemon's metadata, never a scan of the user's disk.
        let git_children = children
            .iter()
            .filter(|workspace| workspace.worktree.is_some())
            .count();
        let prefix = Path::new(root)
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("{name}_"));
        let prefixed_children = prefix.as_deref().is_some_and(|prefix| {
            children
                .iter()
                .filter(|workspace| {
                    Path::new(&workspace.cwd)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with(prefix))
                })
                .count()
                >= 2
        });
        if git_children >= 2 || prefixed_children {
            let rule = prefix
                .filter(|_| prefixed_children)
                .map_or(FolderRule::Git, FolderRule::Prefix);
            candidates.entry(root.to_owned()).or_insert((head, rule));
        }
    }
    let mut ordered_roots: Vec<(String, usize, FolderRule)> = candidates
        .into_iter()
        .map(|(root, (head, rule))| (root, head, rule))
        .collect();
    ordered_roots.sort_by_key(|(root, _, _)| std::cmp::Reverse(root.len()));
    let mut claimed = HashSet::new();
    let mut groups = Vec::new();
    for (root, head, rule) in ordered_roots {
        if claimed.contains(&root) {
            continue;
        }
        let children: Vec<_> = workspaces
            .iter()
            .enumerate()
            .filter(|(index, workspace)| {
                *index != head
                    && !claimed.contains(trim_group_path(&workspace.cwd))
                    && !repository_linked(workspace)
                    && Path::new(&workspace.cwd)
                        .parent()
                        .is_some_and(|parent| trim_group_path(&parent.to_string_lossy()) == root)
                    && match &rule {
                        FolderRule::All => true,
                        FolderRule::Git => workspace.worktree.is_some(),
                        FolderRule::Prefix(prefix) => Path::new(&workspace.cwd)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with(prefix)),
                    }
            })
            .map(|(index, _)| index)
            .collect();
        if children.is_empty() {
            continue;
        }
        claimed.insert(root.clone());
        for &child in &children {
            claimed.insert(trim_group_path(&workspaces[child].cwd).to_owned());
        }
        groups.push(FolderGroup {
            head,
            children,
            key: folder_group_key(&root),
        });
    }
    groups
}

/// The repositories that form a group: those with at least one parent and at
/// least one linked worktree among `workspaces`.
pub(super) fn grouped_keys(workspaces: &[Workspace]) -> HashSet<&str> {
    let mut kinds = HashMap::<&str, (bool, bool)>::new();
    for tree in workspaces.iter().filter_map(|w| w.worktree.as_ref()) {
        let (parent, linked) = kinds.entry(&tree.key).or_default();
        *parent |= !tree.linked;
        *linked |= tree.linked;
    }
    kinds
        .into_iter()
        .filter(|&(_, (parent, linked))| parent && linked)
        .map(|(key, _)| key)
        .collect()
}

/// Display order as `(index, child)`. A group shows where its earliest member
/// sits: all its parents first, in daemon order, then its linked worktrees.
pub(crate) fn workspace_entries(workspaces: &[Workspace]) -> Vec<(usize, bool)> {
    let grouped = grouped_keys(workspaces);
    let mut members = HashMap::<&str, (Vec<usize>, Vec<usize>)>::new();
    for (index, workspace) in workspaces.iter().enumerate() {
        if let Some(tree) = workspace
            .worktree
            .as_ref()
            .filter(|tree| grouped.contains(tree.key.as_str()))
        {
            let (parents, children) = members.entry(&tree.key).or_default();
            if tree.linked {
                children.push(index);
            } else {
                parents.push(index);
            }
        }
    }
    let mut entries = Vec::with_capacity(workspaces.len());
    for (index, workspace) in workspaces.iter().enumerate() {
        let Some(tree) = workspace.worktree.as_ref() else {
            entries.push((index, false));
            continue;
        };
        if !grouped.contains(tree.key.as_str()) {
            entries.push((index, false));
            continue;
        }
        // Emitted once, at the group's earliest member; later members skip.
        if let Some((parents, children)) = members.remove(tree.key.as_str()) {
            entries.extend(parents.into_iter().map(|i| (i, false)));
            entries.extend(children.into_iter().map(|i| (i, true)));
        }
    }
    entries
}

/// Visible rows as `(index, child, group)`: `group` names the repository a
/// parent row folds, on every parent of a group. A folder hub's child keeps
/// its own repository group, and the group's linked worktrees follow it under
/// the hub, hidden when the hub or their repository is folded.
/// Each parent's linked worktrees, in the order `workspace_entries` lays them
/// out: a parent's rows follow it, so a hub that claims the parent claims
/// these with it.
fn linked_by_parent(workspaces: &[Workspace]) -> HashMap<usize, Vec<usize>> {
    let mut linked = HashMap::<usize, Vec<usize>>::new();
    let mut parent = None;
    for (index, child) in workspace_entries(workspaces) {
        if child {
            if let Some(parent) = parent {
                linked.entry(parent).or_default().push(index);
            }
        } else {
            parent = Some(index);
        }
    }
    linked
}

pub(super) fn visible_workspace_entries(
    workspaces: &[Workspace],
    collapsed: &HashSet<String>,
    workspace_groups: &[String],
) -> Vec<(usize, bool, Option<String>)> {
    let grouped = grouped_keys(workspaces);
    // The repository key a parent workspace folds, when it heads a group.
    let folded_group = |index: usize| {
        workspaces[index]
            .worktree
            .as_ref()
            .filter(|tree| !tree.linked && grouped.contains(tree.key.as_str()))
            .map(|tree| tree.key.clone())
    };
    // Each parent's linked worktrees, in the order `workspace_entries` lays
    // them out, so a parent claimed by a folder keeps them under its row.
    let linked = linked_by_parent(workspaces);
    let folders = folder_groups(workspaces, workspace_groups);
    let folder_head = folders
        .iter()
        .enumerate()
        .map(|(group, folder)| (folder.head, group))
        .collect::<HashMap<_, _>>();
    let folder_child = folders
        .iter()
        .enumerate()
        .flat_map(|(group, folder)| {
            folder
                .children
                .iter()
                .copied()
                .map(move |child| (child, group))
        })
        .collect::<HashMap<_, _>>();
    // Linked worktrees a hub owns: their parent is the hub's head or one of
    // its children. The hub's row places them, so the daemon-order pass must
    // skip them wherever in that order they sit, folded hub or not.
    let mut hub_linked = HashSet::new();
    for (&parent, children) in &linked {
        if folder_head.contains_key(&parent) || folder_child.contains_key(&parent) {
            for &child in children {
                hub_linked.insert(child);
            }
        }
    }
    let mut entries = Vec::new();
    // Rows placed while walking a hub, so `workspace_entries`' own pass over
    // the same indexes does not repeat them.
    let mut placed = HashSet::new();
    let place_linked = |linked: &HashMap<usize, Vec<usize>>,
                        placed: &mut HashSet<usize>,
                        parent: usize,
                        entries: &mut Vec<(usize, bool, Option<String>)>| {
        if let Some(children) = linked.get(&parent) {
            for &child in children {
                if placed.insert(child) {
                    entries.push((child, true, None));
                }
            }
        }
    };
    for (index, child) in workspace_entries(workspaces) {
        if placed.contains(&index)
            || hub_linked.contains(&index)
            || folder_child.contains_key(&index)
        {
            continue;
        }
        if let Some(&group) = folder_head.get(&index) {
            let folder = &folders[group];
            entries.push((folder.head, false, Some(folder.key.clone())));
            if collapsed.contains(&folder.key) {
                continue;
            }
            // The hub itself may parent a repository group.
            if folded_group(folder.head).is_some() {
                place_linked(&linked, &mut placed, folder.head, &mut entries);
            }
            for &child in &folder.children {
                let key = folded_group(child);
                entries.push((child, true, key.clone()));
                if key.is_none_or(|key| !collapsed.contains(&key)) {
                    place_linked(&linked, &mut placed, child, &mut entries);
                }
            }
            continue;
        }
        let key = workspaces[index]
            .worktree
            .as_ref()
            .map(|tree| tree.key.as_str())
            .filter(|key| grouped.contains(key));
        if child && key.is_some_and(|key| collapsed.contains(key)) {
            continue;
        }
        let group = key.filter(|_| !child).map(str::to_owned);
        entries.push((index, child, group));
    }
    entries
}

fn workspace_git_input(workspace: &Workspace) -> Option<crate::pull_request::Input> {
    let branch = workspace.branch.as_deref().filter(|branch| {
        !branch.is_empty() && branch.len() <= 1024 && !branch.chars().any(char::is_control)
    })?;
    let worktree = workspace.worktree.as_ref().filter(|tree| {
        tree.linked || !sheprd_github::checkout::left_checkout(&tree.key, &workspace.cwd)
    });
    if let Some(tree) = worktree {
        return Some(crate::pull_request::Input {
            checkout: None,
            repo_key: Some(tree.key.clone()),
            branch: branch.to_owned(),
        });
    }
    Path::new(&workspace.cwd)
        .is_absolute()
        .then(|| crate::pull_request::Input {
            checkout: Some(workspace.cwd.clone()),
            repo_key: None,
            branch: branch.to_owned(),
        })
}

/// Cached pull request, change counts, dirty, note, and teleported marks for a worktree row, if
/// the prefetch and the Git probe already have them. Rendering only reads: a missing entry
/// simply shows nothing, never a stale or guessed state. `git` carries which counts to show:
/// the pull request's, or the checkout's uncommitted lines, never both. Its cache is `None`
/// for a row on another device: it only ever probes this machine's checkouts.
/// `pr` pairs the PR cache with the view of the device the row is on, whether
/// or not it is selected, and is `None` for a device whose pull requests are
/// not looked up. Any workspace, Git checkout or not, carries its muted mark.
pub(super) fn workspace_badge(
    workspace: &Workspace,
    pr: Option<(&crate::pull_request::Cache, &crate::pull_request::View)>,
    git: (Option<&crate::git::Git>, ChangeCounts),
    marks: (&crate::teleport::Marks, &str),
    (note, muted): (Option<&str>, bool),
    theme: &Theme,
) -> Option<RowBadge> {
    let input = workspace_git_input(workspace);
    let tracked = git.0.and_then(|git| git.tracked());
    // The live status belongs to the tracked checkout. A row reads it only
    // when its own lookup key, or the folder it has instead of one, names
    // that checkout: each session has its own focused row, so focus alone
    // must never borrow another project's counts.
    let tracks_this_workspace = tracked.is_some_and(|tracked| {
        input.as_ref().map_or_else(
            || tracked.checkout.as_deref() == Some(workspace.cwd.as_str()),
            |input| input == tracked,
        )
    });
    let focused_counts = tracks_this_workspace
        .then(|| {
            if git.1 == ChangeCounts::Local {
                git.0
                    .and_then(|git| git.status())
                    .filter(|status| status.additions > 0 || status.deletions > 0)
            } else {
                None
            }
        })
        .flatten();
    let focused_dirty = tracks_this_workspace
        .then(|| git.0.and_then(|git| git.uncommitted()))
        .flatten();
    let cached_counts = git
        .0
        .and_then(|git| input.as_ref().and_then(|input| git.counts_for_input(input)));
    let cached_dirty = git
        .0
        .and_then(|git| input.as_ref().and_then(|input| git.dirty_for_input(input)));
    let checkout = workspace
        .worktree
        .as_ref()
        .map(|tree| tree.key.as_str())
        .zip(workspace.branch.as_deref());
    let Some((key, branch)) = checkout else {
        return RowBadge::new(
            None,
            (git.1 == ChangeCounts::Local)
                .then(|| focused_counts.or(cached_counts))
                .flatten()
                .filter(|status| status.additions > 0 || status.deletions > 0)
                .map(|status| Counts::local(status.additions, status.deletions)),
            focused_dirty.or(cached_dirty).unwrap_or(false),
            false,
            false,
            muted,
        );
    };
    let ((marks, endpoint), (git, counts)) = (marks, git);
    // Kept SSH lookups include the checkout directory, unlike H1's
    // repository-only metadata. Read exactly the key prefetch schedules.
    let input = pr.and_then(|(_, view)| match workspace.keeper {
        sheprd_session::Keeper::App => crate::pull_request::kept_input(
            workspace,
            view.origin != crate::pull_request::Origin::Local,
        ),
        sheprd_session::Keeper::Herdr1 => Some(crate::pull_request::Input {
            checkout: None,
            repo_key: Some(key.to_owned()),
            branch: branch.to_owned(),
        }),
    });
    let ahead = git.and_then(|git| git.ahead(key, branch));
    // Commits past a merged pull request need a new one, so its number and
    // counts give way rather than read as covering them.
    let unopened =
        pr.zip(ahead)
            .zip(input.as_ref())
            .is_some_and(|(((cache, view), ahead), input)| {
                cache.unopened_input(view, input, &ahead.head)
            });
    let pr = pr
        .filter(|_| !unopened)
        .zip(input.as_ref())
        .and_then(|((cache, view), input)| cache.peek_input(view, input));
    let counts = match counts {
        ChangeCounts::PullRequest => pr
            .as_ref()
            .map(|pr| Counts::new(pr.additions, pr.deletions)),
        // Untracked files have no line counts; the dirty mark still shows them.
        ChangeCounts::Local => focused_counts
            .or(cached_counts)
            .filter(|status| status.additions > 0 || status.deletions > 0)
            .map(|status| Counts::local(status.additions, status.deletions)),
    };
    RowBadge::new(
        pr.map(|pr| PrBadge::new(&pr, theme))
            .or_else(|| unopened.then(|| PrBadge::unopened(theme))),
        counts,
        focused_dirty.or(cached_dirty).unwrap_or(false),
        marks.find(endpoint, key, branch).is_some(),
        note.is_some(),
        muted,
    )
}

/// The note the user keeps on a workspace's checkout, if it has one.
pub(super) fn workspace_note<'a>(
    workspace: &Workspace,
    notes: Option<&'a crate::worktree_notes::Notes>,
    endpoint: &str,
) -> Option<&'a str> {
    notes?.get(
        endpoint,
        &workspace.worktree.as_ref()?.key,
        workspace.branch.as_deref()?,
    )
}

pub(crate) fn workspace_label(workspace: &Workspace, indented: bool) -> &str {
    label_of(
        &workspace.label,
        workspace.branch.as_deref(),
        workspace.custom_label,
        // Only a linked worktree's row names its branch: a plain project
        // grouped under a folder hub keeps its own name, whatever its branch.
        indented && crate::worktree_identity::is_linked(workspace),
    )
}

/// A workspace's row name from its parts: a linked worktree's row shows its
/// branch unless the user named it.
pub(crate) fn label_of<'a>(
    label: &'a str,
    branch: Option<&'a str>,
    custom_label: bool,
    indented: bool,
) -> &'a str {
    let branch = (indented && !custom_label)
        .then_some(branch)
        .flatten()
        .map(|branch| branch.strip_prefix("worktree/").unwrap_or(branch));
    // The fallback, for a workspace with no label at all, stays English
    // while other areas borrow the label as `&str`.
    // i18n: data
    first_text([branch, Some(label)], "workspace")
}

/// Collapsed groups show their most urgent member's status.
/// The status each folded hub's head row shows, built once per render:
/// `folder_groups` compares every workspace with every other, so rebuilding
/// it per row would multiply that walk by the row count.
pub(super) struct FolderStatuses {
    heads: HashMap<String, AgentStatus>,
}

impl FolderStatuses {
    pub(super) fn of(
        workspaces: &[Workspace],
        collapsed: &HashSet<String>,
        workspace_groups: &[String],
    ) -> Self {
        // Only a folded hub has a status to carry; repository keys are bare
        // paths, so no other collapse asks for this.
        let building = collapsed.iter().any(|key| key.starts_with("folder:"));
        let heads = if building {
            // A folded hub hides its children and the linked worktrees those
            // children keep, so the hub's row speaks for all of their work.
            let linked = linked_by_parent(workspaces);
            folder_groups(workspaces, workspace_groups)
                .into_iter()
                .filter(|group| collapsed.contains(&group.key))
                .map(|group| {
                    let mut statuses = vec![workspaces[group.head].status];
                    statuses.extend(group.children.iter().map(|&index| workspaces[index].status));
                    for parent in std::iter::once(group.head).chain(group.children.iter().copied())
                    {
                        if let Some(children) = linked.get(&parent) {
                            statuses.extend(children.iter().map(|&index| workspaces[index].status));
                        }
                    }
                    let status = statuses
                        .into_iter()
                        .max_by_key(|status| status_priority(*status))
                        .unwrap_or(workspaces[group.head].status);
                    (workspaces[group.head].id.clone(), status)
                })
                .collect()
        } else {
            HashMap::new()
        };
        Self { heads }
    }

    fn head_status(&self, workspace: &Workspace) -> Option<AgentStatus> {
        self.heads.get(&workspace.id).copied()
    }
}

pub(super) fn displayed_workspace_status(
    workspaces: &[Workspace],
    workspace: &Workspace,
    collapsed: &HashSet<String>,
    folders: &FolderStatuses,
) -> AgentStatus {
    let worktree_status = workspace
        .worktree
        .as_ref()
        .filter(|worktree| !worktree.linked && collapsed.contains(&worktree.key))
        .map(|worktree| {
            workspaces
                .iter()
                .filter(|candidate| {
                    candidate
                        .worktree
                        .as_ref()
                        .is_some_and(|candidate| candidate.key == worktree.key)
                })
                .map(|candidate| candidate.status)
                .max_by_key(|status| status_priority(*status))
                .unwrap_or(workspace.status)
        })
        .unwrap_or(workspace.status);
    folders.head_status(workspace).unwrap_or(worktree_status)
}
