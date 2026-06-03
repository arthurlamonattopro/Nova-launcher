use std::{
    env,
    path::PathBuf,
    process::{Command, Stdio},
};

use mc_launcher_core::{
    account::Account,
    command::builder::LaunchOptions,
    install::request::{InstallRequest, JavaInstallPolicy},
    launcher::Launcher,
    loader::common::{LoaderSpec, LoaderVersion},
    progress::{InstallStage, ProgressEvent},
    utils,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderChoice {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl LoaderChoice {
    pub const ALL: [LoaderChoice; 5] = [
        LoaderChoice::Vanilla,
        LoaderChoice::Fabric,
        LoaderChoice::Quilt,
        LoaderChoice::Forge,
        LoaderChoice::NeoForge,
    ];

    fn spec(self) -> Option<LoaderSpec> {
        let version = LoaderVersion::LatestStable;

        match self {
            LoaderChoice::Vanilla => None,
            LoaderChoice::Fabric => Some(LoaderSpec::Fabric { version }),
            LoaderChoice::Quilt => Some(LoaderSpec::Quilt { version }),
            LoaderChoice::Forge => Some(LoaderSpec::Forge { version }),
            LoaderChoice::NeoForge => Some(LoaderSpec::NeoForge { version }),
        }
    }
}

impl std::fmt::Display for LoaderChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            LoaderChoice::Vanilla => "Vanilla",
            LoaderChoice::Fabric => "Fabric",
            LoaderChoice::Quilt => "Quilt",
            LoaderChoice::Forge => "Forge",
            LoaderChoice::NeoForge => "NeoForge",
        };

        f.write_str(label)
    }
}

#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub minecraft_dir: PathBuf,
    pub username: String,
    pub minecraft_version: String,
    pub loader: LoaderChoice,
    pub java_path: Option<PathBuf>,
    pub memory_mb: u16,
}

#[derive(Debug, Clone)]
pub struct LaunchReport {
    pub version_id: String,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinecraftVersion {
    pub id: String,
    pub kind: VersionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionKind {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    Other,
}

impl VersionKind {
    fn from_manifest_type(value: &str) -> Self {
        match value {
            "release" => Self::Release,
            "snapshot" => Self::Snapshot,
            "old_beta" => Self::OldBeta,
            "old_alpha" => Self::OldAlpha,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LaunchProgress {
    Progress { label: String, percent: f32 },
    Finished(std::result::Result<LaunchReport, String>),
}

pub fn default_minecraft_dir() -> PathBuf {
    if let Ok(appdata) = env::var("APPDATA") {
        return PathBuf::from(appdata).join(".nova_launcher");
    }

    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".nova_launcher");
    }

    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".nova_launcher")
}

pub fn list_versions() -> std::result::Result<Vec<MinecraftVersion>, String> {
    utils::get_version_list()
        .map(|versions| {
            versions
                .into_iter()
                .map(|version| MinecraftVersion {
                    id: version.id,
                    kind: VersionKind::from_manifest_type(&version.r#type),
                })
                .collect()
        })
        .map_err(|error| format!("Version list could not be loaded: {error}"))
}

pub fn launch_with_progress(request: LaunchRequest, mut on_progress: impl FnMut(LaunchProgress)) {
    let result = launch(request, &mut on_progress);
    on_progress(LaunchProgress::Finished(result));
}

fn launch(
    request: LaunchRequest,
    on_progress: &mut impl FnMut(LaunchProgress),
) -> std::result::Result<LaunchReport, String> {
    validate_request(&request)?;
    progress(on_progress, "Validated launch settings.", 2.0);

    let launcher = Launcher::new(&request.minecraft_dir);
    let mut tracker = InstallProgress::default();
    let mut reporter = |event: ProgressEvent| {
        if let Some(update) = tracker.observe(event) {
            on_progress(update);
        }
    };

    let install = launcher
        .install_with_progress(
            InstallRequest {
                minecraft_version: request.minecraft_version.clone(),
                loader: request.loader.spec(),
                java: JavaInstallPolicy::Auto,
            },
            &mut reporter,
        )
        .map_err(|error| format!("Install failed: {error}"))?;

    progress(on_progress, "Loading version metadata.", 86.0);
    let version = launcher
        .load_version(&install.version_id)
        .map_err(|error| format!("Version metadata failed to load: {error}"))?;

    progress(on_progress, "Building Java launch command.", 91.0);
    let mut command = launcher
        .build_launch_command_from_version(
            &version,
            LaunchOptions {
                account: Account::offline(request.username.trim()),
                java_executable: request.java_path.clone(),
                launcher_name: "NovaLauncher".to_string(),
                launcher_version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
        )
        .map_err(|error| format!("Launch command could not be built: {error}"))?;

    command
        .args
        .insert(0, format!("-Xmx{}M", request.memory_mb));

    progress(on_progress, "Starting Minecraft process.", 96.0);
    let mut process = Command::new(&command.executable);
    process
        .args(&command.args)
        .current_dir(&command.working_dir)
        .envs(command.env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null());

    let child = process
        .spawn()
        .map_err(|error| format!("Minecraft could not be started: {error}"))?;

    progress(on_progress, "Minecraft started.", 100.0);
    Ok(LaunchReport {
        version_id: install.version_id,
        pid: child.id(),
    })
}

#[derive(Debug)]
struct InstallProgress {
    percent: f32,
}

impl Default for InstallProgress {
    fn default() -> Self {
        Self { percent: 5.0 }
    }
}

impl InstallProgress {
    fn observe(&mut self, event: ProgressEvent) -> Option<LaunchProgress> {
        match event {
            ProgressEvent::StageStarted { stage } => {
                self.percent = self.percent.max(stage_percent(&stage));
                Some(LaunchProgress::Progress {
                    label: format!("Install stage: {}", stage_label(&stage)),
                    percent: self.percent,
                })
            }
            ProgressEvent::TaskStarted { label, .. } => Some(LaunchProgress::Progress {
                label: format!("Downloading {label}"),
                percent: self.percent,
            }),
            ProgressEvent::TaskSkipped { label, .. } => {
                self.percent = (self.percent + 0.7).min(84.0);
                Some(LaunchProgress::Progress {
                    label: format!("Already installed: {label}"),
                    percent: self.percent,
                })
            }
            ProgressEvent::TaskFinished { label } => {
                self.percent = (self.percent + 1.5).min(84.0);
                Some(LaunchProgress::Progress {
                    label: format!("Downloaded {label}"),
                    percent: self.percent,
                })
            }
            ProgressEvent::BytesReceived {
                label,
                received,
                total,
            } => {
                let suffix = total
                    .map(|total| format!(" ({received}/{total} bytes)"))
                    .unwrap_or_else(|| format!(" ({received} bytes)"));

                Some(LaunchProgress::Progress {
                    label: format!("Downloading {label}{suffix}"),
                    percent: self.percent,
                })
            }
        }
    }
}

fn progress(on_progress: &mut impl FnMut(LaunchProgress), label: impl Into<String>, percent: f32) {
    on_progress(LaunchProgress::Progress {
        label: label.into(),
        percent,
    });
}

fn stage_percent(stage: &InstallStage) -> f32 {
    match stage {
        InstallStage::ResolveVersion => 8.0,
        InstallStage::DownloadLibraries => 20.0,
        InstallStage::DownloadAssets => 42.0,
        InstallStage::InstallRuntime => 58.0,
        InstallStage::ExtractNatives => 72.0,
        InstallStage::LoaderInstall => 78.0,
        InstallStage::Verify => 84.0,
    }
}

fn stage_label(stage: &InstallStage) -> &'static str {
    match stage {
        InstallStage::ResolveVersion => "resolving version",
        InstallStage::DownloadLibraries => "downloading libraries",
        InstallStage::DownloadAssets => "downloading assets",
        InstallStage::InstallRuntime => "installing Java runtime",
        InstallStage::ExtractNatives => "extracting natives",
        InstallStage::LoaderInstall => "installing loader",
        InstallStage::Verify => "verifying files",
    }
}

fn validate_request(request: &LaunchRequest) -> std::result::Result<(), String> {
    let username = request.username.trim();
    let version = request.minecraft_version.trim();

    if username.is_empty() {
        return Err("Enter an offline username.".to_string());
    }

    if username.len() > 16
        || !username
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(
            "Minecraft usernames can only use 1-16 letters, numbers, or underscores.".to_string(),
        );
    }

    if version.is_empty() {
        return Err("Enter a Minecraft version, for example 1.20.1.".to_string());
    }

    if request.memory_mb < 512 {
        return Err("Use at least 512 MB of memory.".to_string());
    }

    Ok(())
}
