use std::path::PathBuf;

use iced::{
    Element, Length, Size, Task, Theme, alignment,
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
    LoaderChanged(LoaderChoice),
    JavaPathChanged(String),
    MinecraftDirChanged(String),
    MemoryChanged(u16),
    LaunchPressed,
    LaunchEvent(LaunchProgress),
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
    progress_percent: f32,
    busy: bool,
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
            progress_percent: 0.0,
            busy: false,
        }
    }
}

pub fn run() -> iced::Result {
    iced::application(boot, update, view)
        .title("NovaLauncher")
        .theme(Theme::Dark)
        .window(window::Settings {
            size: Size::new(820.0, 640.0),
            min_size: Some(Size::new(360.0, 480.0)),
            ..Default::default()
        })
        .centered()
        .run()
}

fn boot() -> (NovaLauncher, Task<Message>) {
    (
        NovaLauncher::default(),
        Task::perform(async { list_versions() }, Message::VersionsLoaded),
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
                    app.status = format!("Loaded {count} Minecraft versions.");
                }
                Err(error) => {
                    app.status = error;
                }
            }
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
            app.status = "Starting install and launch workflow...".to_string();

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
        Message::LaunchEvent(event) => match event {
            LaunchProgress::Progress { label, percent } => {
                app.progress_percent = percent.clamp(0.0, 100.0);
                app.status = label;
            }
            LaunchProgress::Finished(result) => {
                app.busy = false;
                app.status = match result {
                    Ok(report) => {
                        app.progress_percent = 100.0;
                        format!(
                            "Launched {} successfully. Java process id: {}.",
                            report.version_id, report.pid
                        )
                    }
                    Err(error) => {
                        app.progress_percent = 0.0;
                        error
                    }
                };
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
    let title = text("NovaLauncher").size(36);
    let subtitle = text("Offline Minecraft launcher powered by iced and mc-launcher-core.")
        .size(16)
        .wrapping(Wrapping::Word)
        .color([0.72, 0.76, 0.82]);

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
        text(format!("Memory: {} MB", app.memory_mb)).size(14),
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

    let launch_button = if app.busy {
        button(text("Working...").align_x(alignment::Horizontal::Center))
    } else {
        button(text("Install and Launch").align_x(alignment::Horizontal::Center))
            .on_press(Message::LaunchPressed)
    }
    .padding([12, 18])
    .width(Length::Fill);

    let progress = column![
        row![
            text("Progress").size(14),
            text(format!("{:.0}%", app.progress_percent)).size(14),
        ]
        .spacing(12),
        progress_bar(0.0..=100.0, app.progress_percent),
    ]
    .spacing(8);

    let identity = responsive_pair(username, version, layout);
    let runtime = responsive_pair(loader, memory.into(), layout);

    let content = column![
        title,
        subtitle,
        rule::horizontal(1),
        identity,
        runtime,
        java_path,
        minecraft_dir,
        progress,
        launch_button,
        text(&app.status)
            .size(14)
            .wrapping(Wrapping::Word)
            .color(if app.busy {
                [0.84, 0.78, 0.48]
            } else {
                [0.62, 0.82, 0.70]
            }),
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

fn field<'a>(label: &'a str, control: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(14), control.into()]
        .spacing(8)
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
            Self::Wide => 780,
        }
    }
}
