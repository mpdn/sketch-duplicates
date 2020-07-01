use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use metrohash::MetroHash128;
use std::{
    hash::Hasher,
    io,
    io::{Read, Write},
    mem::size_of,
};

type Word = u32;
const WORD_BITS: u32 = 32;
const WORD_LEN_BITS: usize = 5;
const WORD_MASK: u32 = 0x55555555;

#[derive(Debug, PartialEq, Eq)]
pub struct DuplicatesSketch {
    probes: u32,
    words: Vec<Word>,
}

impl DuplicatesSketch {
    pub fn new(probes: u32, size: usize) -> DuplicatesSketch {
        assert!(probes > 0);

        let size = size / size_of::<Word>();
        let size = if size.is_power_of_two() {
            size.next_power_of_two()
        } else {
            size
        };

        DuplicatesSketch {
            probes,
            words: vec![0; size],
        }
    }

    pub fn is_compatible(&self, other: &DuplicatesSketch) -> bool {
        self.probes == other.probes && self.words.len() == other.words.len()
    }

    pub fn merge(&mut self, other: &DuplicatesSketch) {
        assert!(self.is_compatible(other));
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a |= *b | (*a & WORD_MASK).wrapping_add(*b & WORD_MASK);
        }
    }

    #[inline]
    pub fn insert(&mut self, buf: &[u8]) {
        for (word_ix, bit_ix) in self.probe_iter(buf) {
            let word = &mut self.words[word_ix];
            *word |= (*word & 1 << bit_ix).wrapping_add(1 << bit_ix);
        }
    }

    #[inline]
    pub fn has_duplicate(&self, buf: &[u8]) -> bool {
        self.probe_iter(buf)
            .all(|(word_ix, bit_ix)| self.words[word_ix] >> bit_ix & 0b11 > 1)
    }

    #[inline]
    fn probe_iter(&self, buf: &[u8]) -> impl Iterator<Item = (usize, u32)> {
        let mut hasher = MetroHash128::new();
        hasher.write(buf);
        let (hash_a, hash_b) = hasher.finish128();

        let mut hash = hash_a;
        let len = self.words.len();
        (0..self.probes).map(move |i| {
            hash = hash.wrapping_add((i as u64).wrapping_mul(hash_b));

            (
                (hash >> (WORD_LEN_BITS - 1)) as usize & (len - 1),
                (hash & (WORD_BITS / 2 - 1) as u64) as u32 * 2,
            )
        })
    }

    pub fn serialize(&self, mut file: impl Write) -> io::Result<()> {
        file.write_u32::<LittleEndian>(self.probes)?;
        file.write_u64::<LittleEndian>(self.words.len() as u64)?;

        for &word in &self.words {
            file.write_u32::<LittleEndian>(word)?;
        }

        Ok(())
    }

    pub fn deserialize(mut file: impl Read) -> io::Result<Option<DuplicatesSketch>> {
        let probes = match file.read_u32::<LittleEndian>() {
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            probes => probes?,
        };

        let mut words = vec![0; file.read_u64::<LittleEndian>()? as usize];
        file.read_u32_into::<LittleEndian>(&mut words)?;

        Ok(Some(DuplicatesSketch { probes, words }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{collection::vec, prelude::*};
    use std::{collections::HashMap, io::Cursor};

    const STRING: &[u8] = b"asdf";

    #[test]
    fn no_dup() {
        let mut sketch = DuplicatesSketch::new(16, 4096);
        sketch.insert(STRING);
        assert!(!sketch.has_duplicate(STRING));
    }

    #[test]
    fn dup() {
        let mut sketch = DuplicatesSketch::new(16, 4096);
        sketch.insert(STRING);
        sketch.insert(STRING);
        assert!(sketch.has_duplicate(STRING));
    }

    #[test]
    fn trip() {
        let mut sketch = DuplicatesSketch::new(16, 4096);
        sketch.insert(STRING);
        sketch.insert(STRING);
        sketch.insert(STRING);
        assert!(sketch.has_duplicate(STRING));
    }

    #[test]
    fn quad() {
        let mut sketch = DuplicatesSketch::new(16, 4096);
        sketch.insert(STRING);
        sketch.insert(STRING);
        sketch.insert(STRING);
        sketch.insert(STRING);
        assert!(sketch.has_duplicate(STRING));
    }

    fn check(sketch: &DuplicatesSketch, bufs: &Vec<Vec<u8>>) -> Result<(), TestCaseError> {
        let mut counts = HashMap::new();
        for buf in bufs {
            *counts.entry(buf.clone()).or_insert(0) += 1;
        }

        for buf in bufs {
            prop_assert!(*counts.get(buf).unwrap() < 2 || sketch.has_duplicate(&buf));
        }

        Ok(())
    }

    prop_compose! {
        /// Generate vecs of vecs of bytes, but make it likely for there to be duplicated buffers
        fn duplicated_bufs()
                          (len in 1..100usize)
                          (bufs in vec(vec(0..=255u8, 0..100usize), len),
                           indices in vec(0..len, 0..100usize))
                          -> Vec<Vec<u8>>
        {
            indices
                .into_iter()
                .map(|i| bufs[i].clone())
                .collect()
        }
    }

    prop_compose! {
        /// Generate multiple vecs of byte buffers, but make it likely for there to be duplicated
        /// buffers across different outer vecs
        fn duplicated_multibufs()
                               (len in 1..100usize)
                               (bufs in vec(vec(0..=255u8, 0..100usize), len),
                                indices in vec(vec(0..len, 0..100usize), 0..100usize))
                               -> Vec<Vec<Vec<u8>>>
        {
            indices
                .into_iter()
                .map(|indices| indices
                    .into_iter()
                    .map(|i| bufs[i].clone())
                    .collect())
                .collect()
        }
    }

    proptest! {
        #[test]
        fn insert(bufs in duplicated_bufs()) {
            let mut sketch = DuplicatesSketch::new(4, 1024);
            bufs.iter().for_each(|buf| sketch.insert(buf));
            check(&sketch, &bufs)?;
        }

        #[test]
        fn merge(bufs in duplicated_multibufs()) {
            let mut sketch = DuplicatesSketch::new(4, 1024);

            for bufs in &bufs {
                let mut sub_sketch = DuplicatesSketch::new(4, 1024);
                bufs.iter().for_each(|buf| sub_sketch.insert(buf));
                sketch.merge(&sub_sketch);
            }

            let merged_bufs = bufs
                .iter()
                .flat_map(|bufs| bufs.iter().cloned())
                .collect();

            check(&sketch, &merged_bufs)?;
        }

        #[test]
        fn serialize(bufs in duplicated_bufs()) {
            let mut sketch_a = DuplicatesSketch::new(4, 1024);
            bufs.iter().for_each(|buf| sketch_a.insert(buf));

            let mut buf = Vec::new();
            sketch_a.serialize(Cursor::new(&mut buf))?;
            let sketch_b = DuplicatesSketch::deserialize(Cursor::new(buf))?.unwrap();

            prop_assert_eq!(sketch_a, sketch_b);
        }
    }
}
