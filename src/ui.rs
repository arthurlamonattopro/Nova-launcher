use std::path::PathBuf;

use iced::{
    Background, Border, Color, Element, Length, Size, Task, Theme, alignment,
    futures::channel::mpsc,
    widget::{
        button, checkbox, column, container, pick_list, progress_bar, responsive, row, rule,
        scrollable, slider, text, text::Wrapping, text_input,
    },
    window,
};

use crate::core::{
    LaunchProgress, LaunchRequest, LoaderChoice, MinecraftVersion, VersionKind,
    default_minecraft_dir, launch_with_progress, list_versions,
};
use crate::mods::{
    InstallProjectReport, InstallProjectRequest, ManagedProject, ProjectKind, ProjectSearchRequest,
    ProjectSource, install_project, list_installed_projects, search_projects, uninstall_project,
};

#[derive(Debug, Clone)]
pub enum Message {
    UsernameChanged(String),
    VersionChanged(String),
    VersionPicked(String),
    ShowReleasesChanged(bool),
    ShowSnapshotsChanged(bool),
    ShowOldBetaChanged(bool),
    ShowOldAlphaChanged(bool),
    RefreshVersions,
    VersionsLoaded(Result<Vec<MinecraftVersion>, String>),
    InitialDataLoaded(Result<(Vec<MinecraftVersion>, Vec<ManagedProject>), String>),
    LoaderChanged(LoaderChoice),
    JavaPathChanged(String),
    MinecraftDirChanged(String),
    MemoryChanged(u16),
    LaunchPressed,
    LaunchModpackPressed(ManagedProject),
    LaunchEvent(LaunchProgress),
    TabSelected(Tab),
    ProjectSourceChanged(ProjectSource),
    ProjectKindChanged(ProjectKind),
    ProjectQueryChanged(String),
    CurseForgeApiKeyChanged(String),
    SearchProjects,
    ProjectsLoaded(Result<Vec<ManagedProject>, String>),
    InstallProjectPressed(ManagedProject),
    InstallProjectResult(Result<InstallProjectReport, String>),
    RefreshInstalledProjects,
    InstalledProjectsLoaded(Vec<ManagedProject>),
    UninstallProjectPressed(ManagedProject),
    UninstallProjectResult(Result<(), String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Launcher,
    Mods,
}

#[derive(Debug, Clone)]
pub struct NovaLauncher {
    username: String,
    minecraft_version: String,
    versions: Vec<MinecraftVersion>,
    versions_loading: bool,
    show_releases: bool,
    show_snapshots: bool,
    show_old_beta: bool,
    show_old_alpha: bool,
    loader: LoaderChoice,
    java_path: String,
    minecraft_dir: String,
    memory_mb: u16,
    status: String,
    status_is_error: bool,
    progress_percent: f32,
    busy: bool,
    tab: Tab,
    project_source: ProjectSource,
    project_kind: ProjectKind,
    project_query: String,
    curseforge_api_key: String,
    search_results: Vec<ManagedProject>,
    search_loading: bool,
    search_error: String,
    installed_projects: Vec<ManagedProject>,
    installed_loading: bool,
    project_action_busy: bool,
}

impl Default for NovaLauncher {
    fn default() -> Self {
        Self {
            username: "Steve".to_string(),
            minecraft_version: "1.20.1".to_string(),
            versions: Vec::new(),
            versions_loading: true,
            show_releases: true,
            show_snapshots: false,
            show_old_beta: false,
            show_old_alpha: false,
            loader: LoaderChoice::Vanilla,
            java_path: String::new(),
            minecraft_dir: default_minecraft_dir().display().to_string(),
            memory_mb: 2048,
            status: "Loading Minecraft versions...".to_string(),
            status_is_error: false,
            progress_percent: 0.0,
            busy: false,
            tab: Tab::Launcher,
            project_source: ProjectSource::Modrinth,
            project_kind: ProjectKind::Mod,
            project_query: String::new(),
            curseforge_api_key: String::new(),
            search_results: Vec::new(),
            search_loading: false,
            search_error: String::new(),
            installed_projects: Vec::new(),
            installed_loading: true,
            project_action_busy: false,
        }
    }
}

pub fn run() -> iced::Result {
    iced::application(boot, update, view)
        .title("Nova Launcher")
        .theme(Theme::Dark)
        .window(window::Settings {
            size: Size::new(860.0, 660.0),
            min_size: Some(Size::new(360.0, 480.0)),
            ..Default::default()
        })
        .centered()
        .run()
}

fn boot() -> (NovaLauncher, Task<Message>) {
    (
        NovaLauncher::default(),
        Task::perform(
            async {
                let installed = list_installed_projects(&default_minecraft_dir());
                list_versions().map(|versions| (versions, installed))
            },
            Message::InitialDataLoaded,
        ),
    )
}

fn update(app: &mut NovaLauncher, message: Message) -> Task<Message> {
    match message {
        Message::UsernameChanged(value) => app.username = value,
        Message::VersionChanged(value) => app.minecraft_version = value,
        Message::VersionPicked(value) => app.minecraft_version = value,
        Message::ShowReleasesChanged(value) => app.show_releases = value,
        Message::ShowSnapshotsChanged(value) => app.show_snapshots = value,
        Message::ShowOldBetaChanged(value) => app.show_old_beta = value,
        Message::ShowOldAlphaChanged(value) => app.show_old_alpha = value,
        Message::RefreshVersions => {
            if app.versions_loading {
                return Task::none();
            }

            app.versions_loading = true;
            app.status_is_error = false;
            app.status = "Refreshing Minecraft versions...".to_string();

            return Task::perform(async { list_versions() }, Message::VersionsLoaded);
        }
        Message::VersionsLoaded(result) => {
            app.versions_loading = false;

            match result {
                Ok(versions) => {
                    if app.minecraft_version.trim().is_empty() {
                        if let Some(version) = versions
                            .iter()
                            .find(|version| version.kind == VersionKind::Release)
                        {
                            app.minecraft_version = version.id.clone();
                        }
                    }

                    let count = versions.len();
                    app.versions = versions;
                    app.status_is_error = false;
                    app.status = format!("Loaded {count} Minecraft versions.");
                }
                Err(error) => {
                    app.status_is_error = true;
                    app.status = error;
                }
            }
        }
        Message::InitialDataLoaded(result) => {
            app.versions_loading = false;
            app.installed_loading = false;

            match result {
                Ok((versions, installed)) => {
                    if app.minecraft_version.trim().is_empty() {
                        if let Some(version) = versions
                            .iter()
                            .find(|version| version.kind == VersionKind::Release)
                        {
                            app.minecraft_version = version.id.clone();
                        }
                    }

                    app.versions = versions;
                    app.installed_projects = installed;
                    app.status_is_error = false;
                    app.status = "Ready.".to_string();
                }
                Err(error) => {
                    app.status_is_error = true;
                    app.status = error;
                }
            }
        }
        Message::ProjectsLoaded(result) => {
            app.search_loading = false;
            app.search_error.clear();

            match result {
                Ok(projects) => {
                    let count = projects.len();
                    app.search_results = projects;
                    app.status_is_error = false;
                    app.status = format!("Found {count} result(s).");
                }
                Err(error) => {
                    app.search_error = error.clone();
                    app.status_is_error = true;
                    app.status = error;
                }
            }
        }
        Message::InstallProjectPressed(project) => {
            if app.project_action_busy {
                return Task::none();
            }

            app.project_action_busy = true;
            app.status_is_error = false;
            app.status = format!("Installing {}...", project.title);

            let request = InstallProjectRequest {
                project,
                minecraft_dir: PathBuf::from(app.minecraft_dir.trim()),
                minecraft_version: app.minecraft_version.clone(),
                loader: app.loader,
                curseforge_api_key: app.curseforge_api_key.clone(),
            };

            return Task::perform(
                async move { install_project(request) },
                Message::InstallProjectResult,
            );
        }
        Message::InstallProjectResult(result) => {
            app.project_action_busy = false;

            match result {
                Ok(report) => {
                    app.status_is_error = false;
                    app.status = format!("Installed {}.", report.filename);

                    let minecraft_dir = PathBuf::from(app.minecraft_dir.trim());
                    app.installed_loading = true;
                    return Task::perform(
                        async move { list_installed_projects(&minecraft_dir) },
                        Message::InstalledProjectsLoaded,
                    );
                }
                Err(error) => {
                    app.status_is_error = true;
                    app.status = error;
                }
            }
        }
        Message::RefreshInstalledProjects => {
            if app.installed_loading {
                return Task::none();
            }

            app.installed_loading = true;
            app.status_is_error = false;
            app.status = "Refreshing installed projects...".to_string();
            let minecraft_dir = PathBuf::from(app.minecraft_dir.trim());

            return Task::perform(
                async move { list_installed_projects(&minecraft_dir) },
                Message::InstalledProjectsLoaded,
            );
        }
        Message::InstalledProjectsLoaded(installed) => {
            app.installed_loading = false;
            app.installed_projects = installed;
            app.status_is_error = false;
            app.status = format!("{} installed item(s).", app.installed_projects.len());
        }
        Message::UninstallProjectPressed(project) => {
            if app.project_action_busy {
                return Task::none();
            }

            app.project_action_busy = true;
            app.status_is_error = false;
            app.status = format!("Removing {}...", project.title);
            let minecraft_dir = PathBuf::from(app.minecraft_dir.trim());

            return Task::perform(
                async move { uninstall_project(&project, &minecraft_dir) },
                Message::UninstallProjectResult,
            );
        }
        Message::UninstallProjectResult(result) => {
            app.project_action_busy = false;

            match result {
                Ok(()) => {
                    app.status_is_error = false;
                    app.status = "Removed successfully.".to_string();
                    let minecraft_dir = PathBuf::from(app.minecraft_dir.trim());
                    app.installed_loading = true;
                    return Task::perform(
                        async move { list_installed_projects(&minecraft_dir) },
                        Message::InstalledProjectsLoaded,
                    );
                }
                Err(error) => {
                    app.status_is_error = true;
                    app.status = error;
                }
            }
        }
        Message::TabSelected(tab) => {
            app.tab = tab;
        }
        Message::ProjectSourceChanged(value) => app.project_source = value,
        Message::ProjectKindChanged(value) => app.project_kind = value,
        Message::ProjectQueryChanged(value) => app.project_query = value,
        Message::CurseForgeApiKeyChanged(value) => app.curseforge_api_key = value,
        Message::SearchProjects => {
            if app.search_loading {
                return Task::none();
            }

            if app.project_query.trim().is_empty() {
                app.search_error = "Enter a search query to find mods or modpacks.".to_string();
                app.status_is_error = true;
                app.status = "Search query required.".to_string();
                return Task::none();
            }

            app.search_loading = true;
            app.search_error.clear();
            app.status_is_error = false;
            app.status = "Searching...".to_string();

            let request = ProjectSearchRequest {
                source: app.project_source,
                kind: app.project_kind,
                query: app.project_query.clone(),
                minecraft_version: app.minecraft_version.clone(),
                loader: app.loader,
                curseforge_api_key: app.curseforge_api_key.clone(),
            };

            return search_task(request);
        }
        Message::LoaderChanged(value) => app.loader = value,
        Message::JavaPathChanged(value) => app.java_path = value,
        Message::MinecraftDirChanged(value) => app.minecraft_dir = value,
        Message::MemoryChanged(value) => app.memory_mb = value,
        Message::LaunchPressed => {
            if app.busy {
                return Task::none();
            }

            app.busy = true;
            app.progress_percent = 0.0;
            app.status_is_error = false;
            app.status = "Starting launch...".to_string();

            let request = LaunchRequest {
                minecraft_dir: PathBuf::from(app.minecraft_dir.trim()),
                username: app.username.clone(),
                minecraft_version: app.minecraft_version.clone(),
                loader: app.loader,
                java_path: optional_path(&app.java_path),
                memory_mb: app.memory_mb,
            };

            return launch_task(request);
        }
        Message::LaunchModpackPressed(project) => {
            if app.busy {
                return Task::none();
            }

            let modpack_dir = project.installed_path.clone().unwrap_or_else(|| {
                PathBuf::from(app.minecraft_dir.trim())
                    .join("modpacks")
                    .join(&project.id)
            });

            if !modpack_dir.is_dir() {
                app.status_is_error = true;
                app.status = format!(
                    "Modpack profile directory not found: {}",
                    modpack_dir.display()
                );
                return Task::none();
            }

            app.busy = true;
            app.progress_percent = 0.0;
            app.status_is_error = false;
            app.status = format!("Launching {}...", project.title);

            let request = LaunchRequest {
                minecraft_dir: modpack_dir,
                username: app.username.clone(),
                minecraft_version: app.minecraft_version.clone(),
                loader: app.loader,
                java_path: optional_path(&app.java_path),
                memory_mb: app.memory_mb,
            };

            return launch_task(request);
        }
        Message::LaunchEvent(event) => match event {
            LaunchProgress::Progress { label, percent } => {
                app.progress_percent = percent.clamp(0.0, 100.0);
                app.status_is_error = false;
                app.status = label;
            }
            LaunchProgress::Finished(result) => {
                app.busy = false;
                match result {
                    Ok(report) => {
                        app.progress_percent = 100.0;
                        app.status_is_error = false;
                        app.status =
                            format!("Launched {} (pid {}).", report.version_id, report.pid);
                    }
                    Err(error) => {
                        app.progress_percent = 0.0;
                        app.status_is_error = true;
                        app.status = error;
                    }
                }
            }
        },
    }

    Task::none()
}

fn view(app: &NovaLauncher) -> Element<'_, Message> {
    responsive(move |size| view_responsive(app, size)).into()
}

fn view_responsive(app: &NovaLauncher, size: Size) -> Element<'_, Message> {
    let layout = LayoutMode::from_width(size.width);

    // Title and tab bar share a row when there is enough horizontal space.
    let title_col = column![
        text("Nova Launcher").size(26),
        text("Offline Minecraft launcher")
            .size(12)
            .color([0.52, 0.58, 0.70]),
    ]
    .spacing(3)
    .width(Length::Fill);

    let tab_bar = row![
        tab_button("Launcher", Tab::Launcher, app.tab),
        tab_button("Mods & Modpacks", Tab::Mods, app.tab),
    ]
    .spacing(6);

    let header: Element<'_, Message> = if layout.is_narrow() {
        column![title_col, tab_bar].spacing(12).into()
    } else {
        row![title_col, tab_bar]
            .align_y(alignment::Vertical::Center)
            .spacing(12)
            .into()
    };

    let body = match app.tab {
        Tab::Launcher => launcher_body(app, layout),
        Tab::Mods => mods_body(app, layout),
    };

    // Status bar: colour and icon reflect current state.
    let (status_color, status_icon): (Color, &str) = if app.busy || app.project_action_busy {
        (Color::from_rgb(0.95, 0.85, 0.45), "⟳  ")
    } else if app.status_is_error {
        (Color::from_rgb(0.95, 0.50, 0.50), "✕  ")
    } else {
        (Color::from_rgb(0.50, 0.84, 0.62), "✓  ")
    };

    let status_bar = row![
        text(status_icon).size(13).color(status_color),
        text(&app.status)
            .size(13)
            .wrapping(Wrapping::Word)
            .color(status_color),
    ]
    .spacing(2)
    .width(Length::Fill);

    let content = column![
        header,
        rule::horizontal(1),
        body,
        rule::horizontal(1),
        status_bar,
    ]
    .spacing(layout.spacing())
    .padding(layout.padding())
    .width(Length::Fill)
    .max_width(layout.max_content_width());

    container(scrollable(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .into()
}

fn launcher_body(app: &NovaLauncher, layout: LayoutMode) -> Element<'_, Message> {
    let username = field(
        "Username",
        text_input("Steve", &app.username)
            .on_input(Message::UsernameChanged)
            .padding(12),
    );

    let version = field(
        "Minecraft version",
        column![
            version_picker_row(app, layout),
            text_input("Or type a version id", &app.minecraft_version)
                .on_input(Message::VersionChanged)
                .padding(12),
            version_filters(app),
        ]
        .spacing(8),
    );

    let loader = field(
        "Loader",
        pick_list(LoaderChoice::ALL, Some(app.loader), Message::LoaderChanged).padding(12),
    );

    let memory = column![
        text(format!("Memory  —  {} MB", app.memory_mb))
            .size(12)
            .color([0.62, 0.68, 0.78]),
        slider(512..=8192, app.memory_mb, Message::MemoryChanged).step(256u16),
    ]
    .spacing(8);

    let java_path = field(
        "Java executable",
        text_input("Leave empty to use java from PATH", &app.java_path)
            .on_input(Message::JavaPathChanged)
            .padding(12),
    );

    let minecraft_dir = field(
        "Launcher data directory",
        text_input("", &app.minecraft_dir)
            .on_input(Message::MinecraftDirChanged)
            .padding(12),
    );

    // Show the target version in the button label when one is selected.
    let version_str = app.minecraft_version.trim();
    let launch_label = if version_str.is_empty() {
        "Install & Launch".to_string()
    } else {
        format!("Install & Launch  {version_str}")
    };

    let launch_button = if app.busy {
        button(text("Working...").align_x(alignment::Horizontal::Center))
    } else {
        button(text(launch_label).align_x(alignment::Horizontal::Center))
            .on_press(Message::LaunchPressed)
    }
    .padding([14, 20])
    .width(Length::Fill);

    // Only render the progress section when a launch is active or has just finished.
    let progress: Element<'_, Message> = if app.busy || app.progress_percent > 0.0 {
        column![
            row![
                text("Progress")
                    .size(12)
                    .color([0.62, 0.68, 0.78])
                    .width(Length::Fill),
                text(format!("{:.0}%", app.progress_percent))
                    .size(12)
                    .color([0.62, 0.68, 0.78]),
            ]
            .spacing(8),
            progress_bar(0.0..=100.0, app.progress_percent),
        ]
        .spacing(6)
        .into()
    } else {
        column![].into()
    };

    let identity = responsive_pair(username, version, layout);
    let runtime = responsive_pair(loader, memory.into(), layout);

    column![
        identity,
        runtime,
        java_path,
        minecraft_dir,
        progress,
        launch_button,
    ]
    .spacing(layout.spacing())
    .width(Length::Fill)
    .into()
}

fn mods_body(app: &NovaLauncher, layout: LayoutMode) -> Element<'_, Message> {
    let version = field(
        "Minecraft version",
        column![
            version_picker_row(app, layout),
            text_input("Or type a version id", &app.minecraft_version)
                .on_input(Message::VersionChanged)
                .padding(12),
            version_filters(app),
        ]
        .spacing(8),
    );

    let loader = field(
        "Loader",
        pick_list(LoaderChoice::ALL, Some(app.loader), Message::LoaderChanged).padding(12),
    );

    let minecraft_dir = field(
        "Launcher data directory",
        text_input("", &app.minecraft_dir)
            .on_input(Message::MinecraftDirChanged)
            .padding(12),
    );

    let source = field(
        "Source",
        pick_list(
            ProjectSource::ALL,
            Some(app.project_source),
            Message::ProjectSourceChanged,
        )
        .padding(12),
    );

    let kind = field(
        "Type",
        pick_list(
            ProjectKind::ALL,
            Some(app.project_kind),
            Message::ProjectKindChanged,
        )
        .padding(12),
    );

    // Enter key submits the search.
    let query = field(
        "Search query",
        text_input("Search mods or modpacks...", &app.project_query)
            .on_input(Message::ProjectQueryChanged)
            .on_submit(Message::SearchProjects)
            .padding(12),
    );

    // Mask the API key so it is not shown in plain text.
    let curseforge_key = if app.project_source == ProjectSource::CurseForge {
        field(
            "CurseForge API key",
            text_input("Paste your API key here", &app.curseforge_api_key)
                .on_input(Message::CurseForgeApiKeyChanged)
                .secure(true)
                .padding(12),
        )
    } else {
        container(column![]).into()
    };

    let search_btn = if app.search_loading {
        button(text("Searching...").align_x(alignment::Horizontal::Center))
    } else {
        button(text("Search").align_x(alignment::Horizontal::Center))
            .on_press(Message::SearchProjects)
    }
    .padding(12)
    .width(Length::Fill);

    let refresh_btn = if app.installed_loading {
        button(text("Refreshing...").align_x(alignment::Horizontal::Center))
    } else {
        button(text("Refresh installed").align_x(alignment::Horizontal::Center))
            .on_press(Message::RefreshInstalledProjects)
    }
    .padding(12)
    .width(Length::Fill);

    let search_controls = column![
        responsive_pair(source, kind, layout),
        query,
        curseforge_key,
        row![search_btn, refresh_btn].spacing(12),
    ]
    .spacing(16);

    // Section headers include counts once data is available.
    let search_header = format!("Search results  ({})", app.search_results.len());
    let installed_header = format!("Installed  ({})", app.installed_projects.len());

    let search_results: Element<'_, Message> = if app.search_loading {
        text("Searching for projects...")
            .size(14)
            .color([0.62, 0.68, 0.78])
            .into()
    } else if !app.search_error.is_empty() {
        text(&app.search_error)
            .size(14)
            .color([0.95, 0.50, 0.50])
            .wrapping(Wrapping::Word)
            .into()
    } else if app.search_results.is_empty() {
        text("No results yet — type a query and press Search or Enter.")
            .size(13)
            .color([0.52, 0.58, 0.70])
            .wrapping(Wrapping::Word)
            .into()
    } else {
        let mut list = column![].spacing(10);

        for project in &app.search_results {
            let action_button = if app.project_action_busy {
                button(text("Working...").size(13))
            } else {
                button(text("Install").size(13))
                    .on_press(Message::InstallProjectPressed(project.clone()))
            }
            .padding([8, 14]);

            list = list.push(project_card(project, action_button.into()));
        }

        list.into()
    };

    let installed_projects: Element<'_, Message> = if app.installed_loading {
        text("Loading installed mods and modpacks...")
            .size(13)
            .color([0.52, 0.58, 0.70])
            .into()
    } else if app.installed_projects.is_empty() {
        text("No installed mods or modpacks found in the selected directory.")
            .size(13)
            .color([0.52, 0.58, 0.70])
            .wrapping(Wrapping::Word)
            .into()
    } else {
        let mut list = column![].spacing(10);

        for project in &app.installed_projects {
            let mut actions = row![].spacing(8);

            if project.kind == ProjectKind::Modpack {
                let launch_button = if app.busy || app.project_action_busy {
                    button(text("Working...").size(13))
                } else {
                    button(text("Launch").size(13))
                        .on_press(Message::LaunchModpackPressed(project.clone()))
                }
                .padding([8, 14]);

                actions = actions.push(launch_button);
            }

            let uninstall_button = if app.project_action_busy {
                button(text("Working...").size(13))
            } else {
                button(text("Remove").size(13))
                    .on_press(Message::UninstallProjectPressed(project.clone()))
            }
            .padding([8, 14]);

            actions = actions.push(uninstall_button);
            list = list.push(project_card(project, actions.into()));
        }

        list.into()
    };

    column![
        responsive_pair(version, loader, layout),
        minecraft_dir,
        rule::horizontal(1),
        search_controls,
        rule::horizontal(1),
        text(search_header).size(16),
        search_results,
        rule::horizontal(1),
        text(installed_header).size(16),
        installed_projects,
    ]
    .spacing(layout.spacing())
    .width(Length::Fill)
    .into()
}

/// A tab button that visually highlights the currently active tab.
fn tab_button(label: &str, tab: Tab, selected: Tab) -> iced::widget::Button<'_, Message> {
    let is_active = tab == selected;

    button(text(label).size(14))
        .on_press(Message::TabSelected(tab))
        .padding([9, 18])
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            if is_active {
                iced::widget::button::Style {
                    background: Some(Background::Color(palette.primary)),
                    text_color: Color::WHITE,
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            } else {
                let hovered = matches!(
                    status,
                    iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
                );
                iced::widget::button::Style {
                    background: if hovered {
                        Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.07)))
                    } else {
                        None
                    },
                    text_color: Color {
                        a: if hovered { 1.0 } else { 0.62 },
                        ..palette.text
                    },
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }
        })
}

/// A card displaying project metadata with a subtle bordered background.
fn project_card<'a>(
    project: &'a ManagedProject,
    actions: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut details = column![
        row![
            column![
                text(&project.title).size(15),
                text(format!("{} • {}", project.source, project.kind))
                    .size(11)
                    .color([0.55, 0.62, 0.76]),
            ]
            .spacing(2)
            .width(Length::Fill),
            actions,
        ]
        .spacing(12)
        .align_y(alignment::Vertical::Center),
    ]
    .spacing(6);

    if !project.description.trim().is_empty() {
        details = details.push(
            text(&project.description)
                .size(13)
                .color([0.70, 0.75, 0.84])
                .wrapping(Wrapping::Word),
        );
    }

    // Compact single-line metadata row: location hint + download count.
    let mut meta: Vec<String> = Vec::new();

    if let Some(hint) = project.location_hint.as_deref() {
        meta.push(hint.to_string());
    }

    if project.downloads > 0 {
        meta.push(format!("↓ {}", format_downloads(project.downloads)));
    }

    if !meta.is_empty() {
        details = details.push(text(meta.join("  ·  ")).size(11).color([0.50, 0.58, 0.72]));
    }

    container(details)
        .padding(14)
        .width(Length::Fill)
        .style(|_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.03))),
            border: Border {
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.10),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn search_task(request: ProjectSearchRequest) -> Task<Message> {
    Task::perform(
        async move { search_projects(request) },
        Message::ProjectsLoaded,
    )
}

fn launch_task(request: LaunchRequest) -> Task<Message> {
    let (sender, receiver) = mpsc::unbounded();

    std::thread::spawn(move || {
        launch_with_progress(request, |event| {
            let _ = sender.unbounded_send(event);
        });
    });

    Task::run(receiver, Message::LaunchEvent)
}

fn selected_version(app: &NovaLauncher) -> Option<String> {
    filtered_version_options(app)
        .iter()
        .find(|version| *version == &app.minecraft_version)
        .cloned()
}

fn filtered_version_options(app: &NovaLauncher) -> Vec<String> {
    app.versions
        .iter()
        .filter(|version| match version.kind {
            VersionKind::Release => app.show_releases,
            VersionKind::Snapshot => app.show_snapshots,
            VersionKind::OldBeta => app.show_old_beta,
            VersionKind::OldAlpha => app.show_old_alpha,
            VersionKind::Other => true,
        })
        .map(|version| version.id.clone())
        .collect()
}

fn version_picker_row(app: &NovaLauncher, layout: LayoutMode) -> Element<'_, Message> {
    let picker = pick_list(
        filtered_version_options(app),
        selected_version(app),
        Message::VersionPicked,
    )
    .placeholder(if app.versions_loading {
        "Loading versions..."
    } else {
        "Select a version"
    })
    .padding(12)
    .width(Length::Fill);

    let refresh = refresh_versions_button(app).width(if layout.is_narrow() {
        Length::Fill
    } else {
        Length::Shrink
    });

    if layout.is_narrow() {
        column![picker, refresh].spacing(8).into()
    } else {
        row![picker, refresh].spacing(8).into()
    }
}

fn version_filters(app: &NovaLauncher) -> Element<'_, Message> {
    let releases = checkbox(app.show_releases)
        .label("Vanilla releases")
        .on_toggle(Message::ShowReleasesChanged);
    let snapshots = checkbox(app.show_snapshots)
        .label("Snapshots")
        .on_toggle(Message::ShowSnapshotsChanged);
    let old_beta = checkbox(app.show_old_beta)
        .label("Old beta")
        .on_toggle(Message::ShowOldBetaChanged);
    let old_alpha = checkbox(app.show_old_alpha)
        .label("Old alpha")
        .on_toggle(Message::ShowOldAlphaChanged);

    row![releases, snapshots, old_beta, old_alpha]
        .spacing(12)
        .wrap()
        .vertical_spacing(8)
        .into()
}

fn responsive_pair<'a>(
    first: Element<'a, Message>,
    second: Element<'a, Message>,
    layout: LayoutMode,
) -> Element<'a, Message> {
    if layout.uses_single_column() {
        column![first, second].spacing(16).into()
    } else {
        row![first, second].spacing(16).into()
    }
}

fn refresh_versions_button(app: &NovaLauncher) -> iced::widget::Button<'_, Message> {
    if app.versions_loading {
        button("...")
    } else {
        button("Refresh").on_press(Message::RefreshVersions)
    }
}

/// Field: a muted label above a control.
fn field<'a>(label: &'a str, control: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![
        text(label).size(12).color([0.60, 0.66, 0.78]),
        control.into(),
    ]
    .spacing(6)
    .width(Length::Fill)
    .into()
}

fn optional_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

/// Human-readable download count: 1 200 000 → "1.2M", 45 000 → "45K", etc.
fn format_downloads(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.0}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[derive(Debug, Clone, Copy)]
enum LayoutMode {
    Narrow,
    Medium,
    Wide,
}

impl LayoutMode {
    fn from_width(width: f32) -> Self {
        if width < 520.0 {
            Self::Narrow
        } else if width < 780.0 {
            Self::Medium
        } else {
            Self::Wide
        }
    }

    fn is_narrow(self) -> bool {
        matches!(self, Self::Narrow)
    }

    fn uses_single_column(self) -> bool {
        matches!(self, Self::Narrow | Self::Medium)
    }

    fn padding(self) -> u16 {
        match self {
            Self::Narrow => 14,
            Self::Medium => 20,
            Self::Wide => 28,
        }
    }

    fn spacing(self) -> u32 {
        match self {
            Self::Narrow => 14,
            Self::Medium => 16,
            Self::Wide => 18,
        }
    }

    fn max_content_width(self) -> u32 {
        match self {
            Self::Narrow => 520,
            Self::Medium => 640,
            Self::Wide => 800,
        }
    }
}
