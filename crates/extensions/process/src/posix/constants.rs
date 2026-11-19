//! `posix_access($file, POSIX_R_OK | POSIX_W_OK)` tests both permissions.

use nix::sys::resource::{RLIM_INFINITY, Resource};
use nix::sys::stat::SFlag;
use nix::unistd::{AccessFlags, PathconfVar, SysconfVar};
use phera_extension_api::Registry;

pub(super) fn register(registry: &mut Registry) {
    for (name, value) in [
        ("POSIX_F_OK", i64::from(AccessFlags::F_OK.bits())),
        ("POSIX_X_OK", i64::from(AccessFlags::X_OK.bits())),
        ("POSIX_W_OK", i64::from(AccessFlags::W_OK.bits())),
        ("POSIX_R_OK", i64::from(AccessFlags::R_OK.bits())),
        ("POSIX_S_IFREG", i64::from(SFlag::S_IFREG.bits())),
        ("POSIX_S_IFCHR", i64::from(SFlag::S_IFCHR.bits())),
        ("POSIX_S_IFBLK", i64::from(SFlag::S_IFBLK.bits())),
        ("POSIX_S_IFIFO", i64::from(SFlag::S_IFIFO.bits())),
        ("POSIX_S_IFSOCK", i64::from(SFlag::S_IFSOCK.bits())),
        ("POSIX_RLIMIT_INFINITY", RLIM_INFINITY as i64),
        ("POSIX_RLIMIT_CORE", Resource::RLIMIT_CORE as i64),
        ("POSIX_RLIMIT_CPU", Resource::RLIMIT_CPU as i64),
        ("POSIX_RLIMIT_DATA", Resource::RLIMIT_DATA as i64),
        ("POSIX_RLIMIT_FSIZE", Resource::RLIMIT_FSIZE as i64),
        ("POSIX_RLIMIT_NOFILE", Resource::RLIMIT_NOFILE as i64),
        ("POSIX_RLIMIT_STACK", Resource::RLIMIT_STACK as i64),
        ("POSIX_SC_ARG_MAX", SysconfVar::ARG_MAX as i64),
        ("POSIX_SC_CHILD_MAX", SysconfVar::CHILD_MAX as i64),
        ("POSIX_SC_CLK_TCK", SysconfVar::CLK_TCK as i64),
        ("POSIX_SC_PAGESIZE", SysconfVar::PAGE_SIZE as i64),
        ("POSIX_SC_OPEN_MAX", SysconfVar::OPEN_MAX as i64),
        ("POSIX_PC_LINK_MAX", PathconfVar::LINK_MAX as i64),
        ("POSIX_PC_MAX_CANON", PathconfVar::MAX_CANON as i64),
        ("POSIX_PC_MAX_INPUT", PathconfVar::MAX_INPUT as i64),
        ("POSIX_PC_NAME_MAX", PathconfVar::NAME_MAX as i64),
        ("POSIX_PC_PATH_MAX", PathconfVar::PATH_MAX as i64),
        ("POSIX_PC_PIPE_BUF", PathconfVar::PIPE_BUF as i64),
        (
            "POSIX_PC_CHOWN_RESTRICTED",
            PathconfVar::_POSIX_CHOWN_RESTRICTED as i64,
        ),
        ("POSIX_PC_NO_TRUNC", PathconfVar::_POSIX_NO_TRUNC as i64),
    ] {
        registry.constant(name, value);
    }
    #[cfg(target_vendor = "apple")]
    for (name, value) in [
        ("POSIX_RLIMIT_AS", Resource::RLIMIT_AS as i64),
        ("POSIX_RLIMIT_RSS", Resource::RLIMIT_AS as i64),
        // Darwin exposes these limit IDs, although nix omits their enum variants.
        ("POSIX_RLIMIT_MEMLOCK", 6),
        ("POSIX_SC_NPROCESSORS_CONF", 57),
        ("POSIX_SC_NPROCESSORS_ONLN", 58),
        ("POSIX_PC_ALLOC_SIZE_MIN", 16),
        ("POSIX_PC_SYMLINK_MAX", 24),
        ("POSIX_RLIMIT_NPROC", 7),
    ] {
        registry.constant(name, value);
    }
    #[cfg(target_os = "linux")]
    for (name, value) in [
        ("POSIX_RLIMIT_AS", Resource::RLIMIT_AS as i64),
        ("POSIX_RLIMIT_LOCKS", Resource::RLIMIT_LOCKS as i64),
        (
            "POSIX_SC_NPROCESSORS_CONF",
            SysconfVar::_NPROCESSORS_CONF as i64,
        ),
        (
            "POSIX_SC_NPROCESSORS_ONLN",
            SysconfVar::_NPROCESSORS_ONLN as i64,
        ),
        (
            "POSIX_PC_ALLOC_SIZE_MIN",
            PathconfVar::POSIX_ALLOC_SIZE_MIN as i64,
        ),
        ("POSIX_PC_SYMLINK_MAX", PathconfVar::SYMLINK_MAX as i64),
        ("POSIX_RLIMIT_MEMLOCK", Resource::RLIMIT_MEMLOCK as i64),
        ("POSIX_RLIMIT_MSGQUEUE", Resource::RLIMIT_MSGQUEUE as i64),
        ("POSIX_RLIMIT_NICE", Resource::RLIMIT_NICE as i64),
        ("POSIX_RLIMIT_NPROC", Resource::RLIMIT_NPROC as i64),
        ("POSIX_RLIMIT_RSS", Resource::RLIMIT_RSS as i64),
        ("POSIX_RLIMIT_RTPRIO", Resource::RLIMIT_RTPRIO as i64),
        ("POSIX_RLIMIT_RTTIME", Resource::RLIMIT_RTTIME as i64),
        (
            "POSIX_RLIMIT_SIGPENDING",
            Resource::RLIMIT_SIGPENDING as i64,
        ),
    ] {
        registry.constant(name, value);
    }
}
