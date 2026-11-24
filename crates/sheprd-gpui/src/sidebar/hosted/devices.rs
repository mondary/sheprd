//! Every device the sidebar lists, for a host's layout: its header (name,
//! connection, load, folded or not) and the rows under it, each with what
//! the window's own row shows, in the order and with the folds it shows them.
//! Rows come from each device's sessions, as the window's own sidebar reads
//! them: Herdr 1's workspaces, then those the app keeps for herdr-core.

#[cfg(feature = "moonframe")]
use super::group_actions;
#[cfg(feature = "moonframe")]
use crate::sidebar::workspaces::{grouped_keys, visible_repository_entries};
use super::{hex, load_state, status_name};
use crate::{
    endpoint::Endpoint,
    session_model::Sessions,
    sidebar::workspaces::{
        FolderStatuses, displayed_workspace_status, workspace_badge, workspace_entries,
        workspace_label, workspace_note,
    },
    window::SheprdWindow,
};
#[cfg(not(feature = "moonframe"))]
use crate::sidebar::workspaces::visible_workspace_entries;
use gpui::App;
use serde_json::{Value, json};
use sheprd_session::{Keeper, Session, Workspace};
use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

/// One row's place: its device, the repository it folds when it heads a
/// group of linked worktrees, and whether it shows as focused.
struct Place<'a> {
    index: usize,
    endpoint: &'a Endpoint,
    group: Option<String>,
    #[cfg(feature = "moonframe")]
    project_root: Option<&'a str>,
    collapsed: &'a HashSet<String>,
    muted: bool,
    focused: bool,
    number: usize,
    /// The status its dot shows: a folded group's most urgent, when the
    /// config shows statuses inline, as the window's own row does.
    status: sheprd_client::protocol::AgentStatus,
}

impl SheprdWindow {
    /// The window's folds and devices, as one number: equal numbers say no
    /// header or group was folded and no device's sessions changed. Read on
    /// every notification, so it allocates nothing.
    pub(super) fn hosted_devices_stamp(&self) -> u64 {
        let mut stamp = 0u64;
        let mut mix = |value: &dyn Fn(&mut DefaultHasher)| {
            let mut hasher = DefaultHasher::new();
            value(&mut hasher);
            // A sum, so the sets' order does not count.
            stamp = stamp.wrapping_add(hasher.finish());
        };
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            let snapshot = live
                .snapshot
                .as_ref()
                .map(|snapshot| std::sync::Arc::as_ptr(snapshot) as usize);
            mix(&|hasher| {
                (
                    index,
                    &endpoint.id,
                    endpoint.collapsed,
                    endpoint.enabled,
                    snapshot,
                )
                    .hash(hasher);
                live.status.is_connected().hash(hasher);
            });
            for key in &endpoint.collapsed_repos {
                mix(&|hasher| (index, key).hash(hasher));
            }
        }
        for key in &self.collapsed_repos {
            mix(&|hasher| ("local", key).hash(hasher));
        }
        // The workspaces the app keeps for herdr-core, and the one in front.
        let core = self
            .sessions(self.selected_endpoint)
            .core
            .as_ref()
            .map(|session| std::sync::Arc::as_ptr(session) as usize);
        let front = self.kept_in_front();
        mix(&|hasher| (core, front).hash(hasher));
        stamp
    }

    /// Each device the sidebar lists, in its order, with its rows.
    pub(super) fn hosted_devices(&self, cx: &App) -> Vec<Value> {
        let multi = self.endpoints.len() > 1;
        self.displayed_devices()
            .into_iter()
            .filter(|(_, endpoint)| self.device_visible(&endpoint.id))
            .map(|(index, endpoint)| {
                let selected = index == self.selected_endpoint;
                let live = if selected { &self.live } else { &endpoint.live };
                let host = crate::usage::Host::load_of(&endpoint.connection.target);
                let reading = host
                    .as_ref()
                    .filter(|_| self.config.show_system_load)
                    .and_then(|host| self.system_load.get(host))
                    .and_then(|load| load.latest().copied());
                let folded = multi && endpoint.collapsed;
                let sessions = self.sidebar_sessions(index, cx);
                let rows = if folded {
                    Vec::new()
                } else {
                    self.hosted_rows(index, endpoint, &sessions, cx)
                };
                let state = json!({
                    "collapsed": folded,
                    "connected": live.status.is_connected(),
                    "enabled": endpoint.enabled,
                    "id": endpoint.id,
                    "label": endpoint.label,
                    "load": reading.as_ref().map(|reading| load_state(Some(reading))),
                    "removing": self.device_removing(endpoint),
                    "search": search_rows(endpoint, &sessions, cx),
                    "selected": selected,
                    "workspaces": rows,
                });
                #[cfg(feature = "moonframe")]
                let state = self.with_project_groups(state, index, endpoint, &sessions, folded, cx);
                state
            })
            .collect()
    }

    /// Every workspace of the selected device, folded or not, in the
    /// sidebar's order, for what reads the focused one.
    pub(super) fn hosted_workspaces(&self, cx: &App) -> Vec<Value> {
        let Some(endpoint) = self.endpoints.get(self.selected_endpoint) else {
            return Vec::new();
        };
        let none = HashSet::new();
        let sessions = self.sidebar_sessions(self.selected_endpoint, cx);
        let silenced = self.silenced(endpoint, cx);
        let kept_front = self.kept_in_front();
        let mut number = 0;
        let mut rows = Vec::new();
        for (keeper, session) in sessions.keyed() {
            let front = Sessions::front_of(keeper, session, kept_front);
            #[cfg(feature = "moonframe")]
            let mut project_root = None;
            for (row, child) in workspace_entries(&session.workspaces) {
                let workspace = &session.workspaces[row];
                #[cfg(feature = "moonframe")]
                if !child {
                    project_root = group_actions::project_key(workspace);
                }
                number += 1;
                let place = Place {
                    index: self.selected_endpoint,
                    endpoint,
                    group: None,
                    #[cfg(feature = "moonframe")]
                    project_root,
                    collapsed: &none,
                    muted: silenced.contains(&workspace.id),
                    focused: front == Some(workspace.id.as_str()),
                    number,
                    status: workspace.status,
                };
                rows.push(self.hosted_row(workspace, child, &place, cx));
            }
        }
        rows
    }

    /// The workspaces of `endpoint` whose notifications are muted. Mutes are
    /// kept per Herdr 1 daemon; a device without one has none.
    fn silenced(&self, endpoint: &Endpoint, cx: &App) -> HashSet<String> {
        let index = self.endpoints.iter().position(|e| e.id == endpoint.id);
        let live = match index {
            Some(index) if index == self.selected_endpoint => &self.live,
            _ => &endpoint.live,
        };
        let Some(snapshot) = live.snapshot.as_deref() else {
            return HashSet::new();
        };
        let silenced = crate::notification_mutes::Mutes::silenced_in(cx, endpoint, snapshot);
        snapshot
            .workspaces
            .iter()
            .map(|workspace| &workspace.workspace_id)
            .filter(|id| silenced.workspace(id))
            .cloned()
            .collect()
    }

    /// A device's rows as its section lists them: each session's workspaces,
    /// linked worktrees under their checkout, and none of a folded group's.
    fn hosted_rows(
        &self,
        index: usize,
        endpoint: &Endpoint,
        sessions: &Sessions,
        cx: &App,
    ) -> Vec<Value> {
        let collapsed = if index == 0 {
            &self.collapsed_repos
        } else {
            &endpoint.collapsed_repos
        };
        let silenced = self.silenced(endpoint, cx);
        let selected = index == self.selected_endpoint;
        let kept_front = self.kept_in_front();
        let mut number = 0;
        let mut rows = Vec::new();
        for (keeper, session) in sessions.keyed() {
            let front = selected
                .then(|| Sessions::front_of(keeper, session, kept_front))
                .flatten();
            #[cfg(feature = "moonframe")]
            let (folder_statuses, entries) = (
                FolderStatuses::default(),
                visible_repository_entries(&session.workspaces, collapsed),
            );
            #[cfg(not(feature = "moonframe"))]
            let folder_statuses = FolderStatuses::of(
                &session.workspaces,
                collapsed,
                &self.config.sidebar_style.workspace_groups,
            );
            #[cfg(not(feature = "moonframe"))]
            let entries = visible_workspace_entries(
                &session.workspaces,
                collapsed,
                &self.config.sidebar_style.workspace_groups,
            );
            #[cfg(feature = "moonframe")]
            let mut project_root = None;
            for (row, child, group) in entries {
                let workspace = &session.workspaces[row];
                #[cfg(feature = "moonframe")]
                if !child {
                    project_root = group_actions::project_key(workspace);
                }
                number += 1;
                let place = Place {
                    index,
                    endpoint,
                    group,
                    #[cfg(feature = "moonframe")]
                    project_root,
                    collapsed,
                    muted: silenced.contains(&workspace.id),
                    focused: front == Some(workspace.id.as_str()),
                    number,
                    status: if self.config.usage.inline {
                        displayed_workspace_status(
                            &session.workspaces,
                            workspace,
                            collapsed,
                            &folder_statuses,
                        )
                    } else {
                        workspace.status
                    },
                };
                rows.push(self.hosted_row(workspace, child, &place, cx));
            }
        }
        rows
    }

    /// One row: what the window's row shows, its device, and its fold.
    fn hosted_row(&self, workspace: &Workspace, child: bool, place: &Place<'_>, cx: &App) -> Value {
        let retained = self.is_closed_project_row(&place.endpoint.id, &workspace.id, cx);
        let note = workspace_note(
            workspace,
            crate::worktree_notes::Notes::of(cx),
            &place.endpoint.id,
        );
        let pr_view = self.endpoint_pr_view(place.index);
        let badge = workspace_badge(
            workspace,
            pr_view.as_ref().map(|view| (&self.menu.pr_cache, view)),
            // Only this computer's checkouts are probed; a remote one with
            // the same repository and branch is another checkout.
            (
                (place.index == 0).then_some(&self.git),
                self.config.sidebar_style.change_counts,
            ),
            (&self.teleport_marks, &place.endpoint.id),
            (note, place.muted),
            &self.theme,
        );
        let (ahead, behind) = workspace.ahead_behind.unwrap_or((0, 0));
        let tree = workspace.worktree.as_ref();
        let counts = badge.as_ref().and_then(|badge| badge.counts.as_ref());
        let pr = badge.as_ref().and_then(|badge| badge.pr.as_ref());
        let state = json!({
            "added": counts.map(|counts| counts.additions.clone()),
            "agent_status": status_name(place.status),
            "ahead": ahead,
            "behind": behind,
            "branch": workspace.branch,
            "child": child,
            "collapsed": place.group.as_ref().is_some_and(|key| place.collapsed.contains(key)),
            "dirty": badge.as_ref().is_some_and(|badge| badge.dirty),
            "endpoint": place.endpoint.id,
            "focused": place.focused,
            "group": place.group,
            "id": workspace.id,
            "kept": workspace.keeper == Keeper::App,
            "label": workspace_label(workspace, child),
            "linked": tree.is_some_and(|tree| tree.linked),
            "local_counts": counts.is_some_and(|counts| counts.local),
            "muted": place.muted,
            "note": note,
            "number": place.number,
            "pr": pr.map(|pr| pr.number.clone()),
            // The number's color says the pull request's state, as the row's
            // own badge does.
            "pr_color": pr.map(|pr| hex(pr.color)),
            "retained": retained,
            "removed": counts.map(|counts| counts.deletions.clone()),
            "repo": tree.map(|tree| tree.label.clone()),
            "repo_key": tree.map(|tree| tree.key.clone()),
            "teleported": badge.as_ref().is_some_and(|badge| badge.teleported),
        });
        #[cfg(feature = "moonframe")]
        let state = project_group_row(state, workspace, child, place, cx);
        state
    }

    #[cfg(feature = "moonframe")]
    fn with_project_groups(
        &self,
        mut state: Value,
        index: usize,
        endpoint: &Endpoint,
        sessions: &Sessions,
        folded: bool,
        cx: &App,
    ) -> Value {
        if let Some(groups) = group_actions::for_endpoint(endpoint, cx) {
            state["project_groups"] = Value::Array(
                groups
                    .entries()
                    .iter()
                    .map(|group| {
                        // A random UUID gives old and new groups a stable hue
                        // without adding a field to their saved state.
                        let hue = f32::from((group.id.as_u128() >> 112) as u16) / 65_536.;
                        let rgb = gpui::Rgba::from(gpui::Hsla {
                            h: hue,
                            s: 0.65,
                            l: 0.55,
                            a: 1.,
                        });
                        json!({
                            "id": group.id,
                            "name": group.name,
                            "folded": group.folded,
                            "color": hex(self.theme.ink(u32::from(rgb) >> 8)),
                        })
                    })
                    .collect(),
            );
        }
        if folded {
            let collapsed = if index == 0 {
                &self.collapsed_repos
            } else {
                &endpoint.collapsed_repos
            };
            let mut has_worktrees = false;
            let has_open_worktrees = sessions.each().any(|session| {
                let keys = grouped_keys(&session.workspaces);
                has_worktrees |= !keys.is_empty();
                keys.into_iter().any(|key| !collapsed.contains(key))
            });
            state["has_worktrees"] = json!(has_worktrees);
            state["has_open_worktrees"] = json!(has_open_worktrees);
        }
        state
    }
}

#[cfg(feature = "moonframe")]
fn project_group_row(
    mut state: Value,
    workspace: &Workspace,
    child: bool,
    place: &Place<'_>,
    cx: &App,
) -> Value {
    let Some(groups) = group_actions::for_endpoint(place.endpoint, cx) else {
        return state;
    };
    if let Some(id) = groups.workspace_group(std::path::Path::new(&workspace.cwd)) {
        state["group_session"] = json!(id);
    } else if let Some(key) = place.project_root {
        if let Some(id) = groups.group_of(key) {
            state["project_group"] = json!(id);
        }
        state["groupable"] = json!(!child);
    }
    state
}

/// Every workspace of a device, folded or not, with what the sidebar's search
/// matches: its name, its branch and the note on its checkout, as the
/// window's own search reads them.
fn search_rows(endpoint: &Endpoint, sessions: &Sessions, cx: &App) -> Vec<Value> {
    let notes = crate::worktree_notes::Notes::of(cx);
    sessions
        .each()
        .flat_map(|session: &Session| {
            workspace_entries(&session.workspaces)
                .into_iter()
                .map(move |(row, child)| (&session.workspaces[row], child))
        })
        .map(|(workspace, child)| {
            json!({
                "agent_status": status_name(workspace.status),
                "branch": workspace.branch,
                "endpoint": endpoint.id,
                "id": workspace.id,
                "label": workspace_label(workspace, child),
                "note": workspace_note(workspace, notes, &endpoint.id),
            })
        })
        .collect()
}
