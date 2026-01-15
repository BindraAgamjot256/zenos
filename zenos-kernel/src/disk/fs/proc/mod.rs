//! Procfs - A virtual filesystem exposing kernel and process information.
//!
//! This module implements a Linux-like /proc filesystem that provides:
//! - `/proc/cpuinfo` - CPU information
//! - `/proc/meminfo` - Memory statistics
//! - `/proc/version` - OS version string
//! - `/proc/uptime` - System uptime in seconds
//! - `/proc/<pid>/` - Per-process directories containing:
//!   - `stat` - Process statistics
//!   - `status` - Human-readable process status
//!   - `cmdline` - Process command line/name

use crate::disk::FileError;
use crate::disk::vfs::{DirEntry, Directory, File, FileSystem, FileType, Metadata, SeekFrom};
use crate::memory;
use crate::process::{PROCESSES, current_pid};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, vec};
use core::arch::x86_64::__cpuid;
use core::sync::atomic::{AtomicU64, Ordering};

/// Global tick counter incremented by the timer interrupt (10ms per tick)
pub static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Increment the tick counter (called from timer interrupt)
pub fn tick() {
    TICK_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Get system uptime in seconds
pub fn uptime_secs() -> u64 {
    TICK_COUNT.load(Ordering::Relaxed) / 100 // 10ms per tick = 100 ticks per second
}

/// The proc filesystem
pub struct ProcFs;

impl FileSystem for ProcFs {
    fn root_dir(&self) -> Result<Box<dyn Directory>, FileError> {
        Ok(Box::new(ProcRootDir))
    }
}

/// Root directory of /proc
struct ProcRootDir;

impl Directory for ProcRootDir {
    fn open_file(&mut self, name: &str) -> Result<Box<dyn File>, FileError> {
        match name {
            "cpuinfo" => Ok(Box::new(ProcFile::new(ProcFileType::CpuInfo))),
            "meminfo" => Ok(Box::new(ProcFile::new(ProcFileType::MemInfo))),
            "version" => Ok(Box::new(ProcFile::new(ProcFileType::Version))),
            "uptime" => Ok(Box::new(ProcFile::new(ProcFileType::Uptime))),
            "self" => {
                // /proc/self is a special case - redirect to current process's stat
                let pid = current_pid();
                Ok(Box::new(ProcFile::new(ProcFileType::ProcessStat(pid))))
            }
            _ => Err(FileError::NotFound),
        }
    }

    fn create_file(&mut self, _name: &str) -> Result<Box<dyn File>, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn open_dir(&mut self, name: &str) -> Result<Box<dyn Directory>, FileError> {
        // Handle "self" as symlink to current process
        if name == "self" {
            let pid = current_pid();
            return Ok(Box::new(ProcessDir { pid }));
        }

        // Try to parse as PID
        if let Ok(pid) = name.parse::<u64>() {
            let procs = PROCESSES.lock();
            if procs.iter().any(|p| p.pid == pid) {
                return Ok(Box::new(ProcessDir { pid }));
            }
        }
        Err(FileError::NotFound)
    }

    fn create_dir(&mut self, _name: &str) -> Result<Box<dyn Directory>, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn remove(&mut self, _name: &str) -> Result<(), FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        let mut entries = Vec::new();

        // Static entries
        for name in &["cpuinfo", "meminfo", "version", "uptime", "self"] {
            entries.push(DirEntry {
                name: String::from(*name),
                metadata: Metadata {
                    size: 0,
                    ftype: FileType::File,
                    created: 0,
                    modified: 0,
                    accessed: 0,
                },
            });
        }

        // Process directories
        let procs = PROCESSES.lock();
        for proc in procs.iter() {
            entries.push(DirEntry {
                name: format!("{}", proc.pid),
                metadata: Metadata {
                    size: 0,
                    ftype: FileType::Directory,
                    created: 0,
                    modified: 0,
                    accessed: 0,
                },
            });
        }

        Ok(entries)
    }
}

/// Per-process directory /proc/<pid>/
struct ProcessDir {
    pid: u64,
}

impl Directory for ProcessDir {
    fn open_file(&mut self, name: &str) -> Result<Box<dyn File>, FileError> {
        match name {
            "stat" => Ok(Box::new(ProcFile::new(ProcFileType::ProcessStat(self.pid)))),
            "status" => Ok(Box::new(ProcFile::new(ProcFileType::ProcessStatus(
                self.pid,
            )))),
            "cmdline" => Ok(Box::new(ProcFile::new(ProcFileType::ProcessCmdline(
                self.pid,
            )))),
            _ => Err(FileError::NotFound),
        }
    }

    fn create_file(&mut self, _name: &str) -> Result<Box<dyn File>, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn open_dir(&mut self, _name: &str) -> Result<Box<dyn Directory>, FileError> {
        Err(FileError::NotFound)
    }

    fn create_dir(&mut self, _name: &str) -> Result<Box<dyn Directory>, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn remove(&mut self, _name: &str) -> Result<(), FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        Ok(vec![
            DirEntry {
                name: String::from("stat"),
                metadata: Metadata {
                    size: 0,
                    ftype: FileType::File,
                    created: 0,
                    modified: 0,
                    accessed: 0,
                },
            },
            DirEntry {
                name: String::from("status"),
                metadata: Metadata {
                    size: 0,
                    ftype: FileType::File,
                    created: 0,
                    modified: 0,
                    accessed: 0,
                },
            },
            DirEntry {
                name: String::from("cmdline"),
                metadata: Metadata {
                    size: 0,
                    ftype: FileType::File,
                    created: 0,
                    modified: 0,
                    accessed: 0,
                },
            },
        ])
    }
}

/// Types of virtual files in procfs
#[derive(Clone)]
enum ProcFileType {
    CpuInfo,
    MemInfo,
    Version,
    Uptime,
    ProcessStat(u64),
    ProcessStatus(u64),
    ProcessCmdline(u64),
}

/// A virtual file in procfs
struct ProcFile {
    file_type: ProcFileType,
    content: Vec<u8>,
    position: u64,
    generated: bool,
}

impl ProcFile {
    fn new(file_type: ProcFileType) -> Self {
        Self {
            file_type,
            content: Vec::new(),
            position: 0,
            generated: false,
        }
    }

    fn generate_content(&mut self) {
        if self.generated {
            return;
        }
        self.generated = true;

        let content = match &self.file_type {
            ProcFileType::CpuInfo => generate_cpuinfo(),
            ProcFileType::MemInfo => generate_meminfo(),
            ProcFileType::Version => generate_version(),
            ProcFileType::Uptime => generate_uptime(),
            ProcFileType::ProcessStat(pid) => generate_process_stat(*pid),
            ProcFileType::ProcessStatus(pid) => generate_process_status(*pid),
            ProcFileType::ProcessCmdline(pid) => generate_process_cmdline(*pid),
        };
        self.content = content.into_bytes();
    }
}

impl File for ProcFile {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FileError> {
        self.generate_content();

        let pos = self.position as usize;
        if pos >= self.content.len() {
            return Ok(0);
        }

        let remaining = &self.content[pos..];
        let to_read = buf.len().min(remaining.len());
        buf[..to_read].copy_from_slice(&remaining[..to_read]);
        self.position += to_read as u64;
        Ok(to_read)
    }

    fn write(&mut self, _buf: &[u8]) -> Result<usize, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn seek(&mut self, pos: SeekFrom) -> Result<u64, FileError> {
        self.generate_content();

        let len = self.content.len() as i64;
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => len + offset,
            SeekFrom::Current(offset) => self.position as i64 + offset,
        };

        if new_pos < 0 {
            return Err(FileError::SeekError);
        }

        self.position = new_pos as u64;
        Ok(self.position)
    }

    fn flush(&mut self) -> Result<(), FileError> {
        Ok(())
    }

    fn metadata(&self) -> Result<Metadata, FileError> {
        Ok(Metadata {
            size: 0, // Virtual files have dynamic size
            ftype: FileType::File,
            created: 0,
            modified: 0,
            accessed: 0,
        })
    }
}

pub fn generate_cpuinfo() -> String {
    let mut info = String::new();

    // Vendor
    let vendor = unsafe {
        let c = __cpuid(0);
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(&c.ebx.to_le_bytes());
        bytes[4..8].copy_from_slice(&c.edx.to_le_bytes());
        bytes[8..12].copy_from_slice(&c.ecx.to_le_bytes());
        core::str::from_utf8(&bytes)
            .unwrap_or("Unknown")
            .to_string()
    };

    // Brand
    let brand = unsafe {
        let max = __cpuid(0x80000000).eax;
        if max >= 0x80000004 {
            let mut bytes = [0u8; 48];
            for i in 0..3 {
                let c = __cpuid(0x80000002 + i);
                let off = (i * 16) as usize;
                bytes[off..off + 4].copy_from_slice(&c.eax.to_le_bytes());
                bytes[off + 4..off + 8].copy_from_slice(&c.ebx.to_le_bytes());
                bytes[off + 8..off + 12].copy_from_slice(&c.ecx.to_le_bytes());
                bytes[off + 12..off + 16].copy_from_slice(&c.edx.to_le_bytes());
            }
            core::str::from_utf8(&bytes)
                .unwrap_or("Unknown")
                .trim_matches('\0')
                .trim()
                .to_string()
        } else {
            "Unknown".to_string()
        }
    };

    // Family / model / stepping
    let (family, model, stepping) = unsafe {
        let c = __cpuid(1);
        let stepping = c.eax & 0xF;
        let model = ((c.eax >> 4) & 0xF) | (((c.eax >> 16) & 0xF) << 4);
        let family = ((c.eax >> 8) & 0xF) + ((c.eax >> 20) & 0xFF);
        (family, model, stepping)
    };

    // Features
    let mut flags = Vec::new();
    unsafe {
        let c1 = __cpuid(1);
        let c7 = __cpuid(7);

        macro_rules! f {
            ($cond:expr, $name:expr) => {
                if $cond {
                    flags.push($name);
                }
            };
        }

        // Basic
        f!(c1.edx & (1 << 0) != 0, "fpu");
        f!(c1.edx & (1 << 4) != 0, "tsc");
        f!(c1.edx & (1 << 5) != 0, "msr");
        f!(c1.edx & (1 << 6) != 0, "pae");
        f!(c1.edx & (1 << 9) != 0, "apic");
        f!(c1.edx & (1 << 23) != 0, "mmx");
        f!(c1.edx & (1 << 25) != 0, "sse");
        f!(c1.edx & (1 << 26) != 0, "sse2");

        f!(c1.ecx & (1 << 0) != 0, "sse3");
        f!(c1.ecx & (1 << 9) != 0, "ssse3");
        f!(c1.ecx & (1 << 19) != 0, "sse4_1");
        f!(c1.ecx & (1 << 20) != 0, "sse4_2");
        f!(c1.ecx & (1 << 28) != 0, "avx");

        // CPUID 7
        f!(c7.ebx & (1 << 3) != 0, "bmi1");
        f!(c7.ebx & (1 << 8) != 0, "bmi2");
        f!(c7.ebx & (1 << 7) != 0, "smep");
        f!(c7.ebx & (1 << 20) != 0, "smap");
        f!(c7.ebx & (1 << 0) != 0, "fsgsbase");
        f!(c7.ebx & (1 << 18) != 0, "rdseed");
        f!(c7.ebx & (1 << 19) != 0, "adx");
        f!(c7.ebx & (1 << 29) != 0, "sha_ni");

        // Extended AMD
        let ce = __cpuid(0x80000001);
        f!(ce.edx & (1 << 20) != 0, "nx");
        f!(ce.edx & (1 << 29) != 0, "lm");
        f!(ce.ecx & (1 << 5) != 0, "abm");
        f!(ce.ecx & (1 << 6) != 0, "sse4a");

        // Hypervisor
        f!(c1.ecx & (1 << 31) != 0, "hypervisor");
    }

    // Address sizes
    let (phys_bits, virt_bits) = unsafe {
        let c = __cpuid(0x80000008);
        ((c.eax & 0xFF), ((c.eax >> 8) & 0xFF))
    };

    info.push_str("processor\t: 0\n");
    info.push_str(&format!("vendor_id\t: {}\n", vendor.trim()));
    info.push_str(&format!("cpu family\t: {}\n", family));
    info.push_str(&format!("model\t\t: {}\n", model));
    info.push_str(&format!("model name\t: {}\n", brand));
    info.push_str(&format!("stepping\t: {}\n", stepping));
    info.push_str("microcode\t: 0x0\n");
    info.push_str("cpu MHz\t\t: 3300.000\n");
    info.push_str("cache size\t: 512 KB\n");
    info.push_str("physical id\t: 0\n");
    info.push_str("siblings\t: 1\n");
    info.push_str("core id\t\t: 0\n");
    info.push_str("cpu cores\t: 1\n");
    info.push_str("apicid\t\t: 0\n");
    info.push_str("initial apicid\t: 0\n");
    info.push_str(&format!("flags\t\t: {}\n", flags.join(" ")));
    info.push_str("bugs\t\t: spectre_v1 spectre_v2 spec_store_bypass retbleed\n");
    info.push_str("bogomips\t: 6600.00\n");
    info.push_str("clflush size\t: 64\n");
    info.push_str("cache_alignment\t: 64\n");
    info.push_str(&format!(
        "address sizes\t: {} bits physical, {} bits virtual\n",
        phys_bits, virt_bits
    ));
    info.push_str("power management:\n\n");

    info
}

/// Generate /proc/meminfo content
fn generate_meminfo() -> String {
    let mut info = String::new();

    match memory::get_stats() {
        Ok((free_pages, total_pages)) => {
            let page_size_kb = memory::PAGE_4K / 1024;
            let total_kb = total_pages * page_size_kb;
            let free_kb = free_pages * page_size_kb;
            let used_kb = total_kb.saturating_sub(free_kb);

            info.push_str(&format!("MemTotal:       {:8} kB\n", total_kb));
            info.push_str(&format!("MemFree:        {:8} kB\n", free_kb));
            info.push_str(&format!("MemUsed:        {:8} kB\n", used_kb));
        }
        Err(_) => {
            info.push_str("MemTotal:              0 kB\n");
            info.push_str("MemFree:               0 kB\n");
        }
    }

    info
}

/// Generate /proc/version content
fn generate_version() -> String {
    format!(
        "zenos version {} (rustc {})\n",
        env!("CARGO_PKG_VERSION"),
        "nightly"
    )
}

/// Generate /proc/uptime content
fn generate_uptime() -> String {
    unsafe { PROCESSES.force_unlock() }
    let ticks = TICK_COUNT.load(Ordering::Relaxed);
    let secs = ticks / 100; // 10ms per tick
    let centisecs = ticks % 100;
    format!("{}.{:02} 0.00\n", secs, centisecs)
}

/// Generate /proc/<pid>/stat content
fn generate_process_stat(pid: u64) -> String {
    unsafe { PROCESSES.force_unlock() }
    let procs = PROCESSES.lock();

    let proc = match procs.iter().find(|p| p.pid == pid) {
        Some(p) => p,
        None => return String::new(),
    };

    // Map process state to Linux-compatible letters
    let state = match proc.status {
        crate::process::ProcessStatus::Running => 'R',
        crate::process::ProcessStatus::Ready => 'S',
        crate::process::ProcessStatus::Blocked(_) => 'D',
        crate::process::ProcessStatus::Created => 'S',
        crate::process::ProcessStatus::Exited => 'Z',
    };

    // Sanitize name to avoid breaking parsers
    let name = proc.name.replace(')', "?");

    // Minimal but valid 52-field /proc/<pid>/stat
    format!(
        concat!(
            "{} ({}) {} {} ", // 1–4
            "0 0 0 0 0 ",     // 5–9
            "0 0 0 0 0 ",     // 10–14
            "0 0 0 0 1 ",     // 15–19
            "0 0 ",           // 20–21
            "0 0 0 ",         // 22–24
            "{} {} {} ",      // 25–27 (code/stack)
            "0 0 0 0 0 ",     // 28–32
            "0 0 0 0 0 ",     // 33–37
            "0 0 0 0 0 ",     // 38–42
            "0 0 0 0 0 0\n"   // 43–52
        ),
        proc.pid,
        name,
        state,
        proc.parent_pid,
        proc.entry_point,    // startcode
        proc.end,            // endcode
        proc.user_stack_top, // startstack
    )
}

/// Generate /proc/<pid>/status content
fn generate_process_status(pid: u64) -> String {
    unsafe { PROCESSES.force_unlock() }
    let procs = PROCESSES.lock();
    if let Some(proc) = procs.iter().find(|p| p.pid == pid) {
        let state_str = match proc.status {
            crate::process::ProcessStatus::Running => "R (running)",
            crate::process::ProcessStatus::Ready => "S (sleeping)",
            crate::process::ProcessStatus::Blocked(_) => "D (disk sleep)",
            crate::process::ProcessStatus::Created => "T (stopped)",
            crate::process::ProcessStatus::Exited => "Z (zombie)",
        };

        let mut info = String::new();
        info.push_str(&format!("Name:\t{}\n", proc.name));
        info.push_str(&format!("State:\t{}\n", state_str));
        info.push_str(&format!("Pid:\t{}\n", proc.pid));
        info.push_str(&format!("PPid:\t{}\n", proc.parent_pid));
        info.push_str(&format!("FDSize:\t{}\n", proc.file_handles.len()));
        info
    } else {
        String::new()
    }
}

/// Generate /proc/<pid>/cmdline content
fn generate_process_cmdline(pid: u64) -> String {
    unsafe { PROCESSES.force_unlock() }
    let procs = PROCESSES.lock();
    if let Some(proc) = procs.iter().find(|p| p.pid == pid) {
        format!("{}\0", proc.name)
    } else {
        String::new()
    }
}

#[cfg(feature = "run-kunittest")]
mod tests {
    use super::*;
    use crate::Test;

    #[zenos_macros::test]
    pub fn test_tick_counter() -> Option<()> {
        let before = TICK_COUNT.load(Ordering::Relaxed);
        tick();
        let after = TICK_COUNT.load(Ordering::Relaxed);
        crate::test_assert!(after > before);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_cpuinfo_not_empty() -> Option<()> {
        let info = generate_cpuinfo();
        crate::test_assert!(!info.is_empty());
        crate::test_assert!(info.contains("processor"));
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_version_format() -> Option<()> {
        let version = generate_version();
        crate::test_assert!(version.contains("zenos"));
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_uptime_format() -> Option<()> {
        let uptime = generate_uptime();
        crate::test_assert!(uptime.contains('.'));
        Some(())
    }
}
