//! Procfs — Virtual filesystem exposing kernel and process information.
//!
//! This module implements a Linux-compatible `/proc` filesystem that provides
//! runtime information about the system and running processes. All files are
//! virtual (generated on read) and read-only.
//!
//! # Available Files
//!
//! ## System Information
//!
//! | Path | Description |
//! |------|-------------|
//! | `/proc/cpuinfo` | CPU vendor, model, features, and capabilities |
//! | `/proc/meminfo` | Memory statistics (total, free, used) |
//! | `/proc/version` | Kernel version string |
//! | `/proc/uptime` | System uptime in seconds |
//!
//! ## Per-Process Information (`/proc/<pid>/`)
//!
//! | File | Description |
//! |------|-------------|
//! | `stat` | Process statistics in Linux-compatible format (52 fields) |
//! | `status` | Human-readable process status (name, state, PID, etc.) |
//! | `cmdline` | Process name/command line (NUL-terminated) |
//!
//! ## Special Entries
//!
//! - `/proc/self` — Symlink-like behavior redirecting to current process
//!
//! # Implementation Notes
//!
//! - File contents are generated lazily on first read
//! - The [`TICK_COUNT`] atomic is incremented by the timer interrupt (10ms/tick)
//! - CPU information is obtained via `CPUID` instruction
//! - Process information is read from the global [`PROCESSES`](PROCESSES) table

use crate::disk::FileError;
use crate::disk::vfs::{DirEntry, FileSystem, FileType, Inode, InodeOps, Permissions};
use crate::memory;
use crate::process::{PROCESSES, SCHEDULER};
use crate::time::TICK_COUNT;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::{format, vec};
use core::arch::x86_64::__cpuid;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Get system uptime in seconds
pub fn uptime_secs() -> u64 {
    TICK_COUNT.load(Ordering::Relaxed) / 100 // 10ms per tick = 100 ticks per second
}

/// The proc filesystem
pub struct ProcFs;

impl FileSystem for ProcFs {
    fn root_dir(&self) -> Result<Arc<Mutex<Inode>>, FileError> {
        Ok(Arc::new(Mutex::new(Inode {
            num: 0,
            kind: FileType::Directory,
            size: AtomicU64::new(0),
            perms: Permissions::OWNER_READ | Permissions::GROUP_READ | Permissions::OTHER_READ,
            links: AtomicU64::new(1),
            data: Box::new(ProcRootDir),
        })))
    }
}

/// Root directory of /proc
struct ProcRootDir;

impl InodeOps for ProcRootDir {
    fn read(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, FileError> {
        Err(FileError::IsADirectory)
    }

    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, FileError> {
        Err(FileError::IsADirectory)
    }

    fn truncate(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::IsADirectory)
    }

    fn sync(&mut self) -> Result<(), FileError> {
        Err(FileError::IsADirectory)
    }

    fn unlink(&mut self, _name: &str) -> Result<(), FileError> {
        Err(FileError::ReadOnlyFilesystem)
    }

    fn lookup(&mut self, name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        match name {
            "cpuinfo" => Ok(Arc::new(Mutex::new(Inode {
                num: 0,
                kind: FileType::File,
                size: AtomicU64::new(0),
                perms: Permissions::OWNER_READ | Permissions::GROUP_READ | Permissions::OTHER_READ,
                links: AtomicU64::new(1),
                data: Box::new(ProcFile::new(ProcFileType::CpuInfo)),
            }))),
            "meminfo" => Ok(Arc::new(Mutex::new(Inode {
                num: 0,
                kind: FileType::File,
                size: AtomicU64::new(0),
                perms: Permissions::OWNER_READ | Permissions::GROUP_READ | Permissions::OTHER_READ,
                links: AtomicU64::new(1),
                data: Box::new(ProcFile::new(ProcFileType::MemInfo)),
            }))),
            "version" => Ok(Arc::new(Mutex::new(Inode {
                num: 0,
                kind: FileType::File,
                size: AtomicU64::new(0),
                perms: Permissions::OWNER_READ | Permissions::GROUP_READ | Permissions::OTHER_READ,
                links: AtomicU64::new(1),
                data: Box::new(ProcFile::new(ProcFileType::Version)),
            }))),
            "uptime" => Ok(Arc::new(Mutex::new(Inode {
                num: 0,
                kind: FileType::File,
                size: AtomicU64::new(0),
                perms: Permissions::OWNER_READ | Permissions::GROUP_READ | Permissions::OTHER_READ,
                links: AtomicU64::new(1),
                data: Box::new(ProcFile::new(ProcFileType::Uptime)),
            }))),
            "self" => Ok(Arc::new(Mutex::new(Inode {
                num: 0,
                kind: FileType::Directory,
                size: AtomicU64::new(0),
                perms: Permissions::OWNER_READ
                    | Permissions::OWNER_EXEC
                    | Permissions::GROUP_READ
                    | Permissions::GROUP_EXEC
                    | Permissions::OTHER_READ
                    | Permissions::OTHER_EXEC,
                links: AtomicU64::new(1),
                data: Box::new(ProcessDir {
                    pid: SCHEDULER.lock().current_pid().unwrap(),
                }),
            }))),

            _ => {
                if name.parse::<u64>().is_ok() {
                    let pid = name.parse::<u64>().unwrap();
                    Ok(Arc::new(Mutex::new(Inode {
                        num: 0,
                        kind: FileType::Directory,
                        size: AtomicU64::new(0),
                        perms: Permissions::OWNER_READ
                            | Permissions::OWNER_EXEC
                            | Permissions::GROUP_READ
                            | Permissions::GROUP_EXEC
                            | Permissions::OTHER_READ
                            | Permissions::OTHER_EXEC,
                        links: AtomicU64::new(1),
                        data: Box::new(ProcessDir { pid }),
                    })))
                } else {
                    Err(FileError::NotFound)
                }
            }
        }
    }

    fn create(
        &mut self,
        _name: &str,
        _kind: FileType,
        _perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::ReadOnlyFilesystem)
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        let mut vec = vec![
            DirEntry {
                name: "cpuinfo".to_string(),
                inode: self.lookup("cpuinfo")?,
                offset: 0,
                file_type: FileType::File,
            },
            DirEntry {
                name: "meminfo".to_string(),
                inode: self.lookup("meminfo")?,
                offset: 1,
                file_type: FileType::File,
            },
            DirEntry {
                name: "version".to_string(),
                inode: self.lookup("version")?,
                offset: 2,
                file_type: FileType::File,
            },
            DirEntry {
                name: "uptime".to_string(),
                inode: self.lookup("uptime")?,
                offset: 3,
                file_type: FileType::File,
            },
        ];

        // Add entries for each process
        unsafe { PROCESSES.force_unlock() }
        let procs = PROCESSES.lock();
        let mut offset = 4;
        for proc in procs.iter() {
            let name = proc.pid.to_string();
            vec.push(DirEntry {
                name: name.clone(),
                inode: self.lookup(&name)?,
                offset,
                file_type: FileType::Directory,
            });
            offset += 1;
        }
        Ok(vec)
    }
}

/// Per-process directory /proc/<pid>/
struct ProcessDir {
    pid: u64,
}

impl InodeOps for ProcessDir {
    fn read(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, FileError> {
        Err(FileError::IsADirectory)
    }

    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, FileError> {
        Err(FileError::IsADirectory)
    }

    fn truncate(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::IsADirectory)
    }

    fn sync(&mut self) -> Result<(), FileError> {
        Err(FileError::IsADirectory)
    }

    fn unlink(&mut self, _name: &str) -> Result<(), FileError> {
        Err(FileError::ReadOnlyFilesystem)
    }
    fn lookup(&mut self, name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        match name {
            "stat" => Ok(Arc::new(Mutex::new(Inode {
                num: 0,
                kind: FileType::File,
                size: AtomicU64::new(0),
                perms: Permissions::OWNER_READ | Permissions::GROUP_READ | Permissions::OTHER_READ,
                links: AtomicU64::new(1),
                data: Box::new(ProcFile::new(ProcFileType::ProcessStat(self.pid))),
            }))),
            "status" => Ok(Arc::new(Mutex::new(Inode {
                num: 0,
                kind: FileType::File,
                size: AtomicU64::new(0),
                perms: Permissions::OWNER_READ | Permissions::GROUP_READ | Permissions::OTHER_READ,
                links: AtomicU64::new(1),
                data: Box::new(ProcFile::new(ProcFileType::ProcessStatus(self.pid))),
            }))),
            "cmdline" => Ok(Arc::new(Mutex::new(Inode {
                num: 0,
                kind: FileType::File,
                size: AtomicU64::new(0),
                perms: Permissions::OWNER_READ | Permissions::GROUP_READ | Permissions::OTHER_READ,
                links: AtomicU64::new(1),
                data: Box::new(ProcFile::new(ProcFileType::ProcessCmdline(self.pid))),
            }))),
            _ => Err(FileError::NotFound),
        }
    }
    fn create(
        &mut self,
        _name: &str,
        _kind: FileType,
        _perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::ReadOnlyFilesystem)
    }
    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        Ok(vec![
            DirEntry {
                name: "stat".to_string(),
                inode: self.lookup("stat")?,
                offset: 0,
                file_type: FileType::File,
            },
            DirEntry {
                name: "status".to_string(),
                inode: self.lookup("status")?,
                offset: 1,
                file_type: FileType::File,
            },
            DirEntry {
                name: "cmdline".to_string(),
                inode: self.lookup("cmdline")?,
                offset: 2,
                file_type: FileType::File,
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
    generated: bool,
}

impl ProcFile {
    fn new(file_type: ProcFileType) -> Self {
        Self {
            file_type,
            content: Vec::new(),
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

impl InodeOps for ProcFile {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, FileError> {
        self.generate_content();
        let content_len = self.content.len() as u64;
        if offset >= content_len {
            return Ok(0);
        }
        let to_read = core::cmp::min(buf.len() as u64, content_len - offset) as usize;
        buf[..to_read].copy_from_slice(&self.content[offset as usize..(offset as usize + to_read)]);
        Ok(to_read)
    }

    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, FileError> {
        Err(FileError::ReadOnlyFilesystem)
    }

    fn truncate(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::ReadOnlyFilesystem)
    }

    fn sync(&mut self) -> Result<(), FileError> {
        Ok(())
    }

    fn unlink(&mut self, _name: &str) -> Result<(), FileError> {
        Err(FileError::ReadOnlyFilesystem)
    }

    fn lookup(&mut self, _name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::NotADirectory)
    }

    fn create(
        &mut self,
        _name: &str,
        _kind: FileType,
        _perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::NotADirectory)
    }
    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        Err(FileError::NotADirectory)
    }
}

pub fn generate_cpuinfo() -> String {
    let mut info = String::new();

    // Vendor
    let vendor = {
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
    let brand = {
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
    let (family, model, stepping) = {
        let c = __cpuid(1);
        let stepping = c.eax & 0xF;
        let model = ((c.eax >> 4) & 0xF) | (((c.eax >> 16) & 0xF) << 4);
        let family = ((c.eax >> 8) & 0xF) + ((c.eax >> 20) & 0xFF);
        (family, model, stepping)
    };

    // Features
    let mut flags = Vec::new();
    {
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
    let (phys_bits, virt_bits) = {
        let c = __cpuid(0x80000008);
        (c.eax & 0xFF, (c.eax >> 8) & 0xFF)
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
        "1.95.0-nightly"
    )
}

/// Generate /proc/uptime content
fn generate_uptime() -> String {
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
        crate::process::ProcessStatus::WaitingFor(_) => 'S', // Sleeping (waiting)
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
            crate::process::ProcessStatus::WaitingFor(_) => "S (sleeping)", // Waiting for child
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
