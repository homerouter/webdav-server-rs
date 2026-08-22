//
//  Virtual Root filesystem for PROPFIND.
//
//  Shows "/" and "/user".
//
use std::path::Path;

use dav_server::davpath::DavPath;
use dav_server::fs::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsResult, FsStream, OpenOptions,
    ReadDirMeta,
};
use futures::future::{self, FutureExt};

use crate::userfs::UserFs;

#[derive(Clone)]
pub struct RootFs {
    user: String,
    fs: UserFs,
}

impl RootFs {
    pub fn new<P>(dir: P, user: Option<String>, creds: Option<(u32, u32, &[u32])>) -> Box<RootFs>
    where
        P: AsRef<Path> + Clone,
    {
        Box::new(RootFs {
            user: user.unwrap_or_default(),
            fs: *UserFs::new(dir, creds, false, false, true),
        })
    }
}

impl DavFileSystem for RootFs {
    // Only allow "/" or "/user", for both return the metadata of the UserFs root.
    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        async move {
            let b = path.as_bytes();
            if b != b"/" && &b[1..] != self.user.as_bytes() {
                return Err(FsError::NotFound);
            }
            let path = DavPath::new("/").unwrap();
            self.fs.metadata(&path).await
        }
        .boxed()
    }

    // Only return one entry: "user".
    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        Box::pin(async move {
            let mut v = Vec::new();
            if !self.user.is_empty() {
                v.push(RootFsDirEntry {
                    name: self.user.clone(),
                    meta: self.fs.metadata(path).await,
                });
            }
            let strm = futures::stream::iter(RootFsReadDir {
                iterator: v.into_iter(),
            });
            Ok(Box::pin(strm) as FsStream<Box<dyn DavDirEntry>>)
        })
    }

    // cannot open any files.
    fn open<'a>(&'a self, _path: &'a DavPath, _options: OpenOptions) -> FsFuture<'a, Box<dyn DavFile>> {
        Box::pin(future::ready(Err(FsError::NotImplemented)))
    }

    // forward quota.
    fn get_quota<'a>(&'a self) -> FsFuture<'a, (u64, Option<u64>)> {
        self.fs.get_quota()
    }
}

#[derive(Debug)]
struct RootFsReadDir {
    iterator: std::vec::IntoIter<RootFsDirEntry>,
}

impl Iterator for RootFsReadDir {
    type Item = Result<Box<dyn DavDirEntry>, FsError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iterator
            .next()
            .map(|entry| Ok(Box::new(entry) as Box<dyn DavDirEntry>))
    }
}

#[derive(Debug)]
struct RootFsDirEntry {
    meta: FsResult<Box<dyn DavMetaData>>,
    name: String,
}

impl DavDirEntry for RootFsDirEntry {
    fn metadata(&self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        Box::pin(future::ready(self.meta.clone()))
    }

    fn name(&self) -> Vec<u8> {
        self.name.as_bytes().to_vec()
    }

    fn is_dir(&self) -> FsFuture<'_, bool> {
        Box::pin(future::ready(Ok(true)))
    }
}
