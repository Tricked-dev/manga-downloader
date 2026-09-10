//! Execution provider selection.
//!
//! ONNX Runtime reaches hardware through Execution Providers. Using one needs
//! two separate things to line up, and conflating them produces confusing
//! errors:
//!
//! 1. **This binary must be compiled with support for it.** `ort` gates each
//!    provider's bindings behind a cargo feature, so `manga-core` re-exports
//!    them as its own features (`cuda`, `directml`, `coreml`, `openvino`), all
//!    off by default.
//! 2. **The linked ONNX Runtime must have been built with it.** That is a
//!    property of the shared library and can only be checked at runtime.
//!
//! `Device` variants exist unconditionally so that command line parsing and
//! error messages do not change shape depending on build flags; the two checks
//! above are reported separately and specifically.
//!
//! The rule that matters: **asking for a specific GPU backend and silently
//! getting CPU is never acceptable.** An explicit `--device cuda` that cannot
//! be honoured is an error. Only `Device::Auto` falls back, and it says which
//! provider it landed on.

use std::fmt;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail};
use ort::ep::ExecutionProvider;
use ort::session::builder::SessionBuilder;
use tracing::{debug, info, warn};

/// Backend-specific knobs.
///
/// Kept separate from [`Device`] so the device itself stays a plain copyable
/// enum that is cheap to pass around and easy to parse from a flag.
#[derive(Debug, Clone, Default)]
pub struct DeviceOptions {
    /// OpenVINO's `device_type`: `CPU`, `GPU`, `GPU.0`, `NPU`, `AUTO`, or a
    /// heterogeneous form like `HETERO:NPU,GPU`.
    ///
    /// This matters more than it looks. OpenVINO runs on Intel CPUs as well as
    /// Intel GPUs, and left unset it picks for itself -- so "OpenVINO is
    /// active" is not the same as "the GPU is being used". Setting `GPU` makes
    /// the intent explicit and turns a missing GPU into an error rather than a
    /// silent CPU run.
    pub openvino_device_type: Option<String>,
}

/// Which hardware backend to run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    /// Try the GPU providers in preference order, then fall back to CPU.
    #[default]
    Auto,
    Cpu,
    Cuda,
    MiGraphX,
    DirectMl,
    CoreMl,
    OpenVino,
}

impl Device {
    pub const VARIANTS: &'static [&'static str] = &[
        "auto", "cpu", "cuda", "migraphx", "directml", "coreml", "openvino",
    ];

    /// Providers `Auto` will try, most preferred first.
    ///
    /// CUDA first because it is the fastest when present; OpenVINO last of the
    /// accelerators because it also covers Intel CPUs and would otherwise
    /// shadow a real GPU.
    const AUTO_ORDER: &'static [Device] = &[
        Device::Cuda,
        Device::MiGraphX,
        Device::DirectMl,
        Device::CoreMl,
        Device::OpenVino,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Device::Auto => "auto",
            Device::Cpu => "cpu",
            Device::Cuda => "cuda",
            Device::MiGraphX => "migraphx",
            Device::DirectMl => "directml",
            Device::CoreMl => "coreml",
            Device::OpenVino => "openvino",
        }
    }

    /// ONNX Runtime's own name for the provider, as reported by
    /// `GetAvailableProviders`.
    pub fn provider_name(self) -> Option<&'static str> {
        match self {
            Device::Auto => None,
            Device::Cpu => Some("CPUExecutionProvider"),
            Device::Cuda => Some("CUDAExecutionProvider"),
            Device::MiGraphX => Some("MIGraphXExecutionProvider"),
            Device::DirectMl => Some("DmlExecutionProvider"),
            Device::CoreMl => Some("CoreMLExecutionProvider"),
            Device::OpenVino => Some("OpenVINOExecutionProvider"),
        }
    }

    pub fn is_accelerator(self) -> bool {
        !matches!(self, Device::Auto | Device::Cpu)
    }

    /// Whether *this build* has the bindings for the provider.
    ///
    /// Separate from [`Device::is_available`], which asks the linked ONNX
    /// Runtime whether it has the provider.
    pub fn is_compiled_in(self) -> bool {
        match self {
            Device::Auto | Device::Cpu => true,
            Device::Cuda => cfg!(feature = "cuda"),
            Device::MiGraphX => cfg!(feature = "migraphx"),
            Device::DirectMl => cfg!(feature = "directml"),
            Device::CoreMl => cfg!(feature = "coreml"),
            Device::OpenVino => cfg!(feature = "openvino"),
        }
    }

    /// Whether the loaded ONNX Runtime was built with this provider.
    ///
    /// This says the provider *exists*, not that it will succeed for a given
    /// model, which is why registration errors are still handled.
    pub fn is_available(self) -> Result<bool> {
        if !self.is_compiled_in() {
            return Ok(false);
        }
        Ok(match self {
            Device::Auto | Device::Cpu => true,
            #[cfg(feature = "cuda")]
            Device::Cuda => ort::ep::CUDA::default().is_available()?,
            #[cfg(feature = "migraphx")]
            Device::MiGraphX => ort::ep::MIGraphX::default().is_available()?,
            #[cfg(feature = "directml")]
            Device::DirectMl => ort::ep::DirectML::default().is_available()?,
            #[cfg(feature = "coreml")]
            Device::CoreMl => ort::ep::CoreML::default().is_available()?,
            #[cfg(feature = "openvino")]
            Device::OpenVino => ort::ep::OpenVINO::default().is_available()?,
            // Unreachable: `is_compiled_in` returned false above.
            _ => false,
        })
    }

    fn register(self, builder: &mut SessionBuilder, options: &DeviceOptions) -> Result<()> {
        let _ = options;
        match self {
            Device::Auto => unreachable!("Auto is resolved before registration"),
            Device::Cpu => ort::ep::CPU::default().register(builder)?,
            #[cfg(feature = "cuda")]
            Device::Cuda => ort::ep::CUDA::default().register(builder)?,
            #[cfg(feature = "migraphx")]
            Device::MiGraphX => ort::ep::MIGraphX::default().register(builder)?,
            #[cfg(feature = "directml")]
            Device::DirectMl => ort::ep::DirectML::default().register(builder)?,
            #[cfg(feature = "coreml")]
            Device::CoreMl => ort::ep::CoreML::default().register(builder)?,
            #[cfg(feature = "openvino")]
            Device::OpenVino => {
                let mut provider = ort::ep::OpenVINO::default();
                if let Some(device_type) = &options.openvino_device_type {
                    provider = provider.with_device_type(device_type);
                }
                provider.register(builder)?
            }
            other => bail!(
                "this build of mangajanai-rs was compiled without {other} support; \
                 rebuild with `--features {other}`"
            ),
        }
        Ok(())
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Device {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        match text.to_ascii_lowercase().as_str() {
            "auto" => Ok(Device::Auto),
            "cpu" => Ok(Device::Cpu),
            "cuda" => Ok(Device::Cuda),
            "migraphx" => Ok(Device::MiGraphX),
            "directml" | "dml" => Ok(Device::DirectMl),
            "coreml" => Ok(Device::CoreMl),
            "openvino" => Ok(Device::OpenVino),
            other => Err(anyhow!(
                "unknown device `{other}`; expected one of: {}",
                Device::VARIANTS.join(", ")
            )),
        }
    }
}

/// Providers this build can use *and* the linked ONNX Runtime provides.
///
/// Probed through the public `ExecutionProvider::is_available` rather than
/// ONNX Runtime's raw provider list, so it reports what is actually reachable
/// from here rather than what merely exists.
pub fn available_providers() -> Vec<&'static str> {
    let mut found = vec![Device::Cpu.as_str()];
    for device in Device::AUTO_ORDER {
        if device.is_available().unwrap_or(false) {
            found.push(device.as_str());
        }
    }
    found
}

/// Configure a session builder for `device`, returning what was actually used.
///
/// For an explicit accelerator this either succeeds on that accelerator or
/// fails; it never quietly degrades to CPU.
pub fn configure(
    builder: &mut SessionBuilder,
    device: Device,
    options: &DeviceOptions,
) -> Result<Device> {
    if device == Device::Auto {
        return configure_auto(builder, options);
    }

    if device.is_accelerator() {
        if !device.is_compiled_in() {
            bail!(
                "this build of mangajanai-rs was compiled without {device} support. \
                 Rebuild with `--features {device}`, or pick one of: {}.",
                available_providers().join(", ")
            );
        }
        if !device.is_available()? {
            bail!(
                "the {device} execution provider is not available in the ONNX Runtime \
                 this binary is linked against. Usable here: {}. Use --device auto to \
                 fall back automatically, or --device cpu to be explicit.",
                available_providers().join(", ")
            );
        }
    }

    device
        .register(builder, options)
        .map_err(|error| anyhow!("failed to enable the {device} execution provider: {error}"))?;
    info!(
        device = %device,
        device_type = options.openvino_device_type.as_deref().unwrap_or("default"),
        "execution provider"
    );
    Ok(device)
}

fn configure_auto(builder: &mut SessionBuilder, options: &DeviceOptions) -> Result<Device> {
    for candidate in Device::AUTO_ORDER {
        match candidate.is_available() {
            Ok(false) => {
                debug!(device = %candidate, "provider not built into this ONNX Runtime");
                continue;
            }
            Err(error) => {
                debug!(device = %candidate, %error, "provider availability check failed");
                continue;
            }
            Ok(true) => {}
        }

        match candidate.register(builder, options) {
            Ok(()) => {
                info!(device = %candidate, "execution provider (auto-selected)");
                return Ok(*candidate);
            }
            Err(error) => {
                // Available but unusable -- a missing driver or runtime
                // library. Worth saying out loud, because the user probably
                // expected it to work.
                warn!(device = %candidate, %error, "provider present but could not be enabled");
            }
        }
    }

    Device::Cpu.register(builder, options)?;
    info!(
        device = "cpu",
        "execution provider (auto-selected, no GPU provider usable)"
    );
    Ok(Device::Cpu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_round_trip() {
        for name in Device::VARIANTS {
            let device: Device = name.parse().unwrap();
            assert_eq!(device.as_str(), *name);
        }
    }

    #[test]
    fn parsing_is_case_insensitive_and_accepts_dml() {
        assert_eq!("CUDA".parse::<Device>().unwrap(), Device::Cuda);
        assert_eq!("DML".parse::<Device>().unwrap(), Device::DirectMl);
        assert_eq!("dml".parse::<Device>().unwrap(), Device::DirectMl);
    }

    #[test]
    fn an_unknown_device_lists_the_valid_ones() {
        let error = "quantum".parse::<Device>().unwrap_err().to_string();
        assert!(error.contains("openvino"), "unhelpful error: {error}");
    }

    #[test]
    fn cpu_and_auto_are_not_accelerators() {
        assert!(!Device::Auto.is_accelerator());
        assert!(!Device::Cpu.is_accelerator());
        assert!(Device::Cuda.is_accelerator());
        assert!(Device::OpenVino.is_accelerator());
    }

    #[test]
    fn cpu_is_always_usable_and_listed() {
        assert!(Device::Cpu.is_compiled_in());
        assert!(Device::Cpu.is_available().unwrap());
        assert!(available_providers().contains(&"cpu"));
    }

    #[test]
    fn a_provider_not_compiled_in_is_never_reported_available() {
        // Whatever the feature set, these two answers must agree.
        for device in Device::AUTO_ORDER {
            if !device.is_compiled_in() {
                assert!(!device.is_available().unwrap());
            }
        }
    }

    #[test]
    fn auto_prefers_cuda_and_leaves_openvino_last() {
        // OpenVINO also runs on Intel CPUs, so it must not shadow a real GPU.
        assert_eq!(Device::AUTO_ORDER.first(), Some(&Device::Cuda));
        assert_eq!(Device::AUTO_ORDER.last(), Some(&Device::OpenVino));
        assert!(!Device::AUTO_ORDER.contains(&Device::Cpu));
    }
}
