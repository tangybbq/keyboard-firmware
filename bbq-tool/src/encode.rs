//! Dictionary memory encoding.

use std::{collections::BTreeMap, io::Write};

use bbq_steno::{memdict::{GroupEntry, RawDictGroup, RawMemDict, HEADER_MAX_BYTES}, stroke::StenoWord};
use byteorder::{LittleEndian, WriteBytesExt};

use crate::Result;

/// Target endianness.
type Target = LittleEndian;

pub struct DictBuilder {
    dicts: Vec<OneDict>,
    offset: usize,
}

/// Internally, store the raw dict, and the padded data.
struct OneDict {
    raw: RawMemDict,
    data: Vec<u8>,
}

impl DictBuilder {
    pub fn new() -> DictBuilder {
        DictBuilder {
            dicts: Vec::new(),
            offset: HEADER_MAX_BYTES,
        }
    }

    pub fn add(&mut self, dict: &BTreeMap<StenoWord, String>) {
        let mut entry = RawMemDict::default();
        let mut data = Vec::new();

        entry.size = dict.len() as u32;

        let starting_offset = self.offset;

        // Write out all of the keys, consecutively, collecting offset and
        // length values for them.
        let base = self.offset;
        let mut keys = Vec::new();
        let mut offset = 0;
        for k in dict.keys() {
            keys.push(TablePos {
                offset,
                length: k.0.len(),
            });
            offset += k.0.len();

            // Push out the strokes themselves.
            for st in &k.0 {
                data.write_u32::<Target>(st.into_raw()).unwrap();
                self.offset += 4;
            }
        }
        self.pad_buffer(&mut data, 8);
        entry.keys_offset = base as u32;
        entry.keys_length = (self.offset - base) as u32;

        assert_eq!(data.len(), self.offset - starting_offset);

        // Write out the key table.
        entry.key_pos_offset = self.offset as u32;
        for pos in &keys {
            data.write_u32::<Target>(pos.encoded()).unwrap();
            self.offset += 4;
        }
        self.pad_buffer(&mut data, 8);

        assert_eq!(data.len(), self.offset - starting_offset);

        // Add all of the text strings, tracking their offsets.
        let base = self.offset;
        let mut texts = Vec::new();
        let mut offset = 0;
        for v in dict.values() {
            texts.push(TablePos {
                offset,
                length: v.len(),
            });
            offset += v.len();

            // Append the raw text.
            data.extend_from_slice(v.as_bytes());
            self.offset += v.len();
        }
        self.pad_buffer(&mut data, 8);
        entry.text_offset = base as u32;
        entry.text_length = (self.offset - base) as u32;

        assert_eq!(data.len(), self.offset - starting_offset);

        // Finally output a table of the offsets and lengths of the
        // strings.
        entry.text_table_offset = self.offset as u32;
        for pos in &texts {
            data.write_u32::<Target>(pos.encoded()).unwrap();
            self.offset += 4;
        }

        assert_eq!(data.len(), self.offset - starting_offset);

        // Pad the whole thing to 16 byte
        self.pad_buffer(&mut data, 16);

        assert_eq!(data.len(), self.offset - starting_offset);

        self.dicts.push(OneDict {
            raw: entry,
            data,
        });
    }

    fn pad_buffer(&mut self, data: &mut Vec<u8>, padding: usize) {
        while data.len() % padding > 0 {
            data.push(0xff);
            self.offset += 1;
        }
    }

    pub fn write_group<W: Write>(self, writer: &mut W) -> Result<()> {
        let (raws, datas): (Vec<_>, Vec<_>) = self.dicts
                            .into_iter()
                            .map(|d| (GroupEntry::Memory(d.raw), d.data))
                            .unzip();

        let header = RawDictGroup {
            dicts: raws,
        };
        let mut header_bytes: Vec<u8> = Vec::new();

        minicbor::encode(&header, &mut header_bytes).unwrap();

        if header_bytes.len() > HEADER_MAX_BYTES {
            panic!("HEADER_MAX_BYTES is insufficient, must be at least {}",
                   header_bytes.len());
        }

        // Pad the header to the actual size.
        while header_bytes.len() < HEADER_MAX_BYTES {
            header_bytes.push(0xFF);
        }

        writer.write_all(&header_bytes)?;

        for data in datas {
            writer.write_all(&data)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
struct TablePos {
    offset: usize,
    length: usize,
}

impl TablePos {
    fn encoded(&self) -> u32 {
        // Encode by putting the length as the upper 8 bits, and the offset
        // in the lower.
        if self.length >= 256 {
            println!("Bogus: {:?}", self);
        }
        assert!(self.length < (1 << 8));
        assert!(self.offset < (1 << 24));
        ((self.length << 24) as u32) | (self.offset as u32)
    }
}
