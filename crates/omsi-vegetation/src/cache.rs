//! Shared tree variants: their keys (archetype, variant, LOD), seeds and preset hashes, and a
//! versioned, checksummed binary file per generated `TreeMesh` under a caller-given cache root.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Vec2, Vec3, Vec4};

use crate::classify::Archetype;
use crate::generator::mesh::{Mesh, VertexAttrs};
use crate::generator::{generate, Lod, Target, TreeMesh, TreeParams};
use crate::presets::{preset, reference_target};
use crate::seed::splitmix64;

/// Bump on ANY change of generator output or of the file format (the golden test fails then).
pub const GEN_VERSION: u32 = 1;
/// Shared variants per archetype and LOD.
pub const VARIANTS: u32 = 8;

const MAGIC: &[u8; 8] = b"OMSVEG01";
const ATTR_FLOATS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VariantKey {
    pub archetype: Archetype,
    pub variant: u32,
    pub lod: Lod,
}

impl VariantKey {
    /// The generator seed: independent of the LOD, so L0/L1/L2 of one variant are one tree.
    pub fn seed(&self) -> u64 {
        splitmix64(0x5645_4745_5441_5249 ^ ((self.archetype as u64) << 32) ^ self.variant as u64)
    }

    /// E.g. "BroadleafWide-3-L1-0123456789abcdef.bin".
    pub fn file_name(&self, preset_hash: u64) -> String {
        format!(
            "{:?}-{}-{:?}-{preset_hash:016x}.bin",
            self.archetype, self.variant, self.lod
        )
    }
}

/// The variant an instance uses: high bits, the low bits already drive yaw/tint in `seed`.
pub fn variant_of(instance_seed: u64) -> u32 {
    ((instance_seed >> 32) % VARIANTS as u64) as u32
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// FNV-1a 64 over `format!("{p:?}|{t:?}")` (Debug of f32 is the shortest round-trip form).
pub fn preset_hash(p: &TreeParams, t: Target) -> u64 {
    fnv1a64(format!("{p:?}|{t:?}").as_bytes())
}

/// `<root>/v<GEN_VERSION>`
pub fn version_dir(root: &Path) -> PathBuf {
    root.join(format!("v{GEN_VERSION}"))
}

fn put_f32s(out: &mut Vec<u8>, xs: &[f32]) {
    xs.iter()
        .for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
}

fn encode_mesh(out: &mut Vec<u8>, m: &Mesh) {
    let n = m.positions.len();
    debug_assert!(m.normals.len() == n && m.uvs.len() == n);
    for c in [n, m.indices.len(), m.attrs.len()] {
        out.extend_from_slice(&(c as u32).to_le_bytes());
    }
    m.positions
        .iter()
        .for_each(|v| put_f32s(out, &v.to_array()));
    m.normals.iter().for_each(|v| put_f32s(out, &v.to_array()));
    m.uvs.iter().for_each(|v| put_f32s(out, &v.to_array()));
    for a in &m.attrs {
        put_f32s(out, &a.branch.to_array());
        put_f32s(out, &a.twig.to_array());
        put_f32s(out, &a.crown_normal.to_array());
        put_f32s(out, &[a.flutter, a.ao, a.phase, a.stiffness, a.rand]);
    }
    m.indices
        .iter()
        .for_each(|i| out.extend_from_slice(&i.to_le_bytes()));
}

pub fn encode(m: &TreeMesh) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&GEN_VERSION.to_le_bytes());
    encode_mesh(&mut out, &m.bark);
    encode_mesh(&mut out, &m.leaves);
    let sum = fnv1a64(&out);
    out.extend_from_slice(&sum.to_le_bytes());
    out
}

struct Cursor<'a>(&'a [u8]);

impl Cursor<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_at_checked(N)?;
        self.0 = rest;
        head.try_into().ok()
    }

    fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }

    fn f32(&mut self) -> Option<f32> {
        self.take().map(f32::from_le_bytes)
    }

    fn f32s<const N: usize>(&mut self) -> Option<[f32; N]> {
        let mut a = [0.0; N];
        for x in &mut a {
            *x = self.f32()?;
        }
        Some(a)
    }
}

fn decode_mesh(c: &mut Cursor) -> Option<Mesh> {
    let (n, n_idx, n_attr) = (c.u32()? as usize, c.u32()? as usize, c.u32()? as usize);
    if (n_attr != 0 && n_attr != n) || n_idx % 3 != 0 {
        return None;
    }
    // Reject absurd counts before allocating: all the data must still fit.
    let floats = n
        .checked_mul(8)?
        .checked_add(n_attr.checked_mul(ATTR_FLOATS)?)?;
    if floats.checked_add(n_idx)?.checked_mul(4)? > c.0.len() {
        return None;
    }
    let mut m = Mesh::default();
    for _ in 0..n {
        m.positions.push(Vec3::from(c.f32s()?));
    }
    for _ in 0..n {
        m.normals.push(Vec3::from(c.f32s()?));
    }
    for _ in 0..n {
        m.uvs.push(Vec2::from(c.f32s()?));
    }
    for _ in 0..n_attr {
        let a: [f32; ATTR_FLOATS] = c.f32s()?;
        m.attrs.push(VertexAttrs {
            branch: Vec4::new(a[0], a[1], a[2], a[3]),
            twig: Vec4::new(a[4], a[5], a[6], a[7]),
            crown_normal: Vec3::new(a[8], a[9], a[10]),
            flutter: a[11],
            ao: a[12],
            phase: a[13],
            stiffness: a[14],
            rand: a[15],
        });
    }
    for _ in 0..n_idx {
        m.indices.push(c.u32().filter(|&i| (i as usize) < n)?);
    }
    Some(m)
}

pub fn decode(b: &[u8]) -> Option<TreeMesh> {
    if b.len() < 8 + 4 + 8 {
        return None;
    }
    let (body, sum) = b.split_at(b.len() - 8);
    if u64::from_le_bytes(sum.try_into().ok()?) != fnv1a64(body) {
        return None;
    }
    let mut c = Cursor(body);
    if &c.take::<8>()? != MAGIC || c.u32()? != GEN_VERSION {
        return None;
    }
    let bark = decode_mesh(&mut c)?;
    let leaves = decode_mesh(&mut c)?;
    c.0.is_empty().then_some(TreeMesh { bark, leaves })
}

fn path_of(root: &Path, key: VariantKey, hash: u64) -> PathBuf {
    version_dir(root).join(key.file_name(hash))
}

/// Read + decode the key's file; a file that exists but fails to decode is removed.
pub fn load(root: &Path, key: VariantKey, preset_hash: u64) -> Option<TreeMesh> {
    let path = path_of(root, key, preset_hash);
    let bytes = fs::read(&path).ok()?;
    let m = decode(&bytes);
    if m.is_none() {
        let _ = fs::remove_file(&path);
    }
    m
}

/// Atomic: write a unique temp file in the same directory, then rename over the final name.
pub fn store(root: &Path, key: VariantKey, preset_hash: u64, m: &TreeMesh) -> io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let path = path_of(root, key, preset_hash);
    fs::create_dir_all(version_dir(root))?;
    let tmp = path.with_extension(format!(
        "bin.tmp{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    // Only a file we created is ours to remove.
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let result = f
        .write_all(&encode(m))
        .and_then(|_| fs::rename(&tmp, &path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// What a worker calls: `None` if the archetype has no preset or reference target.
/// Returns the mesh and whether it came from disk; `root == None` disables the cache.
pub fn get_or_generate(root: Option<&Path>, key: VariantKey) -> Option<(TreeMesh, bool)> {
    let p = preset(key.archetype)?;
    let t = reference_target(key.archetype)?;
    let Some(root) = root else {
        return Some((generate(&p, t, key.seed(), key.lod), false));
    };
    let hash = preset_hash(&p, t);
    if let Some(m) = load(root, key, hash) {
        return Some((m, true));
    }
    let m = generate(&p, t, key.seed(), key.lod);
    let _ = store(root, key, hash, &m);
    Some((m, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BW: Archetype = Archetype::BroadleafWide;
    const LODS: [Lod; 3] = [Lod::L0, Lod::L1, Lod::L2];

    fn key(a: Archetype, variant: u32, lod: Lod) -> VariantKey {
        VariantKey {
            archetype: a,
            variant,
            lod,
        }
    }

    fn sample() -> TreeMesh {
        get_or_generate(None, key(Archetype::Shrub, 0, Lod::L2))
            .unwrap()
            .0
    }

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omsi-veg-cache-test-{}-{name}", std::process::id()))
    }

    /// Recomputes the trailing checksum after a manual edit.
    fn reseal(b: &mut Vec<u8>) {
        b.truncate(b.len() - 8);
        let sum = fnv1a64(b);
        b.extend_from_slice(&sum.to_le_bytes());
    }

    #[test]
    fn roundtrip_all_lods() {
        for a in [BW, Archetype::Pine] {
            for lod in LODS {
                let m = get_or_generate(None, key(a, 0, lod)).unwrap().0;
                assert_eq!(decode(&encode(&m)), Some(m));
            }
        }
    }

    #[test]
    fn decode_rejects_damage() {
        let good = encode(&sample());
        assert!(decode(&good).is_some());
        assert!(decode(&good[..good.len() - 1]).is_none());
        let mut b = good.clone();
        b[good.len() / 2] ^= 1;
        assert!(decode(&b).is_none());
        let mut b = good.clone();
        b[0] ^= 1;
        reseal(&mut b);
        assert!(decode(&b).is_none());
        let mut b = good.clone();
        b[8..12].copy_from_slice(&(GEN_VERSION + 1).to_le_bytes());
        reseal(&mut b);
        assert!(decode(&b).is_none());
        let mut b = good;
        b.insert(b.len() - 8, 0);
        reseal(&mut b);
        assert!(decode(&b).is_none());
    }

    #[test]
    fn decode_rejects_bad_index() {
        let mesh = |last: u32| Mesh {
            positions: vec![Vec3::ZERO; 3],
            normals: vec![Vec3::Y; 3],
            uvs: vec![Vec2::ZERO; 3],
            indices: vec![0, 1, last],
            attrs: vec![],
        };
        let tree = |last| TreeMesh {
            bark: mesh(last),
            leaves: Mesh::default(),
        };
        assert!(decode(&encode(&tree(2))).is_some());
        assert!(decode(&encode(&tree(3))).is_none());
    }

    #[test]
    fn store_then_load() {
        let root = test_root("store");
        let (k, m) = (key(Archetype::Shrub, 1, Lod::L2), sample());
        store(&root, k, 7, &m).unwrap();
        assert_eq!(load(&root, k, 7), Some(m));
        let dir = version_dir(&root);
        assert!(dir.join(k.file_name(7)).is_file());
        for e in fs::read_dir(&dir).unwrap() {
            assert!(!e.unwrap().file_name().to_string_lossy().contains(".tmp"));
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn load_removes_corrupt_file() {
        let root = test_root("corrupt");
        let k = key(BW, 2, Lod::L0);
        let path = path_of(&root, k, 9);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"garbage").unwrap();
        assert!(load(&root, k, 9).is_none());
        assert!(!path.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn get_or_generate_uses_disk() {
        let root = test_root("disk");
        let k = key(Archetype::Shrub, 3, Lod::L1);
        let (m, from_disk) = get_or_generate(Some(&root), k).unwrap();
        assert!(!from_disk);
        let (m2, from_disk) = get_or_generate(Some(&root), k).unwrap();
        assert!(from_disk);
        assert_eq!(m, m2);
        let direct = generate(
            &preset(k.archetype).unwrap(),
            reference_target(k.archetype).unwrap(),
            k.seed(),
            k.lod,
        );
        assert_eq!(get_or_generate(None, k), Some((direct, false)));
        assert!(get_or_generate(Some(&root), key(Archetype::None, 0, Lod::L0)).is_none());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn seeds_and_names_distinct() {
        let (mut seeds, mut names) = (Vec::new(), Vec::new());
        for (a, _, _) in crate::presets::TARGETS {
            for v in 0..VARIANTS {
                seeds.push(key(a, v, Lod::L0).seed());
                for lod in LODS {
                    assert_eq!(key(a, v, lod).seed(), key(a, v, Lod::L0).seed());
                    names.push(key(a, v, lod).file_name(1));
                }
            }
        }
        let (ns, nn) = (seeds.len(), names.len());
        seeds.sort_unstable();
        seeds.dedup();
        names.sort();
        names.dedup();
        assert_eq!((seeds.len(), names.len()), (ns, nn));
        assert_eq!(
            key(BW, 3, Lod::L1).file_name(0x0123_4567_89ab_cdef),
            "BroadleafWide-3-L1-0123456789abcdef.bin"
        );
    }

    #[test]
    fn preset_hash_tracks_params() {
        let h = |a| preset_hash(&preset(a).unwrap(), reference_target(a).unwrap());
        assert_eq!(h(BW), h(BW));
        assert_ne!(h(BW), h(Archetype::Pine));
        let mut p = preset(BW).unwrap();
        let t = reference_target(BW).unwrap();
        let before = preset_hash(&p, t);
        p.level[1].length += 0.01;
        assert_ne!(before, preset_hash(&p, t));
    }

    #[test]
    fn variant_of_in_range() {
        let mut hit = [false; VARIANTS as usize];
        for i in 0..1000 {
            let v = variant_of(splitmix64(i));
            assert!(v < VARIANTS);
            hit[v as usize] = true;
        }
        assert!(hit.iter().all(|&h| h));
    }

    // If this fails the generator output changed: bump GEN_VERSION and update the digest.
    #[test]
    fn generator_golden() {
        let mut d = 0u64;
        for (a, _, _) in crate::presets::TARGETS {
            for lod in LODS {
                let k = key(a, 0, lod);
                let m = generate(
                    &preset(a).unwrap(),
                    reference_target(a).unwrap(),
                    k.seed(),
                    lod,
                );
                d = splitmix64(d ^ m.bark.digest());
                d = splitmix64(d ^ m.leaves.digest());
            }
        }
        assert_eq!(d, 0x14cdff9550d57e49);
    }
}
