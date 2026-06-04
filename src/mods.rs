use std::{
    fs::{self, File},
    io,
    path::PathBuf,
};

use reqwest::{Url, blocking::Client, header};
use serde::Deserialize;
use zip::ZipArchive;

use crate::core::LoaderChoice;

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const CURSEFORGE_API: &str = "https://api.curseforge.com";
const MINECRAFT_GAME_ID: u32 = 432;
const CURSEFORGE_MODS_CLASS_ID: u32 = 6;
const CURSEFORGE_MODPACKS_CLASS_ID: u32 = 4471;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSource {
    Modrinth,
    CurseForge,
    Local,
}

impl ProjectSource {
    pub const ALL: [ProjectSource; 2] = [ProjectSource::Modrinth, ProjectSource::CurseForge];
}

impl std::fmt::Display for ProjectSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Modrinth => f.write_str("Modrinth"),
            Self::CurseForge => f.write_str("CurseForge"),
            Self::Local => f.write_str("Local"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Mod,
    Modpack,
}

impl ProjectKind {
    pub const ALL: [ProjectKind; 2] = [ProjectKind::Mod, ProjectKind::Modpack];

    fn modrinth_type(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
        }
    }

    fn curseforge_class_id(self) -> u32 {
        match self {
            Self::Mod => CURSEFORGE_MODS_CLASS_ID,
            Self::Modpack => CURSEFORGE_MODPACKS_CLASS_ID,
        }
    }
}

impl std::fmt::Display for ProjectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mod => f.write_str("Mods"),
            Self::Modpack => f.write_str("Modpacks"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectSearchRequest {
    pub source: ProjectSource,
    pub kind: ProjectKind,
    pub query: String,
    pub minecraft_version: String,
    pub loader: LoaderChoice,
    pub curseforge_api_key: String,
}

#[derive(Debug, Clone)]
pub struct ManagedProject {
    pub source: ProjectSource,
    pub kind: ProjectKind,
    pub id: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
    pub installed_path: Option<PathBuf>,
    pub location_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstallProjectRequest {
    pub project: ManagedProject,
    pub minecraft_dir: PathBuf,
    pub minecraft_version: String,
    pub loader: LoaderChoice,
    pub curseforge_api_key: String,
}

#[derive(Debug, Clone)]
pub struct InstallProjectReport {
    pub filename: String,
    pub destination: PathBuf,
}

pub fn search_projects(
    request: ProjectSearchRequest,
) -> std::result::Result<Vec<ManagedProject>, String> {
    match request.source {
        ProjectSource::Modrinth => search_modrinth(request),
        ProjectSource::CurseForge => search_curseforge(request),
        ProjectSource::Local => Err("Local projects cannot be searched remotely.".to_string()),
    }
}

pub fn install_project(
    request: InstallProjectRequest,
) -> std::result::Result<InstallProjectReport, String> {
    if request.project.kind == ProjectKind::Mod && request.minecraft_version.trim().is_empty() {
        return Err("Select a Minecraft version before installing a mod.".to_string());
    }

    let file = match request.project.source {
        ProjectSource::Modrinth => latest_modrinth_file(&request)?,
        ProjectSource::CurseForge => latest_curseforge_file(&request)?,
        ProjectSource::Local => {
            return Err("Local projects cannot be installed from a remote source.".to_string());
        }
    };

    let destination_dir = match request.project.kind {
        ProjectKind::Mod => request
            .minecraft_dir
            .join("versions")
            .join(request.minecraft_version.trim())
            .join("mods"),
        ProjectKind::Modpack => request.minecraft_dir.join("modpacks"),
    };

    fs::create_dir_all(&destination_dir)
        .map_err(|error| format!("Could not create destination directory: {error}"))?;

    if request.project.kind == ProjectKind::Modpack
        && file.filename.to_lowercase().ends_with(".zip")
    {
        let tmp_dir = std::env::temp_dir();
        let tmp_path = tmp_dir.join(&file.filename);
        download_file(&file.url, &tmp_path)?;

        let pack_name = file
            .filename
            .trim_end_matches(".zip")
            .trim_end_matches(".ZIP")
            .to_string();

        let extract_destination = destination_dir.join(&pack_name);
        fs::create_dir_all(&extract_destination)
            .map_err(|error| format!("Could not create modpack directory: {error}"))?;

        extract_zip(&tmp_path, &extract_destination)
            .map_err(|error| format!("Could not extract modpack: {error}"))?;

        let _ = fs::remove_file(&tmp_path);

        return Ok(InstallProjectReport {
            filename: file.filename,
            destination: extract_destination,
        });
    }

    let destination = destination_dir.join(&file.filename);
    download_file(&file.url, &destination)?;

    Ok(InstallProjectReport {
        filename: file.filename,
        destination,
    })
}

fn extract_zip(zip_path: &PathBuf, destination: &PathBuf) -> std::result::Result<(), io::Error> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let outpath = destination.join(entry.mangled_name());

        if entry.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut outfile = File::create(&outpath)?;
            io::copy(&mut entry, &mut outfile)?;
        }
    }

    Ok(())
}

/// Lists installed mods and modpacks found under the provided Minecraft directory.
pub fn list_installed_projects(minecraft_dir: &PathBuf) -> Vec<ManagedProject> {
    let mut projects = Vec::new();

    let modpacks_dir = minecraft_dir.join("modpacks");
    if let Ok(entries) = fs::read_dir(&modpacks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|segment| segment.to_str()) {
                    projects.push(ManagedProject {
                        source: ProjectSource::Local,
                        kind: ProjectKind::Modpack,
                        id: name.to_string(),
                        title: name.to_string(),
                        description: String::new(),
                        downloads: 0,
                        installed_path: Some(path),
                        location_hint: Some("Profile directory".to_string()),
                    });
                }
            }
        }
    }

    let versions_dir = minecraft_dir.join("versions");
    if let Ok(entries) = fs::read_dir(&versions_dir) {
        for entry in entries.flatten() {
            let version_path = entry.path();
            if !version_path.is_dir() {
                continue;
            }

            let Some(version_name) = version_path
                .file_name()
                .and_then(|segment| segment.to_str())
            else {
                continue;
            };

            let mods_path = version_path.join("mods");
            if let Ok(mod_entries) = fs::read_dir(&mods_path) {
                for mod_entry in mod_entries.flatten() {
                    let path = mod_entry.path();
                    if !path.is_file() {
                        continue;
                    }

                    let Some(filename) = path.file_name().and_then(|segment| segment.to_str())
                    else {
                        continue;
                    };

                    let title = path
                        .file_stem()
                        .and_then(|segment| segment.to_str())
                        .unwrap_or(filename)
                        .to_string();

                    projects.push(ManagedProject {
                        source: ProjectSource::Local,
                        kind: ProjectKind::Mod,
                        id: filename.to_string(),
                        title,
                        description: filename.to_string(),
                        downloads: 0,
                        installed_path: Some(path),
                        location_hint: Some(format!("Version profile: {version_name}")),
                    });
                }
            }
        }
    }

    projects.sort_by(|left, right| {
        kind_sort_rank(left.kind)
            .cmp(&kind_sort_rank(right.kind))
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
            .then_with(|| {
                left.location_hint
                    .as_deref()
                    .unwrap_or_default()
                    .cmp(right.location_hint.as_deref().unwrap_or_default())
            })
    });

    projects
}

/// Uninstalls a managed project by removing files or directories from the minecraft directory.
pub fn uninstall_project(
    project: &ManagedProject,
    minecraft_dir: &PathBuf,
) -> std::result::Result<(), String> {
    if let Some(path) = project.installed_path.as_ref() {
        if !path.exists() {
            return Err(format!(
                "The installed item no longer exists at {}.",
                path.display()
            ));
        }

        if path.is_dir() {
            fs::remove_dir_all(path)
                .map_err(|error| format!("Could not remove project directory: {error}"))?;
        } else {
            fs::remove_file(path)
                .map_err(|error| format!("Could not remove project file: {error}"))?;
        }

        return Ok(());
    }

    match project.kind {
        ProjectKind::Mod => {
            let versions_dir = minecraft_dir.join("versions");
            if versions_dir.exists() {
                if let Ok(entries) = fs::read_dir(&versions_dir) {
                    for entry in entries.flatten() {
                        let mods_path = entry.path().join("mods");
                        let candidate = mods_path.join(&project.id);
                        if candidate.exists() {
                            fs::remove_file(&candidate)
                                .map_err(|error| format!("Could not remove mod file: {error}"))?;
                        }
                    }
                }
            }
        }
        ProjectKind::Modpack => {
            let modpacks_dir = minecraft_dir.join("modpacks");
            let folder = modpacks_dir.join(&project.id);
            if folder.exists() {
                fs::remove_dir_all(&folder)
                    .map_err(|error| format!("Could not remove modpack: {error}"))?;
            } else {
                let zip_file = modpacks_dir.join(format!("{}.zip", project.id));
                if zip_file.exists() {
                    fs::remove_file(&zip_file)
                        .map_err(|error| format!("Could not remove modpack zip: {error}"))?;
                }
            }
        }
    }

    Ok(())
}

fn search_modrinth(
    request: ProjectSearchRequest,
) -> std::result::Result<Vec<ManagedProject>, String> {
    let client = http_client()?;
    let mut facets = vec![format!(
        r#"["project_type:{}"]"#,
        request.kind.modrinth_type()
    )];

    if !request.minecraft_version.trim().is_empty() {
        facets.push(format!(
            r#"["versions:{}"]"#,
            request.minecraft_version.trim()
        ));
    }

    if let Some(loader) = modrinth_loader(request.loader) {
        facets.push(format!(r#"["categories:{loader}"]"#));
    }

    let facets = format!("[{}]", facets.join(","));
    let search_url = format!("{MODRINTH_API}/search");
    let url = Url::parse_with_params(
        search_url.as_str(),
        &[
            ("query", request.query.trim()),
            ("facets", facets.as_str()),
            ("index", "downloads"),
            ("limit", "12"),
        ],
    )
    .map_err(|error| format!("Modrinth search URL invalid: {error}"))?;

    let response: ModrinthSearchResponse = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Modrinth search failed: {error}"))?
        .json()
        .map_err(|error| format!("Modrinth response could not be read: {error}"))?;

    Ok(response
        .hits
        .into_iter()
        .map(|project| ManagedProject {
            source: ProjectSource::Modrinth,
            kind: request.kind,
            id: project.project_id,
            title: project.title,
            description: project.description,
            downloads: project.downloads,
            installed_path: None,
            location_hint: None,
        })
        .collect())
}

fn search_curseforge(
    request: ProjectSearchRequest,
) -> std::result::Result<Vec<ManagedProject>, String> {
    let api_key = require_curseforge_key(&request.curseforge_api_key)?;
    let client = http_client()?;
    let mut query = vec![
        ("gameId", MINECRAFT_GAME_ID.to_string()),
        ("classId", request.kind.curseforge_class_id().to_string()),
        ("searchFilter", request.query.trim().to_string()),
        ("pageSize", "12".to_string()),
        ("sortField", "6".to_string()),
        ("sortOrder", "desc".to_string()),
    ];

    if !request.minecraft_version.trim().is_empty() {
        query.push(("gameVersion", request.minecraft_version.trim().to_string()));
    }

    let curseforge_search_url = format!("{CURSEFORGE_API}/v1/mods/search");
    let url = Url::parse_with_params(curseforge_search_url.as_str(), &query)
        .map_err(|error| format!("CurseForge search URL invalid: {error}"))?;

    let response: CurseForgeSearchResponse = client
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("CurseForge search failed: {error}"))?
        .json()
        .map_err(|error| format!("CurseForge response could not be read: {error}"))?;

    Ok(response
        .data
        .into_iter()
        .map(|project| ManagedProject {
            source: ProjectSource::CurseForge,
            kind: request.kind,
            id: project.id.to_string(),
            title: project.name,
            description: project.summary.unwrap_or_default(),
            downloads: project.download_count.unwrap_or_default() as u64,
            installed_path: None,
            location_hint: None,
        })
        .collect())
}

fn latest_modrinth_file(
    request: &InstallProjectRequest,
) -> std::result::Result<RemoteFile, String> {
    let client = http_client()?;
    let mut query = vec![("include_changelog", "false".to_string())];

    if !request.minecraft_version.trim().is_empty() {
        query.push((
            "game_versions",
            format!(r#"["{}"]"#, request.minecraft_version.trim()),
        ));
    }

    if let Some(loader) = modrinth_loader(request.loader) {
        query.push(("loaders", format!(r#"["{loader}"]"#)));
    }

    let version_url = format!("{MODRINTH_API}/project/{}/version", request.project.id);
    let url = Url::parse_with_params(version_url.as_str(), &query)
        .map_err(|error| format!("Modrinth version URL invalid: {error}"))?;

    let versions: Vec<ModrinthVersion> = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Could not find Modrinth versions: {error}"))?
        .json()
        .map_err(|error| format!("Modrinth version response could not be read: {error}"))?;

    let version = versions
        .into_iter()
        .find(|version| version.version_type == "release")
        .ok_or_else(|| "No compatible release file was found on Modrinth.".to_string())?;

    version
        .files
        .iter()
        .find(|file| file.primary)
        .cloned()
        .or_else(|| version.files.into_iter().next())
        .map(|file| RemoteFile {
            filename: file.filename,
            url: file.url,
        })
        .ok_or_else(|| "The selected Modrinth version has no downloadable files.".to_string())
}

fn latest_curseforge_file(
    request: &InstallProjectRequest,
) -> std::result::Result<RemoteFile, String> {
    let api_key = require_curseforge_key(&request.curseforge_api_key)?;
    let client = http_client()?;
    let mut query = vec![("pageSize", "20".to_string())];

    if !request.minecraft_version.trim().is_empty() {
        query.push(("gameVersion", request.minecraft_version.trim().to_string()));
    }

    let files_url = format!("{CURSEFORGE_API}/v1/mods/{}/files", request.project.id);
    let url = Url::parse_with_params(files_url.as_str(), &query)
        .map_err(|error| format!("CurseForge file list URL invalid: {error}"))?;

    let response: CurseForgeFilesResponse = client
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Could not find CurseForge files: {error}"))?
        .json()
        .map_err(|error| format!("CurseForge files response could not be read: {error}"))?;

    let file = response
        .data
        .into_iter()
        .find(|file| {
            file.download_url
                .as_deref()
                .is_some_and(|url| !url.is_empty())
        })
        .ok_or_else(|| {
            "No direct CurseForge download URL was available for this project.".to_string()
        })?;

    Ok(RemoteFile {
        filename: file.file_name,
        url: file.download_url.unwrap_or_default(),
    })
}

fn download_file(url: &str, destination: &PathBuf) -> std::result::Result<(), String> {
    let client = http_client()?;
    let mut response = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Download failed: {error}"))?;

    let mut file =
        File::create(destination).map_err(|error| format!("Could not create file: {error}"))?;
    response
        .copy_to(&mut file)
        .map_err(|error| format!("Could not write download: {error}"))?;

    Ok(())
}

fn http_client() -> std::result::Result<Client, String> {
    let user_agent = format!("NovaLauncher/{}", env!("CARGO_PKG_VERSION"));

    Client::builder()
        .user_agent(user_agent)
        .default_headers({
            let mut headers = header::HeaderMap::new();
            headers.insert(
                header::ACCEPT,
                header::HeaderValue::from_static("application/json"),
            );
            headers
        })
        .build()
        .map_err(|error| format!("Could not create HTTP client: {error}"))
}

fn require_curseforge_key(key: &str) -> std::result::Result<&str, String> {
    let key = key.trim();

    if key.is_empty() {
        Err("Enter a CurseForge API key before using CurseForge.".to_string())
    } else {
        Ok(key)
    }
}

fn modrinth_loader(loader: LoaderChoice) -> Option<&'static str> {
    match loader {
        LoaderChoice::Vanilla => None,
        LoaderChoice::Fabric => Some("fabric"),
        LoaderChoice::Quilt => Some("quilt"),
        LoaderChoice::Forge => Some("forge"),
        LoaderChoice::NeoForge => Some("neoforge"),
    }
}

fn kind_sort_rank(kind: ProjectKind) -> u8 {
    match kind {
        ProjectKind::Mod => 0,
        ProjectKind::Modpack => 1,
    }
}

#[derive(Debug)]
struct RemoteFile {
    filename: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthSearchHit>,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchHit {
    project_id: String,
    title: String,
    description: String,
    downloads: u64,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    version_type: String,
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize, Clone)]
struct ModrinthFile {
    url: String,
    filename: String,
    primary: bool,
}

#[derive(Debug, Deserialize)]
struct CurseForgeSearchResponse {
    data: Vec<CurseForgeProject>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeProject {
    id: u32,
    name: String,
    summary: Option<String>,
    download_count: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeFilesResponse {
    data: Vec<CurseForgeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFile {
    file_name: String,
    download_url: Option<String>,
}
