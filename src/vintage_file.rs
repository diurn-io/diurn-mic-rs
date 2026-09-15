//! The on-disk convention for registry files: what they are named, and where
//! they live.
//!
//! # Why this is in a parsing crate
//!
//! It is the one piece of on-disk policy that every consumer has to agree on.
//! `diurn mic fetch` writes a vintage here and `diurn mic vintages`, `validate`
//! and `diff` read it back; `diurn-ops mic-ingest` picks a download up from the
//! same place. Two implementations of "where is it" means one of them is
//! eventually wrong on somebody's machine, and the failure is a command that
//! cannot find a file another command just wrote.
//!
//! Nothing here walks a directory, opens a file, or fetches. It answers two
//! questions — what is a vintage called, and where do they live — and the
//! crate's scope is otherwise unchanged. **Listing is deliberately not here**:
//! the two consumers disagree about what belongs in a listing (whether undated
//! files appear, and in which order), and those are policies rather than
//! conventions.
//!
//! # A data directory, not a cache directory
//!
//! A pinned vintage is the evidence for what was served on a given day. Caches
//! are something the operating system is entitled to delete without asking, and
//! evidence is not.

use std::path::{Path, PathBuf};

use jiff::civil::Date;

/// The prefix `diurn mic fetch` writes and every consumer reads.
const PREFIX: &str = "ISO10383_MIC_";
const SUFFIX: &str = ".csv";

/// Overrides where fetched registries are kept and looked for.
pub const DATA_DIR_ENV: &str = "DIURN_DATA_DIR";

/// Where fetched registries live, and any reason the obvious answer was not
/// used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    pub path: PathBuf,
    /// Set when an environment variable was deliberately not honoured, so a
    /// caller can explain the choice rather than look arbitrary.
    pub note: Option<String>,
}

/// Nothing on this platform said where a home directory is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("could not determine a data directory; set {DATA_DIR_ENV}")]
pub struct NoDataDir;

/// Which conventions apply.
///
/// A parameter rather than a `cfg!` so every branch is reachable in a test on
/// any host — the Windows and snap paths are exactly the ones that otherwise
/// never run in CI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Windows,
    MacOs,
    Xdg,
}

/// Everything [`data_dir`] reads, named so it can be supplied.
#[derive(Debug, Default, Clone)]
struct Inputs {
    override_dir: Option<PathBuf>,
    appdata: Option<PathBuf>,
    home: Option<PathBuf>,
    xdg_data_home: Option<PathBuf>,
}

/// Where fetched registries live.
///
/// Follows each platform's convention, and [`DATA_DIR_ENV`] overrides all of it.
///
/// ```no_run
/// let dir = diurn_mic::data_dir()?;
/// println!("{}", dir.path.display());
/// # Ok::<(), diurn_mic::NoDataDir>(())
/// ```
pub fn data_dir() -> Result<DataDir, NoDataDir> {
    let platform = if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Xdg
    };

    let inputs = Inputs {
        override_dir: non_empty(DATA_DIR_ENV),
        appdata: non_empty("APPDATA"),
        // Under snap confinement HOME is redirected into the snap's own tree and
        // SNAP_REAL_HOME holds the actual one.
        home: non_empty("SNAP_REAL_HOME").or_else(|| non_empty("HOME")),
        xdg_data_home: non_empty("XDG_DATA_HOME"),
    };
    resolve(platform, &inputs)
}

fn non_empty(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Whether a path lies inside a snap's private per-revision tree.
///
/// Snap user data lives at `~/snap/<app>/<revision>/`, and several snaps — VS
/// Code among them — export `XDG_DATA_HOME` pointing there and leak it into any
/// terminal they spawn. The revision number is the problem: it changes when
/// *that* application updates, so anything stored under it silently disappears
/// on an unrelated upgrade.
fn is_snap_private(p: &Path) -> bool {
    p.components().any(|c| c.as_os_str() == "snap")
}

fn resolve(platform: Platform, inputs: &Inputs) -> Result<DataDir, NoDataDir> {
    let plain = |path: PathBuf| DataDir { path, note: None };

    // The override wins everywhere, including over a platform convention that
    // would otherwise have worked. That is what makes it useful for tests, for
    // air-gapped sites, and for anyone whose home directory is not writable.
    if let Some(dir) = &inputs.override_dir {
        return Ok(plain(dir.clone()));
    }

    match platform {
        Platform::Windows => {
            if let Some(appdata) = &inputs.appdata {
                return Ok(plain(appdata.join("diurn")));
            }
        }
        Platform::MacOs => {
            if let Some(home) = &inputs.home {
                return Ok(plain(home.join("Library/Application Support/diurn")));
            }
        }
        Platform::Xdg => {
            let mut note = None;
            if let Some(xdg) = &inputs.xdg_data_home {
                // A relative XDG_DATA_HOME is invalid per the spec, and honouring
                // one would put the registry somewhere that depends on the
                // working directory.
                if xdg.is_absolute() {
                    if !is_snap_private(xdg) {
                        return Ok(plain(xdg.join("diurn")));
                    }
                    note = Some(format!(
                        "ignoring XDG_DATA_HOME={} — it points inside a snap's \
                         per-revision directory, which would not survive that \
                         application updating. Set {DATA_DIR_ENV} to override.",
                        xdg.display()
                    ));
                }
            }
            if let Some(home) = &inputs.home {
                return Ok(DataDir {
                    path: home.join(".local/share/diurn"),
                    note,
                });
            }
        }
    }
    Err(NoDataDir)
}

/// What a filename says about the file.
///
/// Three outcomes rather than `Option<Date>`, because the two consumers need to
/// tell the middle one apart. A file named like a vintage whose date does not
/// parse is not the same as a file that was never a vintage: the filename is the
/// *only* record of a publication date, so `diurn-ops` treats a malformed one as
/// an error rather than something to skip quietly, while the CLI listing a
/// user's directory has no reason to care. Collapsing them to `None` silently
/// took that choice away from both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VintageName {
    /// Follows the convention, and this is the date it carries.
    Vintage(Date),
    /// Follows the convention's shape, but the date does not parse.
    Malformed,
    /// Not a vintage filename.
    Other,
}

/// What `diurn mic fetch` names a vintage published on `published`.
///
/// The inverse of [`classify_filename`], and they are tested against each other:
/// the date is not inside the CSV, so the name is the only record of it.
///
/// ```
/// use jiff::civil::date;
/// assert_eq!(
///     diurn_mic::filename_for(date(2026, 8, 10)),
///     "ISO10383_MIC_2026-08-10.csv"
/// );
/// ```
pub fn filename_for(published: Date) -> String {
    format!("{PREFIX}{published}{SUFFIX}")
}

/// Read the convention off a path.
///
/// ```
/// use std::path::Path;
/// use diurn_mic::{classify_filename, VintageName};
/// use jiff::civil::date;
///
/// let p = Path::new("/data/ISO10383_MIC_2026-08-10.csv");
/// assert_eq!(classify_filename(p), VintageName::Vintage(date(2026, 8, 10)));
/// assert_eq!(classify_filename(Path::new("notes.csv")), VintageName::Other);
/// ```
pub fn classify_filename(path: &Path) -> VintageName {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return VintageName::Other;
    };
    let Some(date) = name
        .strip_prefix(PREFIX)
        .and_then(|r| r.strip_suffix(SUFFIX))
    else {
        return VintageName::Other;
    };
    match date.parse::<Date>() {
        Ok(published) => VintageName::Vintage(published),
        Err(_) => VintageName::Malformed,
    }
}

/// The publication date a filename carries, if it carries one.
///
/// A convenience over [`classify_filename`] for callers with no opinion about
/// malformed names. If you are about to *use* the date as the authority for what
/// a file contains, prefer the classifier and decide what a malformed name
/// means.
pub fn published_from_filename(path: &Path) -> Option<Date> {
    match classify_filename(path) {
        VintageName::Vintage(published) => Some(published),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(home: &str) -> Inputs {
        Inputs {
            home: Some(PathBuf::from(home)),
            ..Inputs::default()
        }
    }

    use jiff::civil::date;

    #[test]
    fn a_conventional_name_round_trips() {
        for d in [
            date(2026, 8, 10),
            date(2026, 1, 1),
            date(2024, 2, 29),
            date(2035, 12, 31),
        ] {
            let name = filename_for(d);
            assert_eq!(
                classify_filename(Path::new(&name)),
                VintageName::Vintage(d),
                "{name}"
            );
        }
    }

    #[test]
    fn the_date_is_read_out_of_a_full_path() {
        assert_eq!(
            published_from_filename(Path::new("/srv/x/ISO10383_MIC_2026-08-10.csv")),
            Some(date(2026, 8, 10))
        );
    }

    /// The distinction the three-way result exists for. Both of these yield
    /// `None` from [`published_from_filename`], and only one of them is a file
    /// somebody meant to be a vintage.
    #[test]
    fn a_malformed_date_is_not_the_same_as_not_a_vintage() {
        for name in [
            "ISO10383_MIC_2026-13-01.csv",
            "ISO10383_MIC_2026-08.csv",
            "ISO10383_MIC_.csv",
            "ISO10383_MIC_yesterday.csv",
        ] {
            assert_eq!(
                classify_filename(Path::new(name)),
                VintageName::Malformed,
                "{name}"
            );
            assert_eq!(published_from_filename(Path::new(name)), None, "{name}");
        }

        for name in [
            "mic.csv",
            "ISO10383_MIC.csv",
            "notes.txt",
            "ISO10383_MIC_2026-08-10.csv.bak",
            // The CLI accepts a trailing date on any name as a convenience for
            // files a user supplied by hand. That is a local affordance, not
            // this convention.
            "mic_2026-08-10.csv",
        ] {
            assert_eq!(
                classify_filename(Path::new(name)),
                VintageName::Other,
                "{name}"
            );
        }
    }

    #[test]
    fn a_path_with_no_filename_is_not_a_vintage() {
        assert_eq!(classify_filename(Path::new("/")), VintageName::Other);
        assert_eq!(classify_filename(Path::new("..")), VintageName::Other);
    }

    #[test]
    fn the_override_wins_on_every_platform() {
        for platform in [Platform::Windows, Platform::MacOs, Platform::Xdg] {
            let i = Inputs {
                override_dir: Some(PathBuf::from("/srv/vintages")),
                appdata: Some(PathBuf::from("C:/Users/x/AppData/Roaming")),
                home: Some(PathBuf::from("/home/x")),
                xdg_data_home: Some(PathBuf::from("/home/x/.local/share")),
            };
            let d = resolve(platform, &i).expect("resolves");
            assert_eq!(d.path, PathBuf::from("/srv/vintages"), "{platform:?}");
            assert!(d.note.is_none());
        }
    }

    #[test]
    fn each_platform_uses_its_own_convention() {
        let mac = resolve(Platform::MacOs, &inputs("/Users/x")).expect("resolves");
        assert_eq!(
            mac.path,
            PathBuf::from("/Users/x/Library/Application Support/diurn")
        );

        let xdg = resolve(Platform::Xdg, &inputs("/home/x")).expect("resolves");
        assert_eq!(xdg.path, PathBuf::from("/home/x/.local/share/diurn"));

        let win = resolve(
            Platform::Windows,
            &Inputs {
                appdata: Some(PathBuf::from("C:/Users/x/AppData/Roaming")),
                ..Inputs::default()
            },
        )
        .expect("resolves");
        assert_eq!(win.path, PathBuf::from("C:/Users/x/AppData/Roaming/diurn"));
    }

    #[test]
    fn xdg_data_home_is_honoured_when_it_is_absolute() {
        let i = Inputs {
            xdg_data_home: Some(PathBuf::from("/opt/share")),
            ..inputs("/home/x")
        };
        let d = resolve(Platform::Xdg, &i).expect("resolves");
        assert_eq!(d.path, PathBuf::from("/opt/share/diurn"));
        assert!(d.note.is_none());
    }

    /// A relative value is invalid per the XDG spec, and obeying it would make
    /// the answer depend on the working directory.
    #[test]
    fn a_relative_xdg_data_home_falls_back_to_home() {
        let i = Inputs {
            xdg_data_home: Some(PathBuf::from("share")),
            ..inputs("/home/x")
        };
        let d = resolve(Platform::Xdg, &i).expect("resolves");
        assert_eq!(d.path, PathBuf::from("/home/x/.local/share/diurn"));
    }

    /// The case worth having a test for: a snap-exported `XDG_DATA_HOME` points
    /// at a per-revision tree that vanishes when *that* application updates.
    #[test]
    fn a_snap_private_xdg_data_home_is_refused_and_explained() {
        let i = Inputs {
            xdg_data_home: Some(PathBuf::from("/home/x/snap/code/158/.local/share")),
            ..inputs("/home/x")
        };
        let d = resolve(Platform::Xdg, &i).expect("resolves");
        assert_eq!(d.path, PathBuf::from("/home/x/.local/share/diurn"));
        let note = d.note.expect("the refusal is explained");
        assert!(note.contains("snap"), "{note}");
        assert!(note.contains(DATA_DIR_ENV), "{note}");
    }

    #[test]
    fn nothing_to_go_on_is_an_error_rather_than_a_guess() {
        assert_eq!(resolve(Platform::Xdg, &Inputs::default()), Err(NoDataDir));
        assert_eq!(
            resolve(Platform::Windows, &Inputs::default()),
            Err(NoDataDir)
        );
    }
}
