//! Creation-time protected ACLs; FAT/exFAT and unverifiable security fail closed.
use super::{File, Path, PathBuf, denied};
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Security::Authorization::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::SystemServices::{ACCESS_ALLOWED_ACE_TYPE, FILE_PERSISTENT_ACLS};
use windows_sys::Win32::System::Threading::*;

pub(super) struct Directory {
    path: PathBuf,
    handle: Option<File>,
}

struct LocalAllocation(*mut core::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut result: Vec<_> = path.as_os_str().encode_wide().collect();
    if result.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains NUL",
        ));
    }
    result.push(0);
    Ok(result)
}

// usize storage guarantees alignment for TOKEN_USER; its SID points into the buffer.
fn current_user() -> io::Result<Vec<usize>> {
    unsafe {
        let mut raw = null_mut();
        if OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut raw) == 0 {
            if GetLastError() != ERROR_NO_TOKEN
                || OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        let token = OwnedHandle::from_raw_handle(raw);
        let mut length = 0;
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length);
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(buffer)
    }
}

fn descriptor() -> io::Result<LocalAllocation> {
    let user = current_user()?;
    unsafe {
        let sid = (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid;
        let mut sid_text = null_mut();
        if ConvertSidToStringSidW(sid, &mut sid_text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let _sid_text_owner = LocalAllocation(sid_text.cast());
        let mut length = 0;
        while *sid_text.add(length) != 0 {
            length += 1;
        }
        let sid_string = String::from_utf16(std::slice::from_raw_parts(sid_text, length))
            .map_err(|_| denied())?;
        // Explicit owner and protected DACL, exactly one non-inheritable full-access ACE.
        let sddl: Vec<u16> = format!("O:{sid_string}D:P(A;;FA;;;{sid_string})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut raw = null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut raw,
            null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(LocalAllocation(raw))
    }
}

fn verify(file: &File, directory: bool) -> io::Result<()> {
    let user = current_user()?;
    unsafe {
        let handle = file.as_raw_handle();
        let mut info: BY_HANDLE_FILE_INFORMATION = zeroed();
        if GetFileInformationByHandle(handle, &mut info) == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
            || (!directory && info.nNumberOfLinks != 1)
        {
            return Err(denied());
        }
        let mut flags = 0;
        if GetVolumeInformationByHandleW(
            handle,
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            null_mut(),
            0,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        if flags & FILE_PERSISTENT_ACLS == 0 {
            return Err(denied());
        }
        let mut owner = null_mut();
        let mut acl = null_mut();
        let mut sd = null_mut();
        let status = GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut sd,
        );
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let _sd_owner = LocalAllocation(sd);
        let expected = (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid;
        if owner.is_null() || EqualSid(owner, expected) == 0 || acl.is_null() {
            return Err(denied());
        }
        let mut control = 0;
        let mut revision = 0;
        if GetSecurityDescriptorControl(sd, &mut control, &mut revision) == 0 {
            return Err(io::Error::last_os_error());
        }
        if control & SE_DACL_PROTECTED == 0 || (*acl).AceCount != 1 {
            return Err(denied());
        }
        let mut raw_ace = null_mut();
        if GetAce(acl, 0, &mut raw_ace) == 0 {
            return Err(io::Error::last_os_error());
        }
        let ace = &*raw_ace.cast::<ACCESS_ALLOWED_ACE>();
        if u32::from(ace.Header.AceType) != ACCESS_ALLOWED_ACE_TYPE
            || ace.Header.AceFlags != 0
            || ace.Mask != FILE_ALL_ACCESS
            || EqualSid((&ace.SidStart as *const u32).cast_mut().cast(), expected) == 0
        {
            return Err(denied());
        }
        Ok(())
    }
}

impl Directory {
    pub(super) fn create(path: &Path) -> io::Result<Self> {
        let descriptor = descriptor()?;
        let attrs = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let path_wide = wide(path)?;
        // The DACL is present at creation; no interval with inherited access.
        if unsafe { CreateDirectoryW(path_wide.as_ptr(), &attrs) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Self::open(path)
    }
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        let path_wide = wide(path)?;
        let raw = unsafe {
            CreateFileW(
                path_wide.as_ptr(),
                READ_CONTROL | FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { File::from_raw_handle(raw) };
        verify(&handle, true)?;
        Ok(Self {
            path: path.to_owned(),
            handle: Some(handle),
        })
    }
    fn file(&self, name: &str, create: bool) -> io::Result<File> {
        let path = wide(&self.path.join(name))?;
        let descriptor = descriptor()?;
        let attrs = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let raw = unsafe {
            CreateFileW(
                path.as_ptr(),
                READ_CONTROL | GENERIC_READ | if create { GENERIC_WRITE } else { 0 },
                FILE_SHARE_READ,
                &attrs,
                if create { CREATE_NEW } else { OPEN_EXISTING },
                FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let file = unsafe { File::from_raw_handle(raw) };
        verify(&file, false)?;
        Ok(file)
    }
    pub(super) fn create_file(&self, name: &str) -> io::Result<File> {
        self.file(name, true)
    }
    pub(super) fn open_file(&self, name: &str) -> io::Result<File> {
        self.file(name, false)
    }
    pub(super) fn publish_file(&self, staging: &str, name: &str) -> io::Result<()> {
        let staging = wide(&self.path.join(staging))?;
        let name = wide(&self.path.join(name))?;
        // No REPLACE_EXISTING: competing initialization never overwrites a session.
        if unsafe { MoveFileExW(staging.as_ptr(), name.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub(super) fn remove_file(&self, name: &str) {
        let _ = std::fs::remove_file(self.path.join(name));
    }
    pub(super) fn remove(&mut self) {
        // Reopen with delete access only for cleanup, then compare the actual
        // handle identity. Never delete a replacement at the old pathname.
        let Some(original) = self.handle.take() else {
            return;
        };
        unsafe {
            let mut before: BY_HANDLE_FILE_INFORMATION = zeroed();
            if GetFileInformationByHandle(original.as_raw_handle(), &mut before) == 0 {
                return;
            }
            drop(original);
            let Ok(path) = wide(&self.path) else {
                return;
            };
            let raw = CreateFileW(
                path.as_ptr(),
                DELETE | READ_CONTROL | FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            );
            if raw == INVALID_HANDLE_VALUE {
                return;
            }
            let cleanup = File::from_raw_handle(raw);
            let mut after: BY_HANDLE_FILE_INFORMATION = zeroed();
            if GetFileInformationByHandle(cleanup.as_raw_handle(), &mut after) == 0
                || before.dwVolumeSerialNumber != after.dwVolumeSerialNumber
                || before.nFileIndexHigh != after.nFileIndexHigh
                || before.nFileIndexLow != after.nFileIndexLow
                || verify(&cleanup, true).is_err()
            {
                return;
            }
            let mut disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            SetFileInformationByHandle(
                cleanup.as_raw_handle(),
                FileDispositionInfo,
                (&mut disposition as *mut FILE_DISPOSITION_INFO).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn native_owner_and_protected_acl_are_verified_before_dummy_write() {
        let path = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("hycli-dashboard-{}", uuid::Uuid::new_v4()));
        let mut directory = Directory::create(&path).unwrap();
        let mut file = directory.create_file("dummy.txt").unwrap();
        assert_eq!(file.metadata().unwrap().len(), 0);
        // These query native handles: effective-user owner, exactly one explicit
        // user-only full-access ACE, protected DACL, no reparse point/hardlink,
        // and FILE_PERSISTENT_ACLS on the underlying filesystem.
        assert!(verify(directory.handle.as_ref().unwrap(), true).is_ok());
        assert!(verify(&file, false).is_ok());
        file.write_all(b"dummy bytes only").unwrap();
        file.sync_all().unwrap();
        drop(file);
        directory.remove_file("dummy.txt");
        directory.remove();
        assert!(!path.exists());
    }
}
