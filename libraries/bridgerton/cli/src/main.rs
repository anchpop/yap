//! `cargo bridgerton`: one build entry point for both platforms.
//!
//! `web` wraps wasm-pack for the JavaScript package. `swift` builds a package's
//! library, loads the host build to run the bridge's Swift generator (or runs
//! it on an iOS target), and records build metadata. `package` turns one or
//! more `swift` outputs into an XCFramework plus a Swift package.
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser)]
#[command(
    name = "cargo-bridgerton",
    bin_name = "cargo bridgerton",
    version,
    about
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build the JavaScript package with wasm-pack.
    Web(Web),
    /// Build a library and generate its Swift bindings.
    Swift(Swift),
    /// Package generated Swift bindings as an XCFramework and Swift package.
    Package(Package),
}

#[derive(Args)]
struct Build {
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,
    /// The workspace package to build.
    #[arg(long)]
    package: String,
    #[arg(long)]
    release: bool,
    /// Additional Cargo features (repeatable).
    #[arg(long)]
    features: Vec<String>,
    #[arg(long)]
    no_default_features: bool,
    /// Require Cargo.lock to be up to date.
    #[arg(long)]
    locked: bool,
}

#[derive(Args)]
struct Web {
    #[command(flatten)]
    build: Build,
    /// wasm-pack target: web, nodejs, or bundler.
    #[arg(long, default_value = "bundler")]
    target: String,
    /// Output directory; defaults to `pkg` inside the crate.
    #[arg(long)]
    out_dir: Option<PathBuf>,
}

#[derive(Args)]
struct Swift {
    #[command(flatten)]
    build: Build,
    #[arg(long)]
    out_dir: PathBuf,
    /// Rust target triple; cross-target metadata is executed on that target.
    #[arg(long)]
    target: Option<String>,
    /// Booted simulator UUID for target metadata execution.
    #[arg(long)]
    simulator: Option<String>,
    /// Target runner command; {executable} and {output} are substituted as individual arguments.
    #[arg(long)]
    runner: Option<String>,
}

#[derive(Args)]
struct Package {
    /// Generator output for one target (repeatable).
    #[arg(long, required = true)]
    bindings: Vec<PathBuf>,
    #[arg(long)]
    module: String,
    #[arg(long)]
    out_dir: PathBuf,
}

fn main() -> ExitCode {
    // Invoked as `cargo bridgerton ...` the first argument is the subcommand name itself.
    let mut args: Vec<OsString> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "bridgerton") {
        args.remove(1);
    }
    let result = match Cli::parse_from(args).command {
        Cmd::Web(web) => web.run(),
        Cmd::Swift(swift) => swift.run(),
        Cmd::Package(package) => package.run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: &mut Command) -> Result<()> {
    let display = std::iter::once(command.get_program().to_string_lossy().into_owned())
        .chain(
            command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned()),
        )
        .collect::<Vec<_>>()
        .join(" ");
    println!("+ {display}");
    let status = command.status()?;
    if !status.success() {
        return Err(format!("command failed with {status}: {display}").into());
    }
    Ok(())
}

fn output(command: &mut Command) -> Result<String> {
    let output = command.stderr(Stdio::inherit()).output()?;
    if !output.status.success() {
        return Err(format!("command failed with {}", output.status).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

struct Metadata {
    package_id: String,
    manifest_dir: PathBuf,
    target_directory: PathBuf,
}

impl Build {
    fn metadata(&self) -> Result<Metadata> {
        let mut command = Command::new("cargo");
        command
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .arg("--manifest-path")
            .arg(&self.manifest_path);
        if self.locked {
            command.arg("--locked");
        }
        let metadata: Value = serde_json::from_str(&output(&mut command)?)?;
        let packages = metadata["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|package| package["name"] == self.package)
            .collect::<Vec<_>>();
        let [package] = packages[..] else {
            return Err(format!(
                "--package must name one package in the workspace: {}",
                self.package
            )
            .into());
        };
        Ok(Metadata {
            package_id: package["id"].as_str().unwrap_or_default().to_owned(),
            manifest_dir: Path::new(package["manifest_path"].as_str().unwrap_or_default())
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            target_directory: PathBuf::from(
                metadata["target_directory"].as_str().unwrap_or_default(),
            ),
        })
    }

    fn cargo_flags(&self, command: &mut Command) {
        if self.locked {
            command.arg("--locked");
        }
        if self.no_default_features {
            command.arg("--no-default-features");
        }
        for feature in &self.features {
            command.args(["--features", feature]);
        }
    }
}

impl Web {
    fn run(self) -> Result<()> {
        let metadata = self.build.metadata()?;
        let mut command = Command::new("wasm-pack");
        command
            .arg("build")
            .arg(&metadata.manifest_dir)
            .args(["--target", &self.target]);
        if let Some(out_dir) = &self.out_dir {
            command.arg("--out-dir").arg(out_dir);
        }
        command.arg(if self.build.release {
            "--release"
        } else {
            "--dev"
        });
        command.arg("--");
        self.build.cargo_flags(&mut command);
        run(&mut command)
    }
}

/// Mirrors the C ABI of the bridge's generator entry point.
#[repr(C)]
struct Buffer {
    data: *mut u8,
    len: usize,
}
#[repr(C)]
struct BridgeResult {
    handle: *const std::ffi::c_void,
    value: u32,
    status: u32,
    data: Buffer,
}

fn generate(library: &Path, output: &Path) -> Result<()> {
    type Generate = unsafe extern "C" fn(*const u8, usize) -> BridgeResult;
    type Free = unsafe extern "C" fn(Buffer);
    // The library stays loaded until Rust has freed the result buffer.
    let library = unsafe { libloading::Library::new(library)? };
    let entry: libloading::Symbol<Generate> = unsafe { library.get(b"bridgerton_generate_v1")? };
    let release: libloading::Symbol<Free> = unsafe { library.get(b"bridgerton_buffer_free")? };
    let path = output
        .canonicalize()
        .unwrap_or_else(|_| output.to_path_buf());
    let path = path.to_string_lossy();
    let result = unsafe { entry(path.as_ptr(), path.len()) };
    let message = if result.status != 0 && !result.data.data.is_null() {
        let bytes = unsafe { std::slice::from_raw_parts(result.data.data, result.data.len) };
        Some(String::from_utf8_lossy(bytes).into_owned())
    } else {
        None
    };
    unsafe { release(result.data) };
    match message {
        Some(message) => Err(message.into()),
        None if result.status != 0 => Err("Swift generation failed".into()),
        None => Ok(()),
    }
}

fn sha256(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(fs::read(path)?);
    Ok(format!("{:x}", hasher.finalize()))
}

fn host_triple() -> Result<String> {
    output(Command::new("rustc").arg("-vV"))?
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        .ok_or_else(|| "rustc did not report its host".into())
}

fn xcrun_sdk_path(sdk: &str) -> Result<String> {
    Ok(
        output(Command::new("xcrun").args(["--sdk", sdk, "--show-sdk-path"]))?
            .trim()
            .to_owned(),
    )
}

impl Swift {
    fn run(self) -> Result<()> {
        let metadata = self.build.metadata()?;
        let host = host_triple()?;
        let cross = self.target.as_ref().is_some_and(|target| *target != host);
        if cross && self.runner.is_none() && self.simulator.is_none() {
            return Err("cross-target generation requires --runner or --simulator; host metadata is not a substitute for target metadata".into());
        }
        if self.simulator.is_some() && self.target.as_deref() != Some("aarch64-apple-ios-sim") {
            return Err("--simulator requires --target aarch64-apple-ios-sim".into());
        }
        let mut command = Command::new("cargo");
        command
            .args(["rustc", "--manifest-path"])
            .arg(&self.build.manifest_path)
            .args(["--package", &self.build.package, "--lib"])
            .arg("--message-format=json-render-diagnostics");
        if let Some(target) = &self.target {
            command.args(["--target", target]);
        }
        self.build.cargo_flags(&mut command);
        if self.build.release {
            command.arg("--release");
        }
        command.args(["--", "--print=native-static-libs"]);
        println!(
            "+ {}",
            std::iter::once("cargo".to_owned())
                .chain(command.get_args().map(|a| a.to_string_lossy().into_owned()))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let built = command.stderr(Stdio::piped()).output()?;
        let stderr = String::from_utf8_lossy(&built.stderr).into_owned();
        eprint!("{stderr}");
        if !built.status.success() {
            return Err("cargo build failed".into());
        }
        let mut archives = Vec::new();
        let mut libraries = Vec::new();
        for line in String::from_utf8_lossy(&built.stdout).lines() {
            let Ok(artifact) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if artifact["reason"] != "compiler-artifact"
                || artifact["package_id"] != metadata.package_id
            {
                continue;
            }
            for name in artifact["filenames"].as_array().into_iter().flatten() {
                let path = PathBuf::from(name.as_str().unwrap_or_default());
                match path.extension().and_then(|e| e.to_str()) {
                    Some("a") => archives.push(path),
                    Some("dylib" | "so" | "dll") => libraries.push(path),
                    _ => {}
                }
            }
        }
        let native_notes = stderr
            .lines()
            .filter_map(|line| line.strip_prefix("note: native-static-libs: "))
            .collect::<Vec<_>>();
        if !archives.is_empty() && native_notes.len() != 1 {
            return Err(
                "expected Rust to report the static library's native link dependencies".into(),
            );
        }
        let native_libraries = native_notes
            .first()
            .map(|note| shell_words(note))
            .transpose()?
            .unwrap_or_default();
        fs::create_dir_all(&self.out_dir)?;
        if cross {
            let [archive] = &archives[..] else {
                return Err("target metadata execution requires a staticlib crate-type".into());
            };
            self.generate_on_target(archive, &native_libraries)?;
        } else {
            let [library] = &libraries[..] else {
                return Err("expected one host-native cdylib; add cdylib to the package's crate-type and build for this host".into());
            };
            generate(library, &self.out_dir)?;
        }
        let absolute = |path: &PathBuf| path.canonicalize().unwrap_or_else(|_| path.clone());
        let archive_hashes = archives
            .iter()
            .map(|path| Ok((absolute(path).to_string_lossy().into_owned(), sha256(path)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let info = json!({
            "target": self.target.clone().unwrap_or(host),
            "package": self.build.package,
            "features": self.build.features,
            "default_features": !self.build.no_default_features,
            "release": self.build.release,
            "libraries": libraries.iter().map(|p| absolute(p).to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "archives": archives.iter().map(|p| absolute(p).to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "native_static_libraries": native_libraries,
            "archive_sha256": archive_hashes,
        });
        fs::write(
            self.out_dir.join("build.json"),
            format!("{}\n", serde_json::to_string_pretty(&info)?),
        )?;
        let _ = metadata.target_directory;
        println!(
            "Generated Swift bindings in {}",
            absolute(&self.out_dir).display()
        );
        Ok(())
    }

    /// Link the bridge's C entry point against the target archive and run it on that target.
    fn generate_on_target(&self, archive: &Path, native_libraries: &[String]) -> Result<()> {
        let target = self.target.as_deref().unwrap_or_default();
        let (sdk, triple) = match target {
            "aarch64-apple-ios-sim" => ("iphonesimulator", "arm64-apple-ios18.0-simulator"),
            "aarch64-apple-ios" => ("iphoneos", "arm64-apple-ios18.0"),
            _ => return Err("metadata runner linking currently supports Apple iOS targets".into()),
        };
        let temporary = tempdir("bridgerton-metadata-")?;
        let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
        let executable = temporary.join("metadata");
        let mut clang = Command::new("xcrun");
        clang
            .args([
                "clang",
                "-target",
                triple,
                "-isysroot",
                &xcrun_sdk_path(sdk)?,
            ])
            .arg("-I")
            .arg(&runtime)
            .arg(runtime.join("generate.c"))
            .arg(format!("-Wl,-force_load,{}", archive.display()))
            .args(native_libraries)
            .arg("-o")
            .arg(&executable);
        run(&mut clang)?;
        let target_output = temporary.join("generated");
        let mut runner = if let Some(simulator) = &self.simulator {
            let mut command = Command::new("xcrun");
            command
                .args(["simctl", "spawn", simulator])
                .arg(&executable)
                .arg(&target_output);
            command
        } else {
            let parts = shell_words(self.runner.as_deref().unwrap_or_default())?
                .into_iter()
                .map(|part| {
                    part.replace("{executable}", &executable.to_string_lossy())
                        .replace("{output}", &target_output.to_string_lossy())
                })
                .collect::<Vec<_>>();
            let (program, rest) = parts.split_first().ok_or("empty --runner")?;
            let mut command = Command::new(program);
            command.args(rest);
            command
        };
        run(&mut runner)?;
        copy_tree(&target_output, &self.out_dir)?;
        fs::remove_dir_all(&temporary)?;
        Ok(())
    }
}

const APPLE: &[(&str, &str, &str, &str, &str, &str)] = &[
    // (target, sdk, triple, swift condition, platform, minimum)
    (
        "aarch64-apple-darwin",
        "macosx",
        "arm64-apple-macos15.0",
        "os(macOS) && arch(arm64)",
        "MacOSX",
        "15.0",
    ),
    (
        "aarch64-apple-ios-sim",
        "iphonesimulator",
        "arm64-apple-ios18.0-simulator",
        "os(iOS) && targetEnvironment(simulator) && arch(arm64)",
        "iPhoneSimulator",
        "18.0",
    ),
    (
        "aarch64-apple-ios",
        "iphoneos",
        "arm64-apple-ios18.0",
        "os(iOS) && !targetEnvironment(simulator) && arch(arm64)",
        "iPhoneOS",
        "18.0",
    ),
];

fn version_key(version: &str) -> (u32, u32, u32) {
    let mut parts = version.split('.').map(|p| p.parse().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// Native C/C++ dependencies can require a newer OS than Rust itself. Read
/// every archive member rather than advertising an unsupported package minimum.
fn deployment_minimum(archive: &str, baseline: &str) -> Result<String> {
    let listing = output(Command::new("xcrun").args(["otool", "-l", archive]))?;
    let mut versions = vec![baseline.to_owned()];
    let mut command = None;
    for line in listing.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if let [key, value] = fields[..] {
            if key == "cmd" {
                command = Some(value.to_owned());
            }
            let build_version = command.as_deref() == Some("LC_BUILD_VERSION") && key == "minos";
            let legacy = matches!(
                command.as_deref(),
                Some("LC_VERSION_MIN_MACOSX" | "LC_VERSION_MIN_IPHONEOS")
            ) && key == "version";
            if build_version || legacy {
                versions.push(value.to_owned());
            }
        }
    }
    Ok(versions
        .into_iter()
        .max_by_key(|v| version_key(v))
        .unwrap_or_default())
}

impl Package {
    fn run(self) -> Result<()> {
        let module = &self.module;
        let valid = module.starts_with(|c: char| c.is_ascii_uppercase())
            && module
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(
                "module must be an ASCII identifier starting with an uppercase letter".into(),
            );
        }
        if self.out_dir.exists() {
            return Err("output directory already exists; choose a new package directory".into());
        }
        let ffi = format!("{module}FFI");
        let work = tempdir("bridgerton-package-")?;
        let mut frameworks = Vec::new();
        let mut sources = Vec::new();
        let mut targets = Vec::new();
        let mut minimums: BTreeMap<String, String> = BTreeMap::new();
        for bindings in &self.bindings {
            let info: Value =
                serde_json::from_str(&fs::read_to_string(bindings.join("build.json"))?)?;
            let target = info["target"].as_str().unwrap_or_default().to_owned();
            let Some(&(_, sdk, triple, condition, platform, minimum)) =
                APPLE.iter().find(|entry| entry.0 == target)
            else {
                return Err(format!("unsupported target: {target}").into());
            };
            if targets.contains(&target) {
                return Err(format!("duplicate target: {target}").into());
            }
            targets.push(target.clone());
            let archives = info["archives"].as_array().cloned().unwrap_or_default();
            let [archive] = &archives[..] else {
                return Err("packaging requires a staticlib crate-type".into());
            };
            let archive = archive.as_str().unwrap_or_default().to_owned();
            let minimum = deployment_minimum(&archive, minimum)?;
            let triple = replace_first_version(triple, &minimum);
            let platform_name = if sdk == "macosx" { "macOS" } else { "iOS" };
            let previous = minimums
                .get(platform_name)
                .cloned()
                .unwrap_or_else(|| "0".into());
            let newest = [previous, minimum.clone()]
                .into_iter()
                .max_by_key(|v| version_key(v))
                .unwrap_or_default();
            minimums.insert(platform_name.to_owned(), newest);
            if info["archive_sha256"][&archive] != json!(sha256(Path::new(&archive))?) {
                return Err(format!(
                    "archive changed since generation: {archive}; regenerate the bindings"
                )
                .into());
            }
            let directory = work.join(&target);
            let framework = directory.join(format!("{ffi}.framework"));
            fs::create_dir_all(framework.join("Headers"))?;
            fs::create_dir_all(framework.join("Modules"))?;
            let header = fs::read_to_string(bindings.join("BridgeFFI.h"))?;
            let swift = fs::read_to_string(bindings.join("Bridge.swift"))?;
            let mut aliases = Vec::new();
            let mut exports = Vec::new();
            let mut replacements = BTreeMap::new();
            for line in header.lines() {
                let Some(name) = export_name(line) else {
                    continue;
                };
                let original = line
                    .split("__asm__(\"")
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("_{name}"));
                let renamed = format!("{module}_{name}");
                aliases.push(format!("{original} _{renamed}"));
                exports.push(format!("_{renamed}"));
                replacements.insert(name, renamed);
            }
            if aliases.is_empty() {
                return Err(format!("no exports in {}", bindings.display()).into());
            }
            let header = strip_asm(&header);
            let header = replace_words(&header, &replacements);
            let swift = replace_words(&swift, &replacements)
                .replace("import BridgeFFI", &format!("import {ffi}"));
            fs::write(framework.join("Headers").join(format!("{ffi}.h")), header)?;
            fs::write(
                framework.join("Modules/module.modulemap"),
                format!("framework module {ffi} {{ umbrella header \"{ffi}.h\" export * }}\n"),
            )?;
            fs::write(
                directory.join("aliases"),
                format!("{}\n", aliases.join("\n")),
            )?;
            fs::write(
                directory.join("exports"),
                format!("{}\n", exports.join("\n")),
            )?;
            // Only namespaced C entry points are exported. Rust's allocator,
            // inventory, and task runtime stay local to this dynamic image.
            let mut clang = Command::new("xcrun");
            clang
                .args([
                    "clang",
                    "-target",
                    &triple,
                    "-isysroot",
                    &xcrun_sdk_path(sdk)?,
                    "-dynamiclib",
                ])
                .arg(format!("-Wl,-force_load,{archive}"))
                .arg(format!(
                    "-Wl,-alias_list,{}",
                    directory.join("aliases").display()
                ))
                .arg(format!(
                    "-Wl,-exported_symbols_list,{}",
                    directory.join("exports").display()
                ))
                .arg("-Wl,-dead_strip")
                .arg(format!("-Wl,-install_name,@rpath/{ffi}.framework/{ffi}"));
            for library in info["native_static_libraries"]
                .as_array()
                .into_iter()
                .flatten()
            {
                clang.arg(library.as_str().unwrap_or_default());
            }
            clang.arg("-o").arg(framework.join(&ffi));
            run(&mut clang)?;
            fs::write(
                framework.join("Info.plist"),
                info_plist(&ffi, platform, &minimum),
            )?;
            frameworks.push(framework);
            sources.push(format!("#if {condition}\n{swift}\n#endif\n"));
        }
        let staged = work.join("package");
        let source_dir = staged.join("Sources").join(module);
        fs::create_dir_all(&source_dir)?;
        fs::write(source_dir.join("Bridge.swift"), sources.join("\n"))?;
        let mut xcodebuild = Command::new("xcodebuild");
        xcodebuild.arg("-create-xcframework");
        for framework in &frameworks {
            xcodebuild.arg("-framework").arg(framework);
        }
        xcodebuild
            .arg("-output")
            .arg(staged.join(format!("{ffi}.xcframework")));
        run(&mut xcodebuild)?;
        let platforms = minimums
            .iter()
            .map(|(name, version)| format!(".{name}(\"{version}\")"))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            staged.join("Package.swift"),
            format!(
                "// swift-tools-version: 6.2\nimport PackageDescription\nlet package = Package(\n    name: \"{module}\",\n    platforms: [{platforms}],\n    products: [.library(name: \"{module}\", targets: [\"{module}\"])],\n    targets: [\n        .binaryTarget(name: \"{ffi}\", path: \"{ffi}.xcframework\"),\n        .target(name: \"{module}\", dependencies: [\"{ffi}\"])\n    ]\n)\n"
            ),
        )?;
        targets.sort();
        fs::write(
            staged.join("build.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({
                    "module": module, "targets": targets, "minimum_os": minimums
                }))?
            ),
        )?;
        copy_tree(&staged, &self.out_dir)?;
        fs::remove_dir_all(&work)?;
        println!(
            "Packaged {module}: {} at {}",
            targets.join(", "),
            self.out_dir.display()
        );
        Ok(())
    }
}

fn export_name(line: &str) -> Option<String> {
    let rest = ["BridgeResult ", "void ", "uint8_t "]
        .iter()
        .find_map(|prefix| line.strip_prefix(prefix))?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty() && rest[name.len()..].starts_with('(')).then_some(name)
}

fn strip_asm(header: &str) -> String {
    header
        .lines()
        .map(|line| match line.find(" __asm__(\"") {
            Some(start) => {
                let end = line[start..]
                    .find("\")")
                    .map(|i| start + i + 2)
                    .unwrap_or(line.len());
                format!("{}{}", &line[..start], &line[end..])
            }
            None => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Replace whole identifiers only.
fn replace_words(text: &str, replacements: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if !word.is_empty() {
            out.push_str(replacements.get(word.as_str()).unwrap_or(word));
            word.clear();
        }
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Replace the first dotted version number (for example `15.0` in `arm64-apple-macos15.0`).
fn replace_first_version(triple: &str, version: &str) -> String {
    let bytes = triple.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if bytes[start].is_ascii_digit() && (start == 0 || !bytes[start - 1].is_ascii_digit()) {
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
                end += 1;
            }
            if triple[start..end].contains('.') {
                return format!("{}{version}{}", &triple[..start], &triple[end..]);
            }
        }
        start += 1;
    }
    triple.to_owned()
}

fn info_plist(ffi: &str, platform: &str, minimum: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>{ffi}</string>
    <key>CFBundleIdentifier</key><string>org.bridgerton.{ffi}</string>
    <key>CFBundleName</key><string>{ffi}</string>
    <key>CFBundlePackageType</key><string>FMWK</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>CFBundleSupportedPlatforms</key><array><string>{platform}</string></array>
    <key>CFBundleVersion</key><string>1</string>
    <key>MinimumOSVersion</key><string>{minimum}</string>
</dict>
</plist>
"#
    )
}

fn shell_words(text: &str) -> Result<Vec<String>> {
    shlex::split(text).ok_or_else(|| "unclosed quote or escape in command arguments".into())
}

fn tempdir(prefix: &str) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("{prefix}{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path)?;
    }
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn runner_arguments_preserve_quoting_without_shell_expansion() {
        assert_eq!(
            super::shell_words(r#""/path with spaces/runner" --output '{output}' '' '$HOME'"#)
                .unwrap(),
            [
                "/path with spaces/runner",
                "--output",
                "{output}",
                "",
                "$HOME"
            ]
        );
        assert!(super::shell_words("runner 'unfinished").is_err());
    }

    #[test]
    fn version_replacement_skips_architecture_digits() {
        assert_eq!(
            super::replace_first_version("arm64-apple-macos15.0", "15.4"),
            "arm64-apple-macos15.4"
        );
        assert_eq!(
            super::replace_first_version("arm64-apple-ios18.0-simulator", "18.2"),
            "arm64-apple-ios18.2-simulator"
        );
    }
}
