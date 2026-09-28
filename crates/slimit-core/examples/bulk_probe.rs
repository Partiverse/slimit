//! 诊断用：增量位图实验，定位每个属性在记录中的落点。
//! `cargo run -p slimit-core --example bulk_probe <dir>`
use std::ffi::CString;

#[repr(C)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

fn run(dir: &str, common: u32, file: u32) {
    let cpath = CString::new(dir.as_bytes()).unwrap();
    let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
    assert!(fd >= 0);
    let mut attrlist = AttrList {
        bitmapcount: 5,
        reserved: 0,
        commonattr: common,
        volattr: 0,
        dirattr: 0,
        fileattr: file,
        forkattr: 0,
    };
    let mut buf = vec![0u8; 1 << 16];
    let n = unsafe {
        libc::getattrlistbulk(
            fd,
            &mut attrlist as *mut _ as *mut _,
            buf.as_mut_ptr() as *mut _,
            buf.len(),
            0,
        )
    };
    if n <= 0 {
        eprintln!("common=0x{common:08x} file=0x{file:08x}: n={n}, errno={}", std::io::Error::last_os_error());
        return;
    }
    // 只 dump 第一条 record
    let len = u32::from_ne_bytes(buf[0..4].try_into().unwrap()) as usize;
    let mut words = Vec::new();
    for j in (0..len).step_by(4) {
        let end = (j + 4).min(len);
        let bytes = &buf[j..end];
        if bytes.len() == 4 {
            words.push(format!("+{j:02}:0x{:08x}", u32::from_ne_bytes(bytes.try_into().unwrap())));
        } else {
            words.push(format!("+{j:02}:{}", String::from_utf8_lossy(bytes)));
        }
    }
    eprintln!("common=0x{common:08x} file=0x{file:08x} len={len}: {}", words.join(" "));
    unsafe { libc::close(fd) };
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp".into());
    let returned = 0x8000_0000u32;
    // 从最小位图逐步增加
    run(&dir, returned | 0x1, 0); // NAME only
    run(&dir, returned | 0x1 | 0x2, 0); // +DEVID
    run(&dir, returned | 0x1 | 0x8, 0); // +OBJTYPE
    run(&dir, returned | 0x1 | 0x0200_0000, 0); // +FILEID
    run(&dir, returned | 0x1, 0x4); // NAME + FILE_ALLOCSIZE
    run(&dir, returned | 0x1, 0x200); // NAME + FILE_DATALENGTH
}
