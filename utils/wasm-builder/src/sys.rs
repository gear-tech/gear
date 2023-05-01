use std::{ffi::OsStr, path::PathBuf};
use sysinfo::{ProcessExt, System, SystemExt};

/// Executes closure over currently running build command.
pub fn with_current_command<T>(f: impl FnOnce(&[String]) -> T) -> T {
    let mut sys = System::new();

    let build_script_pid = sysinfo::get_current_pid().expect("Infallible");
    assert!(sys.refresh_process(build_script_pid));
    let build_script_process = sys.process(build_script_pid).expect("Infallible");

    let build_pid = build_script_process.parent().expect("Infallible");
    assert!(sys.refresh_process(build_pid));
    let build_process = sys.process(build_pid).expect("Infallible");

    f(build_process.cmd())
}

/// Struct representing used for build toolchain.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Toolchain(String);

impl Toolchain {
    /// Extracts `Toolchain` from cargo executable path.
    ///
    /// WARNING: No validation for argument provided. Avoid incorrect usage.
    pub fn from_cargo_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();

        // Cargo path format:
        // "$RUSTUP_HOME/toolchains/**toolchain_name**/bin/cargo"
        let toolchain_name: &str = path
            .iter()
            .nth_back(2)
            .and_then(OsStr::to_str)
            .expect("Infallible");

        // Toolchain name format:
        // "**toolchain**-arch-arch-arch"
        let toolchain = toolchain_name
            .rsplitn(4, '-')
            .last()
            .map(String::from)
            .expect("Infallible");

        Self(toolchain)
    }

    /// Extracts `Toolchain` from currently running build process.
    pub fn from_command(cmd: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let cargo_path = cmd.into_iter().next().expect("Infallible");

        Self::from_cargo_path(cargo_path.as_ref())
    }

    /// Returns ref string representing toolchain in CLI cargo parameters.
    ///
    /// Equals one of the following variants:
    /// * "stable"
    /// * "nightly"
    /// * "nightly-yyyy-mm-dd"
    #[allow(unused)]
    pub fn toolchain_string(&'_ self) -> &'_ str {
        self.0.as_ref()
    }

    /// Returns string representing toolchain in CLI cargo parameters,
    /// consuming self object.
    ///
    /// Equals one of the following variants:
    /// * "stable"
    /// * "nightly"
    /// * "nightly-yyyy-mm-dd"
    pub fn into_toolchain_string(self) -> String {
        self.0
    }

    /// Returns ref string representing nightly toolchain in CLI cargo param.
    ///
    /// Equals one of the following variants:
    /// * "+nightly" (for case of initially +stable or +nightly used)
    /// * "+nightly-yyyy-mm-dd"
    #[allow(unused)]
    pub fn nightly_toolchain_string(&'_ self) -> &'_ str {
        if self.is_stable() {
            "nightly"
        } else {
            self.toolchain_string()
        }
    }

    /// Returns string representing nightly toolchain in CLI cargo param,
    /// consuming self object.
    ///
    /// Equals one of the following variants:
    /// * "+nightly" (for case of initially +stable or +nightly used)
    /// * "+nightly-yyyy-mm-dd"
    pub fn into_nightly_toolchain_string(self) -> String {
        if self.is_stable() {
            self.into_toolchain_string()
                .replacen("stable", "nightly", 1)
        } else {
            self.into_toolchain_string()
        }
    }

    /// Returns bool representing if "+stable" compiler used.
    pub fn is_stable(&self) -> bool {
        self.0.starts_with("stable")
    }
}

/// Struct representing specific feature set for compilation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SpecificFeatureSet {
    no_default_features: bool,
    features: Vec<String>,
}

impl SpecificFeatureSet {
    fn new() -> Self {
        Self {
            no_default_features: false,
            features: Vec::new(),
        }
    }

    fn add_feature(&mut self, feature: impl ToString) {
        let feature: String = feature.to_string().replace('"', "");

        if !feature.is_empty() && !self.features.contains(&feature) {
            self.features.push(feature)
        }
    }

    fn add_features(
        &mut self,
        crate_name: impl AsRef<str>,
        features: impl IntoIterator<Item = impl ToString>,
    ) {
        for feature in features.into_iter() {
            let feature: String = feature.to_string().replace('"', "");

            let feature = feature
                .split_once('/')
                .map(|(name, feat)| {
                    (name == crate_name.as_ref())
                        .then(|| feat.to_string())
                        .unwrap_or_default()
                })
                .unwrap_or(feature);

            if !feature.is_empty() && !self.features.contains(&feature) {
                self.features.push(feature)
            }
        }
    }

    fn remove_feature(&mut self, feature: impl AsRef<str>) {
        self.features.retain(|e| e != feature.as_ref());
    }

    fn filter_existing(&mut self, existing_features: impl IntoIterator<Item = impl AsRef<str>>) {
        let mut existing_features = existing_features.into_iter();

        self.features
            .retain(|e| e.contains('/') || existing_features.any(|f| f.as_ref() == e))
    }
}

/// Struct representing feature set for compilation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeatureSet {
    AllFeatures,
    SomeFeatures(SpecificFeatureSet),
}

impl FeatureSet {
    /// Returns bool representing if `--all-features` should be used.
    pub fn all_features(&self) -> bool {
        matches!(self, Self::AllFeatures)
    }

    pub fn convert_all_features(&mut self, features: Vec<String>) {
        assert!(self.all_features(), "Incorrect usage");

        *self = Self::SomeFeatures(SpecificFeatureSet {
            no_default_features: true,
            features,
        });
    }

    /// Returns bool representing if `--no-default-features` should be used.
    pub fn no_default_features(&self) -> bool {
        let Self::SomeFeatures(SpecificFeatureSet { no_default_features, .. }) = self else {
            return false
        };

        *no_default_features
    }

    /// Explicitly adds feature without checks.
    pub fn add_feature(&mut self, feature: impl ToString) {
        let Self::SomeFeatures(specific_feature_set) = self else { return };

        specific_feature_set.add_feature(feature);
    }

    /// Explicitly removes feature, if present.
    pub fn remove_feature(&mut self, feature: impl AsRef<str>) {
        let Self::SomeFeatures(specific_feature_set) = self else { return };

        specific_feature_set.remove_feature(feature);
    }

    pub fn filter_existing(
        &mut self,
        existing_features: impl IntoIterator<Item = impl AsRef<str>>,
    ) {
        let Self::SomeFeatures(specific_feature_set) = self else { return };

        specific_feature_set.filter_existing(existing_features);
    }

    /// Returns Vec of features, if any should be used.
    pub fn features(&self) -> Option<&Vec<String>> {
        let Self::SomeFeatures(SpecificFeatureSet { features, .. }) = self else {
            return None
        };

        (!features.is_empty()).then_some(features)
    }

    /// Returns string of concatenated with comma features, if any should be used.
    pub fn features_string(&self) -> Option<String> {
        self.features().map(|v| v.join(","))
    }
}

/// Builder for feature set used to compile underlying program.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FeatureSetBuilder(String);

impl FeatureSetBuilder {
    /// Creates builder from crate's name.
    ///
    /// Crate name used for handling correctness in case of workspace building.
    pub fn from_crate_name(crate_name: impl ToString) -> Self {
        Self(crate_name.to_string())
    }

    /// Returns crate name associated with this builder.
    pub fn crate_name(&self) -> &str {
        self.0.as_ref()
    }

    /// Returns feature set associated with underlying crate
    /// based on given executable command.
    pub fn build_from_command(&self, cmd: impl IntoIterator<Item = impl AsRef<str>>) -> FeatureSet {
        let mut feature_set = SpecificFeatureSet::new();
        let mut cmd = cmd.into_iter();

        while let Some(mut arg) = cmd.next() {
            let mut features = None;

            match arg.as_ref() {
                "--" => break,
                "--all-features" => return FeatureSet::AllFeatures,
                "--no-default-features" => feature_set.no_default_features = true,
                "-F" | "--features" => {
                    arg = cmd.next().expect("Infallible");
                    features = Some(arg.as_ref());
                }
                other => {
                    features = other
                        .strip_prefix("-F=")
                        .or_else(|| other.strip_prefix("--features="))
                }
            };

            if let Some(features) = features {
                feature_set.add_features(self.crate_name(), features.replace('"', "").split(','))
            };
        }

        FeatureSet::SomeFeatures(feature_set)
    }
}

#[test]
fn toolchain_parsing() {
    let toolchains = ["stable", "nightly", "nightly-2023-01-08"];

    for toolchain in toolchains {
        let command = format!(
            "/Users/user/.rustup/toolchains/{toolchain}-aarch64-apple-darwin/bin/cargo build"
        );
        let cmd = command.split(' ').collect::<Vec<_>>();

        assert_eq!(toolchain, Toolchain::from_command(cmd).toolchain_string())
    }
}

#[test]
fn features_parsing() {
    // All types of features specification with complications: repetitions and extra arguments between.
    //
    // NOTE: `-F crate-a/"feat1,feat2"` equals `-F crate-a/feat1 -F feat2`
    let command = r#"
        cargo build
            --features a
            --release
            -F ax
            --features b,c,d
            -F bx,cx,dx
            --features "e"
            -F "ex"
            --features "f,g,h"
            -F "fx,gx,hx"
            --features i,
            -F ix,
            -F ix,
            -F ""
            --no-default-features
            --features=j
            -F=jx
            --features=k,l,m
            -F=kx,lx,mx
            --features="n"
            -F="nx"
            --features="o,p,q"
            --features="o,p,q"
            -F="ox,px,qx"
            --features=r,
            -F=rx,

            --features=s,this-crate/t
            --features=other-crate/u
            -F other-crate/v,w
            -F other-crate/"x,y"
            -F another-crate/"z"
            -F this-crate/""
    "#
    .replace('\n', "");
    let command = command.split(' ').collect::<Vec<_>>();

    let feature_set_builder = FeatureSetBuilder::from_crate_name("this-crate");
    let feature_set = feature_set_builder.build_from_command(command);

    assert!(!feature_set.all_features());
    assert!(feature_set.no_default_features());
    assert_eq!(
        feature_set.features_string().expect("Infallible"),
        "a,ax,b,c,d,bx,cx,dx,e,ex,f,g,h,fx,gx,hx,i,ix,j,jx,k,l,m,kx,lx,mx,n,nx,o,p,q,ox,px,qx,r,rx,s,t,w,y"
    );
}

#[test]
fn all_features_parsing() {
    let command = "cargo build --no-default-features --features a --all-features -F b"
        .split(' ')
        .collect::<Vec<_>>();

    let feature_set_builder = FeatureSetBuilder::from_crate_name("some_crate");
    let feature_set = feature_set_builder.build_from_command(command);

    assert!(feature_set.all_features());
    assert!(!feature_set.no_default_features());
    assert!(feature_set.features_string().is_none());
}

// Test for demonstration of parsing command.
#[test]
fn demonstration() {
    with_current_command(|cmd| {
        println!("Executed command = {cmd:?}");
        let toolchain = Toolchain::from_command(cmd).into_toolchain_string();
        println!("Cargo toolchain: {toolchain:?}");
    })
}
