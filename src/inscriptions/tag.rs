use super::*;

#[derive(Copy, Clone)]
pub(crate) enum Tag {
  Pointer,
  #[allow(unused)]
  Unbound,

  ContentType,
  Parent,
  Metadata,
  Metaprotocol,
  ContentEncoding,
  Delegate,
  #[allow(unused)]
  Nop,
}

impl Tag {
  fn is_chunked(self) -> bool {
    matches!(self, Self::Metadata)
  }

  pub(crate) fn bytes(self) -> &'static [u8] {
    match self {
      Self::Pointer => &[2],
      Self::Unbound => &[66],

      Self::ContentType => &[1],
      Self::Parent => &[3],
      Self::Metadata => &[5],
      Self::Metaprotocol => &[7],
      Self::ContentEncoding => &[9],
      Self::Delegate => &[11],
      Self::Nop => &[255],
    }
  }

  pub(crate) fn push_tag(self, tmp: script::Builder) -> script::Builder {
    let bytes = self.bytes();

    if bytes.len() == 1 && (1..17).contains(&bytes[0]) {
      // if it's a single byte between 1 and 16, use a PUSHNUM opcode
      tmp.push_opcode(
        match bytes[0] {
           1 => opcodes::all::OP_PUSHNUM_1,   2 => opcodes::all::OP_PUSHNUM_2,   3 => opcodes::all::OP_PUSHNUM_3,   4 => opcodes::all::OP_PUSHNUM_4,
           5 => opcodes::all::OP_PUSHNUM_5,   6 => opcodes::all::OP_PUSHNUM_6,   7 => opcodes::all::OP_PUSHNUM_7,   8 => opcodes::all::OP_PUSHNUM_8,
           9 => opcodes::all::OP_PUSHNUM_9,  10 => opcodes::all::OP_PUSHNUM_10, 11 => opcodes::all::OP_PUSHNUM_11, 12 => opcodes::all::OP_PUSHNUM_12,
          13 => opcodes::all::OP_PUSHNUM_13, 14 => opcodes::all::OP_PUSHNUM_14, 15 => opcodes::all::OP_PUSHNUM_15, 16 => opcodes::all::OP_PUSHNUM_16,
           _ => panic!("unreachable"),
        })
    } else {
      // otherwise use a PUSHBYTES opcode
      tmp.push_slice::<&script::PushBytes>(bytes.try_into().unwrap())
    }
  }

  pub(crate) fn encode(self, builder: &mut script::Builder, value: &Option<Vec<u8>>) {
    if let Some(value) = value {
      let mut tmp = script::Builder::new();
      mem::swap(&mut tmp, builder);

      if self.is_chunked() {
        for chunk in value.chunks(MAX_SCRIPT_ELEMENT_SIZE) {
          tmp = self.push_tag(tmp)
            .push_slice::<&script::PushBytes>(chunk.try_into().unwrap());
        }
      } else {
        tmp = self.push_tag(tmp)
          .push_slice::<&script::PushBytes>(value.as_slice().try_into().unwrap());
      }

      mem::swap(&mut tmp, builder);
    }
  }

  pub(crate) fn remove_field(self, fields: &mut BTreeMap<&[u8], Vec<&[u8]>>) -> Option<Vec<u8>> {
    if self.is_chunked() {
      let value = fields.remove(self.bytes())?;

      if value.is_empty() {
        None
      } else {
        Some(value.into_iter().flatten().cloned().collect())
      }
    } else {
      let values = fields.get_mut(self.bytes())?;

      if values.is_empty() {
        None
      } else {
        let value = values.remove(0).to_vec();

        if values.is_empty() {
          fields.remove(self.bytes());
        }

        Some(value)
      }
    }
  }
}
