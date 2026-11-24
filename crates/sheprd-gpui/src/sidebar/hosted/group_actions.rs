//! The Moonframe sidebar's project groups wrap ordinary local workspaces.

use super::project_groups::{Groups, prepare_session};
use crate::{
    endpoint::Endpoint,
    window::{Flash, SheprdWindow},
};
use gpui::{App, Context, Window};
use sheprd_session::Workspace;
use std::path::Path;
use uuid::Uuid;

pub(super) fn is_local(endpoint: &Endpoint) -> bool {
    endpoint.id == crate::endpoint::LOCAL && !endpoint.connection.target.is_remote()
}

pub(super) fn for_endpoint<'a>(endpoint: &Endpoint, cx: &'a App) -> Option<&'a Groups> {
    if is_local(endpoint) {
        cx.try_global::<Groups>()
    } else {
        None
    }
}

/// The launch directory, which stays stable when worktree metadata appears.
/// A linked checkout's Git directory can name its primary checkout even
/// when that checkout is not open in the sidebar.
pub(super) fn project_key(workspace: &Workspace) -> Option<&str> {
    if !Path::new(&workspace.cwd).is_absolute() {
        return None;
    }
    let main = workspace
        .worktree
        .as_ref()
        .filter(|tree| tree.linked)
        .and_then(|tree| tree.key.strip_suffix("/.git").or_else(|| tree.key.strip_suffix("\\.git")))
        .filter(|path| Path::new(path).is_absolute());
    Some(main.unwrap_or(&workspace.cwd))
}

impl SheprdWindow {
    pub(super) fn create_project_group(&mut self, name: &str, cx: &mut Context<Self>) {
        if cx.try_global::<Groups>().is_none() {
            return;
        }
        if let Err(error) = cx.global_mut::<Groups>().create(name) {
            self.show_flash(Flash::warning(error), cx);
        }
        cx.notify();
    }

    pub(super) fn toggle_project_group(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if cx.try_global::<Groups>().is_some() {
            cx.global_mut::<Groups>().toggle(id);
            cx.notify();
        }
    }

    pub(super) fn assign_project_group(
        &mut self,
        workspace: &str,
        group: Option<Uuid>,
        cx: &mut Context<Self>,
    ) {
        let Some(endpoint) = self.endpoints.get(self.selected_endpoint) else {
            return;
        };
        let Some(groups) = for_endpoint(endpoint, cx) else {
            return;
        };
        let sessions = self.sidebar_sessions(self.selected_endpoint, cx);
        let Some(workspace) = sessions.workspace(workspace) else {
            return;
        };
        if groups.workspace_group(Path::new(&workspace.cwd)).is_some() {
            return;
        }
        let Some(key) = project_key(workspace) else {
            return;
        };
        cx.global_mut::<Groups>()
            .assign(key, &workspace.cwd, &workspace.label, group);
        cx.notify();
    }

    pub(super) fn open_project_group(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(groups) = cx.try_global::<Groups>() else {
            return;
        };
        let Some((group, dir)) = groups.get(id).zip(groups.session_dir(id)) else {
            return;
        };
        let Some(local) = self
            .endpoints
            .iter()
            .position(|endpoint| is_local(endpoint) && endpoint.enabled)
        else {
            return;
        };
        let sessions = self.sessions(local);
        let open = sessions
            .each()
            .flat_map(|session| &session.workspaces)
            .find(|workspace| Path::new(&workspace.cwd) == dir.as_path());
        if let Some(open) = open {
            self.activate_sidebar_workspace(crate::endpoint::LOCAL, &open.id, window, cx);
            return;
        }
        let name = group.name.clone();
        let prepared = cx.background_executor().spawn(async move {
            prepare_session(&dir, id)?;
            Ok::<_, std::io::Error>(dir)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = prepared.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(dir) if this.local_workspaces_use_core() => {
                    this.open_local_kept_folder(&name, dir, cx);
                }
                Ok(dir) => {
                    if let Err(error) = this.open_named_project_at(dir, name, window, cx) {
                        this.show_flash(Flash::warning(error.to_string()), cx);
                    }
                }
                Err(error) => this.show_flash(Flash::warning(error.to_string()), cx),
            });
        })
        .detach();
    }
}
