use std::any::Any;
use std::path::{Path, PathBuf};
#[cfg(feature = "quota")]
use std::sync::OnceLock;
#[cfg(feature = "quota")]
use std::time::Duration;

use dav_server::davpath::DavPath;
#[cfg(feature = "quota")]
use dav_server::fs::FsError;
use dav_server::fs::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsFuture, FsStream, OpenOptions, ReadDirMeta,
};
use dav_server::localfs::LocalFs;
#[cfg(feature = "quota")]
use log::debug;

#[cfg(feature = "quota")]
use crate::cache;
use crate::suid::UgidSwitch;

#[derive(Clone)]
pub struct UserFs {
    pub fs: LocalFs,
    #[allow(dead_code)]
    basedir: PathBuf,
    #[allow(dead_code)]
    uid: u32,
}

impl UserFs {
    pub fn new(
        dir: impl AsRef<Path>,
        target_creds: Option<(u32, u32, &[u32])>,
        public: bool,
        case_insensitive: bool,
        macos: bool,
    ) -> Box<UserFs> {
        // uid is used for quota() calls.
        let uid = target_creds.as_ref().map(|ugid| ugid.0).unwrap_or(0);

        // set up the LocalFs hooks for uid switching.
        let switch = UgidSwitch::new(target_creds);
        let blocking_guard = Box::new(move || Box::new(switch.guard()) as Box<dyn Any>);

        Box::new(UserFs {
            basedir: dir.as_ref().to_path_buf(),
            fs: *LocalFs::new_with_fs_access_guard(
                dir,
                public,
                case_insensitive,
                macos,
                Some(blocking_guard),
            ),
            uid,
        })
    }
}

impl DavFileSystem for UserFs {
    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        self.fs.metadata(path)
    }

    fn symlink_metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        self.fs.symlink_metadata(path)
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        self.fs.read_dir(path, meta)
    }

    fn open<'a>(&'a self, path: &'a DavPath, options: OpenOptions) -> FsFuture<'a, Box<dyn DavFile>> {
        self.fs.open(path, options)
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        self.fs.create_dir(path)
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        self.fs.remove_dir(path)
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        self.fs.remove_file(path)
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        self.fs.rename(from, to)
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        self.fs.copy(from, to)
    }

    #[cfg(feature = "quota")]
    fn get_quota<'a>(&'a self) -> FsFuture<'a, (u64, Option<u64>)> {
        use fs_quota::*;
        use futures::future::FutureExt;

        static QCACHE: OnceLock<cache::Cache<PathBuf, FsQuota>> = OnceLock::new();
        let cache = QCACHE.get_or_init(|| cache::Cache::new().maxage(Duration::from_secs(30)));

        async move {
            let mut key = self.basedir.clone();
            key.push(self.uid.to_string());
            let r = match cache.get(&key) {
                Some(r) => {
                    debug!("get_quota for {:?}: from cache", key);
                    r
                },
                None => {
                    let path = self.basedir.clone();
                    let uid = self.uid;
                    let r = self
                        .fs
                        .blocking(move || {
                            FsQuota::check(&path, Some(uid)).map_err(|_| FsError::GeneralFailure)
                        })
                        .await?;
                    debug!("get_quota for {:?}: insert to cache", key);
                    cache.insert(key, r)
                },
            };
            Ok((r.bytes_used, r.bytes_limit))
        }
        .boxed()
    }
}
