//! The files `attachment_pre_upload` holds while plugins work on their bytes.
//!
//! The hook's subject is a file's name, size and type, and the file itself
//! stays where the person picked it. While the chain runs, a plugin in it may
//! read the file in pieces; a plugin that answers `hold` keeps it after the
//! chain, writes the file that should go in its place, or sends it to a host
//! of its own with `http.fetch`'s `body_upload`, and says what goes with
//! `uploads.finish`. Nothing here holds a whole file in memory: a read is one
//! piece from disk, a write goes straight to a file of its own, and an upload
//! is streamed from disk by the gate.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{UploadRead, UploadSend};

/// How long a written file is kept once its upload has gone: a message with it
/// may wait in the composer, and its upload reads it when it is sent.
const KEEP_WRITTEN: Duration = Duration::from_secs(24 * 60 * 60);

/// One held file.
#[derive(Debug)]
pub struct Held {
    pub room: String,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    /// The largest file the room takes, which a written file is held to: it is
    /// Discord's own limit for this account and conversation.
    pub limit: u64,
    /// The plugins asked in the chain, any of which may read while it runs.
    pub chain: Vec<PluginId>,
    /// The plugin that answered `hold`, the only one that may touch it after.
    pub holder: Option<PluginId>,
    written: Option<Written>,
}

impl Held {
    /// A file the person picked for `room`, at `path`, of `size` bytes, which
    /// a replacement is held to `limit` beside, for the plugins of `chain`.
    pub fn new(
        room: &str,
        name: &str,
        path: &Path,
        size: u64,
        limit: u64,
        chain: Vec<PluginId>,
    ) -> Self {
        Held {
            room: room.to_owned(),
            name: name.to_owned(),
            path: path.to_owned(),
            size,
            limit,
            chain,
            holder: None,
            written: None,
        }
    }
}

#[derive(Debug)]
struct Written {
    path: PathBuf,
    file: File,
    bytes: u64,
}

/// What `uploads.finish` decided, for the composer to carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    pub room: String,
    /// The file that goes, and its size, or none.
    pub file: Option<(PathBuf, u64)>,
    pub name: String,
    pub insert: Option<String>,
}

/// The held files, by the number the hook's subject names them by.
#[derive(Debug)]
pub struct Uploads {
    /// Where written files go: `<data>/plugins-attach/uploads`.
    folder: PathBuf,
    held: BTreeMap<u64, Held>,
}

impl Uploads {
    pub fn new(folder: PathBuf) -> Self {
        Uploads {
            folder,
            held: BTreeMap::new(),
        }
    }

    /// Holds a file for its chain.
    pub fn hold(&mut self, id: u64, held: Held) {
        self.held.insert(id, held);
    }

    /// The chain answered `hold`: from now on the file is this plugin's, under
    /// the name the chain gave it.
    pub fn keep(&mut self, id: u64, plugin: &PluginId, name: &str) -> Option<&Held> {
        let held = self.held.get_mut(&id)?;
        held.holder = Some(plugin.clone());
        if !name.trim().is_empty() {
            held.name = name.to_owned();
        }
        Some(held)
    }

    /// The chain answered without holding it: nothing more is read.
    pub fn release(&mut self, id: u64) {
        if let Some(held) = self.held.remove(&id)
            && let Some(written) = held.written
        {
            drop(written.file);
            let _ = std::fs::remove_file(written.path);
        }
    }

    /// The files a plugin held, which go as the person picked them once it
    /// stops: a plugin that isn't running can't finish them.
    pub fn let_go(&mut self, plugin: &PluginId) -> Vec<u64> {
        let ids: Vec<u64> = self
            .held
            .iter()
            .filter(|(_, held)| held.holder.as_ref() == Some(plugin))
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.release(*id);
        }
        ids
    }

    fn mine(&mut self, id: u64, plugin: &PluginId) -> Result<&mut Held, String> {
        let held = self
            .held
            .get_mut(&id)
            .ok_or_else(|| format!("no file is held as upload {id}"))?;
        let may = match &held.holder {
            Some(holder) => holder == plugin,
            None => held.chain.contains(plugin),
        };
        if !may {
            return Err(format!("upload {id} isn't {plugin}'s to touch"));
        }
        Ok(held)
    }

    /// One piece of the person's file, or of the plugin's written one.
    pub fn read(
        &mut self,
        plugin: &PluginId,
        request: &UploadRead,
    ) -> Result<(Vec<u8>, u64, bool), String> {
        let held = self.mine(request.upload, plugin)?;
        let length = request
            .length
            .unwrap_or(UploadRead::MAX_PIECE)
            .min(UploadRead::MAX_PIECE) as usize;
        let (mut file, size) = if request.replacement {
            let written = held
                .written
                .as_ref()
                .ok_or_else(|| "nothing was written for this upload yet".to_owned())?;
            let file = File::open(&written.path).map_err(|error| error.to_string())?;
            (file, written.bytes)
        } else {
            let file = File::open(&held.path)
                .map_err(|error| format!("{} can't be read: {error}", held.name))?;
            (file, held.size)
        };
        if request.offset > size {
            return Err(format!(
                "{} is {size} bytes, not {}",
                held.name, request.offset
            ));
        }
        file.seek(SeekFrom::Start(request.offset))
            .map_err(|error| error.to_string())?;
        let mut piece = Vec::with_capacity(length.min((size - request.offset) as usize));
        file.take(length as u64)
            .read_to_end(&mut piece)
            .map_err(|error| error.to_string())?;
        let eof = request.offset + piece.len() as u64 >= size;
        Ok((piece, size, eof))
    }

    /// The next piece of the file that goes in place of the person's. The
    /// pieces go one after another, each where the last ended, and the whole is
    /// held to the room's limit, Discord's own.
    pub fn write(
        &mut self,
        plugin: &PluginId,
        id: u64,
        offset: u64,
        bytes: &[u8],
    ) -> Result<u64, String> {
        let folder = self.folder.clone();
        let held = self.mine(id, plugin)?;
        if held.holder.is_none() {
            return Err("only a plugin that answered `hold` writes a file in its place".to_owned());
        }
        if held.written.is_none() {
            std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
            sweep(&folder);
            let path = folder.join(format!("{}-{}", uuid::Uuid::new_v4().simple(), held.name));
            let file = File::create(&path).map_err(|error| error.to_string())?;
            held.written = Some(Written {
                path,
                file,
                bytes: 0,
            });
        }
        let limit = held.limit;
        let written = held.written.as_mut().expect("made above");
        if offset != written.bytes {
            return Err(format!(
                "the next piece starts at {}, where the last ended, not {offset}",
                written.bytes
            ));
        }
        if written.bytes + bytes.len() as u64 > limit {
            return Err(format!(
                "files here can be up to {limit} bytes, as Discord takes them"
            ));
        }
        written
            .file
            .write_all(bytes)
            .map_err(|error| error.to_string())?;
        written.bytes += bytes.len() as u64;
        Ok(written.bytes)
    }

    /// The written file, or the person's, for `http.fetch` to stream: its path
    /// and size.
    pub fn body(
        &mut self,
        plugin: &PluginId,
        id: u64,
        replacement: bool,
    ) -> Result<(PathBuf, u64, String), String> {
        let held = self.mine(id, plugin)?;
        if replacement {
            let written = held
                .written
                .as_mut()
                .ok_or_else(|| "nothing was written for this upload yet".to_owned())?;
            written.file.flush().map_err(|error| error.to_string())?;
            return Ok((written.path.clone(), written.bytes, held.name.clone()));
        }
        Ok((held.path.clone(), held.size, held.name.clone()))
    }

    /// What goes, which ends the hold.
    pub fn finish(
        &mut self,
        plugin: &PluginId,
        id: u64,
        send: UploadSend,
        name: Option<String>,
        insert: Option<String>,
    ) -> Result<Finished, String> {
        let held = self.mine(id, plugin)?;
        if held.holder.is_none() {
            return Err("a file is finished by the plugin that answered `hold`".to_owned());
        }
        if send == UploadSend::Replacement && held.written.is_none() {
            return Err(
                "nothing was written for this upload, so nothing can replace it".to_owned(),
            );
        }
        let mut held = self.held.remove(&id).expect("found above");
        let file = match send {
            UploadSend::Original => Some((held.path.clone(), held.size)),
            UploadSend::Replacement => {
                let mut written = held.written.take().expect("checked above");
                written.file.flush().map_err(|error| error.to_string())?;
                Some((written.path, written.bytes))
            }
            UploadSend::Nothing => None,
        };
        if let Some(written) = held.written.take() {
            drop(written.file);
            let _ = std::fs::remove_file(written.path);
        }
        Ok(Finished {
            room: held.room,
            file,
            name: name.unwrap_or(held.name),
            insert: insert.filter(|text| !text.is_empty()),
        })
    }
}

/// Takes away written files old enough that no message waits on them.
fn sweep(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > KEEP_WRITTEN);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(name: &str) -> PluginId {
        PluginId::parse(&format!("com.example.{name}")).expect("an id")
    }

    fn read(upload: u64, offset: u64, length: Option<u32>, replacement: bool) -> UploadRead {
        UploadRead {
            upload,
            offset,
            length,
            replacement,
        }
    }

    #[test]
    fn a_held_file_is_read_in_pieces_replaced_and_finished_by_its_holder() {
        let folder = tempfile::tempdir().expect("a folder");
        let picked = folder.path().join("cat.png");
        std::fs::write(&picked, b"0123456789").expect("the file");
        let mut uploads = Uploads::new(folder.path().join("uploads"));
        let (one, two) = (plugin("one"), plugin("two"));
        uploads.hold(
            7,
            Held::new(
                "room-1",
                "cat.png",
                &picked,
                10,
                16,
                vec![one.clone(), two.clone()],
            ),
        );

        // While the chain runs, anyone in it reads; nobody else.
        let (piece, size, eof) = uploads
            .read(&two, &read(7, 4, Some(3), false))
            .expect("a read");
        assert_eq!((piece.as_slice(), size, eof), (&b"456"[..], 10, false));
        let (piece, _, eof) = uploads
            .read(&one, &read(7, 8, None, false))
            .expect("the end");
        assert_eq!((piece.as_slice(), eof), (&b"89"[..], true));
        assert!(
            uploads
                .read(&plugin("other"), &read(7, 0, None, false))
                .is_err()
        );
        assert!(
            uploads.write(&one, 7, 0, b"x").is_err(),
            "only a holder writes"
        );

        uploads.keep(7, &one, "cat.png");
        assert!(
            uploads.read(&two, &read(7, 0, None, false)).is_err(),
            "it's one's now"
        );
        assert_eq!(uploads.write(&one, 7, 0, b"abc"), Ok(3));
        assert!(
            uploads.write(&one, 7, 1, b"d").is_err(),
            "pieces go in order"
        );
        assert_eq!(uploads.write(&one, 7, 3, b"def"), Ok(6));
        assert!(
            uploads.write(&one, 7, 6, &[0; 11]).is_err(),
            "the room's limit is the limit"
        );
        let (piece, size, _) = uploads
            .read(&one, &read(7, 0, None, true))
            .expect("its own");
        assert_eq!((piece.as_slice(), size), (&b"abcdef"[..], 6));
        let (path, bytes, _) = uploads.body(&one, 7, true).expect("a body");
        assert_eq!(bytes, 6);

        let finished = uploads
            .finish(
                &one,
                7,
                UploadSend::Replacement,
                Some("cat.webp".into()),
                None,
            )
            .expect("finished");
        assert_eq!(finished.file, Some((path.clone(), 6)));
        assert_eq!(finished.name, "cat.webp");
        assert_eq!(
            std::fs::read(&path).expect("kept for the upload"),
            b"abcdef"
        );
        assert!(
            uploads.read(&one, &read(7, 0, None, false)).is_err(),
            "it's gone"
        );
    }

    #[test]
    fn a_file_that_goes_nowhere_can_leave_a_link_and_a_stopped_plugin_lets_go() {
        let folder = tempfile::tempdir().expect("a folder");
        let picked = folder.path().join("big.zip");
        std::fs::write(&picked, b"zip").expect("the file");
        let mut uploads = Uploads::new(folder.path().join("uploads"));
        let one = plugin("one");
        uploads.hold(
            1,
            Held::new("room-1", "big.zip", &picked, 3, 16, vec![one.clone()]),
        );
        uploads.keep(1, &one, "big.zip");
        assert!(
            uploads
                .finish(&one, 1, UploadSend::Replacement, None, None)
                .is_err()
        );
        let finished = uploads
            .finish(
                &one,
                1,
                UploadSend::Nothing,
                None,
                Some("https://files.example/abc".into()),
            )
            .expect("finished");
        assert_eq!(finished.file, None);
        assert_eq!(
            finished.insert.as_deref(),
            Some("https://files.example/abc")
        );

        uploads.hold(
            2,
            Held::new("room-1", "big.zip", &picked, 3, 16, vec![one.clone()]),
        );
        uploads.keep(2, &one, "big.zip");
        uploads.write(&one, 2, 0, b"x").expect("a write");
        assert_eq!(uploads.let_go(&one), vec![2]);
        assert!(uploads.body(&one, 2, false).is_err());
    }
}
