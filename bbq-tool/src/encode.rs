//! Dictionary memory encoding.

use std::{collections::BTreeMap, io::Write, mem};

use bbq_steno::{memdict::{RawDictSet, MAGIC1, MAGIC_GROUP, MAX_DICT_GROUP_SIZE}, stroke::StenoWord};
use byteorder::{LittleEndian, WriteBytesExt};

use crate::Result;

/// Target endianness.
type Target = LittleEndian;

pub fn encode_dict(dict: &BTreeMap<StenoWord, String>) -> Result<Vec<u8>> {
    let mut result = Vec::new();

    // Add a placeholder for the header.
    for _ in 0..128 {
        result.push(0);
    }
    let mut header = Vec::new();
    header.extend(MAGIC1);
    header.write_u32::<Target>(dict.len() as u32)?;

    // Write out the key table.
    let mut keys = Vec::new();
    let key_table = result.len();
    let mut offset = 0;
    for k in dict.keys() {
        // Record the key offset table.
        keys.push(TablePos {
            offset,
            length: k.0.len(),
        });
        offset += k.0.len();

        // Push out the strokes to the file.
        for st in &k.0 {
            result.write_u32::<Target>(st.into_raw())?;
        }
    }
    let new_pos = result.len();
    header.write_u32::<Target>(key_table as u32)?;
    header.write_u32::<Target>((new_pos - key_table) as u32)?;

    assert_eq!(dict.len(), keys.len());

    pad(&mut result, 8);
    let keyposes = result.len();
    header.write_u32::<Target>(keyposes as u32)?;

    for pos in &keys {
        result.write_u32::<Target>(pos.encoded())?;
    }

    pad(&mut result, 8);

    // Encode all of the text strings.
    let mut texts = Vec::new();
    let text_table = result.len();
    let mut offset = 0;
    for v in dict.values() {
        texts.push(TablePos {
            offset,
            length: v.len(),
        });
        offset += v.len();

        // Append the raw text.
        result.extend_from_slice(v.as_bytes());
    }

    pad(&mut result, 8);
    header.write_u32::<Target>(text_table as u32)?;
    header.write_u32::<Target>(offset as u32)?;

    let textposes = result.len();
    header.write_u32::<Target>(textposes as u32)?;
    for pos in &texts {
        result.write_u32::<Target>(pos.encoded())?;
    }

    pad(&mut result, 16);

    // Stamp the header in place.
    result[0..header.len()].copy_from_slice(&header);
    let mut wr = &mut result[header.len()..128];
    write!(&mut wr, "({:?}, {:?}, {:?})",
        env!("GIT_COMMIT"),
        env!("GIT_DIRTY"),
        env!("BUILD_TIMESTAMP"),
    )?;

    Ok(result)
}

/// Write out a group of dictionaries.
pub fn write_group<W: Write>(out: &mut W, dicts: &[&[u8]]) -> Result<()> {
    out.write_all(MAGIC_GROUP)?;
    out.write_u32::<Target>(dicts.len() as u32)?;

    let header_len = mem::size_of::<RawDictSet>();
    let mut offset = header_len.next_multiple_of(16);
    for d in dicts {
        out.write_u32::<Target>(offset as u32)?;
        out.write_u32::<Target>(d.len() as u32)?;
        offset += d.len();
    }

    // Just zeros for the remainder.
    for _ in dicts.len()..MAX_DICT_GROUP_SIZE {
        out.write_u32::<Target>(0)?;
        out.write_u32::<Target>(0)?;
    }

    // Pad the output.
    let len = header_len.next_multiple_of(16) - header_len;
    let buf = vec![0b0; len];
    out.write(&buf)?;

    for d in dicts {
        out.write_all(d)?;
    }

    Ok(())
}

/// Pad the buffer to align with the given alignment.
fn pad(buf: &mut Vec<u8>, align: usize) {
    while (buf.len() % align) > 0 {
        buf.push(0);
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
