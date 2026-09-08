use std::fs::File;
use std::io;
use std::io::Seek;
use std::io::Write;

pub(crate) fn filter(deny_network: bool) -> io::Result<File> {
    #[cfg(not(target_arch = "x86_64"))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Linux sandbox currently requires x86_64",
    ));
    #[cfg(target_arch = "x86_64")]
    {
        let mut instructions = vec![
            (0x20u16, 0u8, 0u8, 4u32),
            (0x15, 1, 0, 0xc000003e),
            (0x06, 0, 0, 0x80000000),
            (0x20, 0, 0, 0),
            (0x45, 0, 1, 0x40000000),
            (0x06, 0, 0, 0x80000000),
        ];
        let mut denied = vec![
            libc::SYS_io_uring_setup,
            libc::SYS_ptrace,
            libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev,
            libc::SYS_mount,
            libc::SYS_umount2,
            libc::SYS_unshare,
            libc::SYS_setns,
        ];
        if deny_network {
            denied.push(libc::SYS_socket);
        }
        for syscall in denied {
            instructions.push((0x15, 0, 1, syscall as u32));
            instructions.push((0x06, 0, 0, 0x00050001));
        }
        instructions.push((0x06, 0, 0, 0x7fff0000));
        let mut file = tempfile::tempfile()?;
        for (code, jt, jf, value) in instructions {
            file.write_all(&code.to_ne_bytes())?;
            file.write_all(&[jt, jf])?;
            file.write_all(&value.to_ne_bytes())?;
        }
        file.rewind()?;
        Ok(file)
    }
}
