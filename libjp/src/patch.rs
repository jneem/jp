use serde_yaml;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{self, prelude::*};

use crate::storage::{DigleMut, Storage};
use crate::Error;

mod change;
pub use self::change::{Change, Changes};

// This is just a wrapper around some instance of io::Write that calculates a hash of everything
// that's written.
struct HashingWriter<W: Write> {
    writer: W,
    hasher: Sha256,
}

impl<W: Write> HashingWriter<W> {
    fn new(writer: W) -> HashingWriter<W> {
        HashingWriter {
            writer: writer,
            hasher: Default::default(),
        }
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.hasher.input(buf);
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// A global identifier for a patch.
///
/// A `PatchId` is derived from a patch by hashing its contents. It must be unique: a repository
/// cannot simultaneously contain two patches with the same id.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PatchId {
    #[serde(with = "crate::Base64Slice")]
    pub(crate) data: [u8; 32],
}

impl PatchId {
    pub fn cur() -> PatchId {
        PatchId { data: [0; 32] }
    }

    pub fn is_cur(&self) -> bool {
        self.data == [0; 32]
    }

    pub fn filename(&self) -> String {
        // We encode the filename in the URL_SAFE encoding because it needs to be a valid path
        // (e.g. no slashes).
        base64::encode_config(&self.data[..], base64::URL_SAFE)
    }

    pub fn from_filename<S: ?Sized + AsRef<[u8]>>(name: &S) -> Result<PatchId, Error> {
        let data = base64::decode_config(name, base64::URL_SAFE)?;
        let mut ret = PatchId::cur();
        ret.data.copy_from_slice(&data);
        // TODO: check that the size is right
        Ok(ret)
    }
}

/// A patch is ultimately identified by its id, which is generated by hashing the contents of the
/// serialized patch. This ends up being a bit circular, because the contents of the patch might
/// actually depend on the id, and those contents in turn will affect the id. The way we break this
/// cycle is by separating "unidentified" patches (those without an id yet) from completed patches
/// with an id.
///
/// This is an unidentified patch; it does not have an id field, and any changes in the `changes`
/// array that need to refer to this patch use the all-zeros placeholder as their patch id.
///
/// This patch *cannot* be applied to a repository, because doing so would require an id. However,
/// it can be serialized to a file, and it can be turned into an identified patch.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct UnidentifiedPatch {
    pub header: PatchHeader,
    pub changes: Changes,
    pub deps: Vec<PatchId>,
}

impl UnidentifiedPatch {
    pub fn new(author: String, description: String, changes: Changes) -> UnidentifiedPatch {
        // The dependencies of this patch consist of all patches that are referred to by the list
        // of changes.
        let mut deps = HashSet::new();
        for c in &changes.changes {
            match *c {
                Change::DeleteNode { ref id } => {
                    if !id.patch.is_cur() {
                        deps.insert(id.patch.clone());
                    }
                }
                Change::NewEdge { ref src, ref dst } => {
                    if !src.patch.is_cur() {
                        deps.insert(src.patch.clone());
                    }
                    if !dst.patch.is_cur() {
                        deps.insert(dst.patch.clone());
                    }
                }
                _ => {}
            }
        }

        UnidentifiedPatch {
            header: PatchHeader {
                author,
                description,
            },
            changes,
            deps: deps.into_iter().collect(),
        }
    }

    fn set_id(self, id: PatchId) -> Patch {
        let mut ret = Patch {
            id,
            header: self.header,
            changes: self.changes,
            deps: self.deps,
        };

        ret.changes.set_patch_id(&ret.id);
        ret
    }

    pub fn write_out<W: Write>(self, writer: W) -> Result<Patch, serde_yaml::Error> {
        let mut w = HashingWriter::new(writer);
        serde_yaml::to_writer(&mut w, &self)?;

        let id = w.hasher.result();
        let mut patch_id = PatchId::cur();
        patch_id.data.copy_from_slice(&id[..]);

        Ok(self.set_id(patch_id))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq)]
pub struct Patch {
    pub id: PatchId,
    pub header: PatchHeader,
    pub changes: Changes,
    pub deps: Vec<PatchId>,
}

impl Patch {
    pub fn store_new_contents(&self, storage: &mut Storage) {
        self.changes.store_new_contents(storage);
    }

    pub fn unstore_new_contents(&self, storage: &mut Storage) {
        self.changes.unstore_new_contents(storage)
    }

    pub fn apply_to_digle(&self, digle: &mut DigleMut) {
        self.changes.apply_to_digle(digle)
    }

    pub fn unapply_to_digle(&self, digle: &mut DigleMut) {
        self.changes.unapply_to_digle(digle)
    }

    pub fn from_reader<R: Read>(input: R, id: PatchId) -> Result<Patch, Error> {
        let up: UnidentifiedPatch = serde_yaml::from_reader(input)?;
        // TODO: should we verify that the id matches the hash of the input?
        Ok(up.set_id(id))
    }

    pub fn write_out<W: Write>(&self, mut writer: W) -> Result<(), serde_yaml::Error> {
        let up = UnidentifiedPatch {
            header: self.header.clone(),
            changes: self.changes.clone(),
            deps: self.deps.clone(),
        };
        serde_yaml::to_writer(&mut writer, &up)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PatchHeader {
    pub author: String,
    pub description: String,
}
