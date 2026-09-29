use {
  bitcoin::{
    blockdata::{opcodes, script},
    Script,
  },
  std::str,
  super::*,
};

const PROTOCOL_ID: &[u8] = b"ord";

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq, Default)]
pub(crate) struct Inscription {
  pub(crate) body: Option<Vec<u8>>,
  pub(crate) content_type: Option<Vec<u8>>,
  pub(crate) delegate: Option<Vec<u8>>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum ParsedInscription {
  None,
  Partial,
  Complete(Inscription),
}


impl Inscription {
  #[cfg(test)]
  pub(crate) fn new(content_type: Option<Vec<u8>>, body: Option<Vec<u8>>) -> Self {
    Self {
      content_type,
      body,
      delegate: None
    }
  }

  /// The parser profile `doginals-trac-1.0.2-compat-v1`: only `vin[0]`'s
  /// scriptSig of each supplied transaction is read, a continuation is the
  /// next transaction of the chain the updater keyed by previous txid, and
  /// pushes are decoded by `InscriptionParser::decode_push_datas`, including
  /// its historical PUSHDATA2/PUSHDATA4 length quirk. Every accepted
  /// inscription in existing databases was decided by exactly this code, so
  /// it must not change without a new profile and a replay; the Core-correct
  /// decoder exists only for the `dogemap-pushdata-audit` diagnostic.
  /// Vectors: tests/dogemap_parser_compatibility.rs.
  pub(crate) fn from_transactions(txs: Vec<Transaction>) -> ParsedInscription {
    let mut sig_scripts = Vec::with_capacity(txs.len());
    for i in 0..txs.len() {
      if txs[i].input.is_empty() {
        return ParsedInscription::None;
      }
      sig_scripts.push(txs[i].input[0].script_sig.clone());
    }
    InscriptionParser::parse(sig_scripts)
  }

  /// Parse the first transaction of a stored chain on its own and report how
  /// many body bytes its pieces contributed. The whole chain's body starts
  /// with exactly these bytes (the parser appends pieces and never removes
  /// them), so a prefix longer than a length bound proves the complete body
  /// exceeds it without reading the continuation transactions.
  pub(crate) fn parse_first_part(tx: &Transaction) -> (ParsedInscription, usize) {
    let Some(input) = tx.input.first() else {
      return (ParsedInscription::None, 0);
    };
    InscriptionParser::parse_with(
      std::slice::from_ref(&input.script_sig),
      InscriptionParser::decode_push_datas,
    )
  }

  /// The compat profile's outcome for these scriptSigs, as the index decides it.
  pub(crate) fn parse_script_sigs_compat(sig_scripts: &[Script]) -> ParsedInscription {
    InscriptionParser::parse_with(sig_scripts, InscriptionParser::decode_push_datas).0
  }

  /// The same envelope rules with Dogecoin Core's PUSHDATA2/PUSHDATA4 length
  /// decoding. Diagnostic only; never used to index.
  pub(crate) fn parse_script_sigs_strict(sig_scripts: &[Script]) -> ParsedInscription {
    InscriptionParser::parse_with(sig_scripts, InscriptionParser::decode_push_datas_strict).0
  }

  pub(crate) fn decode_pushes_compat(script: &Script) -> Option<Vec<Vec<u8>>> {
    InscriptionParser::decode_push_datas(script)
  }

  pub(crate) fn decode_pushes_strict(script: &Script) -> Option<Vec<Vec<u8>>> {
    InscriptionParser::decode_push_datas_strict(script)
  }

  pub(crate) fn from_file(chain: Chain, path: impl AsRef<Path>) -> Result<Self, Error> {
    let path = path.as_ref();

    let body = fs::read(path).with_context(|| format!("io error reading {}", path.display()))?;

    if let Some(limit) = chain.inscription_content_size_limit() {
      let len = body.len();
      if len > limit {
        bail!("content size of {len} bytes exceeds {limit} byte limit for {chain} inscriptions");
      }
    }

    let content_type = Media::content_type_for_path(path)?;

    Ok(Self {
      body: Some(body),
      content_type: Some(content_type.into()),
      delegate: None,
    })
  }

  fn append_reveal_script_to_builder(&self, mut builder: script::Builder) -> script::Builder {
    builder = builder
      .push_opcode(opcodes::OP_FALSE)
      .push_opcode(opcodes::all::OP_IF)
      .push_slice(PROTOCOL_ID);

    if let Some(content_type) = &self.content_type {
      builder = builder.push_slice(&[1]).push_slice(content_type);
    }

    if let Some(body) = &self.body {
      builder = builder.push_slice(&[]);
      for chunk in body.chunks(520) {
        builder = builder.push_slice(chunk);
      }
    }

    builder.push_opcode(opcodes::all::OP_ENDIF)
  }

  pub(crate) fn append_reveal_script(&self, builder: script::Builder) -> Script {
    self.append_reveal_script_to_builder(builder).into_script()
  }

  pub(crate) fn media(&self) -> Media {
    if self.body.is_none() {
      return Media::Unknown;
    }

    let Some(content_type) = self.content_type() else {
      return Media::Unknown;
    };

    let modified_content_type = content_type.replace("; ", ";").replace(" ;", ";");
    modified_content_type.parse().unwrap_or(Media::Unknown)
  }

  pub(crate) fn body(&self) -> Option<&[u8]> {
    Some(self.body.as_ref()?)
  }

  pub(crate) fn into_body(self) -> Option<Vec<u8>> {
    self.body
  }

  pub(crate) fn content_length(&self) -> Option<usize> {
    Some(self.body()?.len())
  }

  pub(crate) fn delegate(&self) -> Option<InscriptionId> {
    Self::inscription_id_field(self.delegate.as_deref())
  }

  fn inscription_id_field(field: Option<&[u8]>) -> Option<InscriptionId> {
    let value = field.as_ref()?;

    if value.len() < Txid::LEN {
      return None;
    }

    if value.len() > Txid::LEN + 4 {
      return None;
    }

    let (txid, index) = value.split_at(Txid::LEN);

    if let Some(last) = index.last() {
      // Accept fixed length encoding with 4 bytes (with potential trailing zeroes)
      // or variable length (no trailing zeroes)
      if index.len() != 4 && *last == 0 {
        return None;
      }
    }

    let txid = Txid::from_slice(txid).unwrap();

    let index = [
      index.first().copied().unwrap_or(0),
      index.get(1).copied().unwrap_or(0),
      index.get(2).copied().unwrap_or(0),
      index.get(3).copied().unwrap_or(0),
    ];

    let index = u32::from_le_bytes(index);

    Some(InscriptionId { txid, index })
  }

  pub(crate) fn content_type(&self) -> Option<&str> {
    str::from_utf8(self.content_type.as_ref()?).ok()
  }

  #[cfg(test)]
  pub(crate) fn to_witness(&self) -> Witness {
    let builder = script::Builder::new();

    let script = self.append_reveal_script(builder);

    let mut witness = Witness::new();

    witness.push(script);
    witness.push([]);

    witness
  }
}

struct InscriptionParser {}

/// A push decoder: the compat decoder that decided every indexed inscription,
/// or the Core-correct one used only for diagnostics.
type PushDecoder = fn(&Script) -> Option<Vec<Vec<u8>>>;

impl InscriptionParser {
  fn parse(sig_scripts: Vec<Script>) -> ParsedInscription {
    Self::parse_with(&sig_scripts, Self::decode_push_datas).0
  }

  /// The profile's envelope rules over `sig_scripts` with the given push
  /// decoder. Besides the outcome it returns the number of body bytes that
  /// had been assembled when the outcome was decided. An empty list is not an
  /// inscription (it used to index out of bounds).
  fn parse_with(sig_scripts: &[Script], decode: PushDecoder) -> (ParsedInscription, usize) {
    let Some(sig_script) = sig_scripts.first() else {
      return (ParsedInscription::None, 0);
    };

    let mut push_datas_vec = match decode(sig_script) {
      Some(push_datas) => push_datas,
      None => return (ParsedInscription::None, 0),
    };

    let mut push_datas = push_datas_vec.as_slice();

    // read protocol

    if push_datas.len() < 3 {
      return (ParsedInscription::None, 0);
    }

    let protocol = &push_datas[0];

    if protocol != PROTOCOL_ID {
      return (ParsedInscription::None, 0);
    }

    // read npieces

    let mut npieces = match Self::push_data_to_number(&push_datas[1]) {
      Some(n) => n,
      None => return (ParsedInscription::None, 0),
    };

    if npieces == 0 {
      return (ParsedInscription::None, 0);
    }

    // read content type

    let content_type = push_datas[2].clone();

    push_datas = &push_datas[3..];

    // read body

    let mut body = vec![];

    let mut sig_scripts = sig_scripts;

    // loop over transactions
    loop {
      // loop over chunks
      loop {
        if npieces == 0 {
          let mut fields: BTreeMap<&[u8], Vec<&[u8]>> = BTreeMap::new();

          for item in push_datas.chunks(2) {
            match item {
              [key, value] => {
                if key.len() != 1 {
                  break;
                }

                fields.entry(key).or_default().push(value)
              }
              _ => {}
            }
          }

          let delegate = Tag::Delegate.take(&mut fields);
          let body_len = body.len();
          let inscription = Inscription {
            content_type: Some(content_type),
            body: Some(body),
            delegate,
          };

          return (ParsedInscription::Complete(inscription), body_len);
        }

        if push_datas.len() < 2 {
          break;
        }

        let next = match Self::push_data_to_number(&push_datas[0]) {
          Some(n) => n,
          None => break,
        };

        if next != npieces - 1 {
          break;
        }

        body.append(&mut push_datas[1].clone());

        push_datas = &push_datas[2..];
        npieces -= 1;
      }

      if sig_scripts.len() <= 1 {
        return (ParsedInscription::Partial, body.len());
      }

      sig_scripts = &sig_scripts[1..];

      push_datas_vec = match decode(&sig_scripts[0]) {
        Some(push_datas) => push_datas,
        None => return (ParsedInscription::None, body.len()),
      };

      if push_datas_vec.len() < 2 {
        return (ParsedInscription::None, body.len());
      }

      let next = match Self::push_data_to_number(&push_datas_vec[0]) {
        Some(n) => n,
        None => return (ParsedInscription::None, body.len()),
      };

      if next != npieces - 1 {
        return (ParsedInscription::None, body.len());
      }

      push_datas = push_datas_vec.as_slice();
    }
  }

  /// The compat push decoder (profile `doginals-trac-1.0.2-compat-v1`).
  ///
  /// PUSHDATA2 and PUSHDATA4 read their length from the opcode byte and the
  /// following one (three) bytes instead of the following two (four)
  /// little-endian bytes Dogecoin Core reads (findings P-F03). `4d0001` plus
  /// 256 payload bytes is therefore a 77-byte push followed by an invalid
  /// opcode, and `4e00010000` plus 256 bytes declares 65614 bytes and fails.
  /// That quirk decided which historical scripts are inscriptions, so it is
  /// kept exactly. Every length is bounds-checked before slicing, the only
  /// allocations are copies of bytes present in the script, and the
  /// PUSHDATA4 end offset is computed with checked arithmetic so a 32-bit
  /// target rejects instead of wrapping; none of that changes an outcome on
  /// the 64-bit targets that built existing databases.
  fn decode_push_datas(script: &Script) -> Option<Vec<Vec<u8>>> {
    let mut bytes = script.as_bytes();
    let mut push_datas = vec![];

    while !bytes.is_empty() {
      // op_0
      if bytes[0] == 0 {
        push_datas.push(vec![]);
        bytes = &bytes[1..];
        continue;
      }

      // op_1 - op_16
      if bytes[0] >= 81 && bytes[0] <= 96 {
        push_datas.push(vec![bytes[0] - 80]);
        bytes = &bytes[1..];
        continue;
      }

      // op_push 1-75
      if bytes[0] >= 1 && bytes[0] <= 75 {
        let len = usize::from(bytes[0]);
        if bytes.len() < 1 + len {
          return None;
        }
        push_datas.push(bytes[1..1 + len].to_vec());
        bytes = &bytes[1 + len..];
        continue;
      }

      // op_pushdata1
      if bytes[0] == 76 {
        if bytes.len() < 2 {
          return None;
        }
        let len = usize::from(bytes[1]);
        if bytes.len() < 2 + len {
          return None;
        }
        push_datas.push(bytes[2..2 + len].to_vec());
        bytes = &bytes[2 + len..];
        continue;
      }

      // op_pushdata2, historical length (bytes[1] << 8) + bytes[0]
      if bytes[0] == 77 {
        if bytes.len() < 3 {
          return None;
        }
        let len = (usize::from(bytes[1]) << 8) + usize::from(bytes[0]);
        if bytes.len() < 3 + len {
          return None;
        }
        push_datas.push(bytes[3..3 + len].to_vec());
        bytes = &bytes[3 + len..];
        continue;
      }

      // op_pushdata4, historical length from bytes[3], bytes[2], bytes[1], bytes[0]
      if bytes[0] == 78 {
        if bytes.len() < 5 {
          return None;
        }
        let len = (usize::from(bytes[3]) << 24)
          + (usize::from(bytes[2]) << 16)
          + (usize::from(bytes[1]) << 8)
          + usize::from(bytes[0]);
        let end = 5usize.checked_add(len)?;
        if bytes.len() < end {
          return None;
        }
        push_datas.push(bytes[5..end].to_vec());
        bytes = &bytes[end..];
        continue;
      }

      return None;
    }

    Some(push_datas)
  }

  /// Dogecoin Core's push decoding (`GetScriptOp`: the length follows the
  /// opcode as LE16 or LE32 and the payload must be present) with the same
  /// opcode policy as the compat decoder: only OP_0, OP_1..OP_16 and data
  /// pushes are accepted and anything else rejects the script. Used only by
  /// `ord dogemap-pushdata-audit`.
  fn decode_push_datas_strict(script: &Script) -> Option<Vec<Vec<u8>>> {
    let mut bytes = script.as_bytes();
    let mut push_datas = vec![];

    while let Some(&opcode) = bytes.first() {
      let (header, len): (usize, usize) = match opcode {
        0 => (1, 0),
        81..=96 => {
          push_datas.push(vec![opcode - 80]);
          bytes = &bytes[1..];
          continue;
        }
        1..=75 => (1, usize::from(opcode)),
        76 => (2, usize::from(*bytes.get(1)?)),
        77 => (
          3,
          usize::from(u16::from_le_bytes([*bytes.get(1)?, *bytes.get(2)?])),
        ),
        78 => (
          5,
          usize::try_from(u32::from_le_bytes([
            *bytes.get(1)?,
            *bytes.get(2)?,
            *bytes.get(3)?,
            *bytes.get(4)?,
          ]))
          .ok()?,
        ),
        _ => return None,
      };
      let end = header.checked_add(len)?;
      push_datas.push(bytes.get(header..end)?.to_vec());
      bytes = &bytes[end..];
    }

    Some(push_datas)
  }

  fn push_data_to_number(data: &[u8]) -> Option<u64> {
    if data.len() == 0 {
      return Some(0);
    }

    if data.len() > 8 {
      return None;
    }

    let mut n: u64 = 0;
    let mut m: u64 = 0;

    for i in 0..data.len() {
      n += u64::from(data[i]) << m;
      m += 8;
    }

    return Some(n);
  }
}

#[cfg(test)]
mod tests {
  use bitcoin::hashes::hex::FromHex;

  use super::*;

  #[test]
  fn valid_with_delegate() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[0]);
    script.push(&[0]);
    script.push(&[0]);
    script.push(&[91]);
    script.push(&[32]);
    script.push(&[0; 32]);
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(Inscription {
        body: Some(vec![]),
        content_type: Some(vec![]),
        delegate: Some(vec![0; 32])
      })
    );
  }

  #[test]
  fn empty() {
    assert_eq!(
      InscriptionParser::parse(vec![Script::new()]),
      ParsedInscription::None
    );
  }

  #[test]
  fn no_inscription() {
    assert_eq!(
      InscriptionParser::parse(vec![Script::from_hex("483045022100a942753a4e036f59648469cb6ac19b33b1e423ff5ceaf93007001b54df46ca1f022025f6554a58b6fde5ff24b5e2556acc57d1d2108c0de2a14096e7ddae9c9fb96d0121034523d20080d1abe75a9fbed07b83e695db2f30e2cd89b80b154a0ed70badfc90").unwrap()]),
      ParsedInscription::None
    );
  }

  #[test]
  fn valid() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof"))
    );
  }

  #[test]
  fn valid_empty_fields() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[0]);
    script.push(&[0]);
    script.push(&[0]);
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(inscription("", ""))
    );
  }

  #[test]
  fn valid_multipart() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[82]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[81]);
    script.push(&[4]);
    script.push(b"woof");
    script.push(&[0]);
    script.push(&[5]);
    script.push(b" woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof woof"))
    );
  }

  #[test]
  fn valid_multitx() {
    let mut script1: Vec<&[u8]> = Vec::new();
    let mut script2: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[82]);
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[81]);
    script1.push(&[4]);
    script1.push(b"woof");
    script2.push(&[0]);
    script2.push(&[5]);
    script2.push(b" woof");
    assert_eq!(
      InscriptionParser::parse(vec![
        Script::from(script1.concat()),
        Script::from(script2.concat())
      ]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof woof"))
    );
  }

  #[test]
  fn valid_multitx_long() {
    let mut expected = String::new();
    let mut script_parts = vec![];

    let mut script: Vec<Vec<u8>> = Vec::new();
    script.push(vec![3]);
    script.push(b"ord".to_vec());
    const LEN: usize = 100000;
    push_number(&mut script, LEN as u64);
    script.push(vec![24]);
    script.push(b"text/plain;charset=utf-8".to_vec());

    let mut i = 0;
    while i < LEN {
      let text = format!("{}", i % 10);
      expected += text.as_str();
      push_number(&mut script, (LEN - i - 1) as u64);
      script.push(vec![1]);
      script.push(text.as_bytes().to_vec());
      i += 1;

      let text = format!("{}", i % 10);
      expected += text.as_str();
      push_number(&mut script, (LEN - i - 1) as u64);
      script.push(vec![1]);
      script.push(text.as_bytes().to_vec());
      i += 1;

      script_parts.push(script);
      script = Vec::new();
    }

    let mut scripts = vec![];
    script_parts
      .iter()
      .for_each(|script| scripts.push(Script::from(script.concat())));

    assert_eq!(
      InscriptionParser::parse(scripts),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", expected))
    );
  }

  #[test]
  fn valid_multitx_extradata() {
    let mut script1: Vec<&[u8]> = Vec::new();
    let mut script2: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[82]);
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[81]);
    script1.push(&[4]);
    script1.push(b"woof");
    script1.push(&[82]);
    script1.push(&[4]);
    script1.push(b"bark");
    script2.push(&[0]);
    script2.push(&[5]);
    script2.push(b" woof");
    assert_eq!(
      InscriptionParser::parse(vec![
        Script::from(script1.concat()),
        Script::from(script2.concat())
      ]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof woof"))
    );
  }

  #[test]
  fn invalid_multitx_missingdata() {
    let mut script1: Vec<&[u8]> = Vec::new();
    let mut script2: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[82]);
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[81]);
    script1.push(&[4]);
    script1.push(b"woof");
    script2.push(&[0]);
    assert_eq!(
      InscriptionParser::parse(vec![
        Script::from(script1.concat()),
        Script::from(script2.concat())
      ]),
      ParsedInscription::None
    );
  }

  #[test]
  fn invalid_multitx_wrongcountdown() {
    let mut script1: Vec<&[u8]> = Vec::new();
    let mut script2: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[82]);
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[81]);
    script1.push(&[4]);
    script1.push(b"woof");
    script2.push(&[81]);
    script2.push(&[5]);
    script2.push(b" woof");
    assert_eq!(
      InscriptionParser::parse(vec![
        Script::from(script1.concat()),
        Script::from(script2.concat())
      ]),
      ParsedInscription::None
    );
  }

  fn push_number(script: &mut Vec<Vec<u8>>, num: u64) {
    if num == 0 {
      script.push(vec![0]);
      return;
    }

    if num <= 16 {
      script.push(vec![(80 + num) as u8]);
      return;
    }

    if num <= 0x7f {
      script.push(vec![1]);
      script.push(vec![num as u8]);
      return;
    }

    if num <= 0x7fff {
      script.push(vec![2]);
      script.push(vec![(num % 256) as u8, (num / 256) as u8]);
      return;
    }

    if num <= 0x7fffff {
      script.push(vec![3]);
      script.push(vec![
        (num % 256) as u8,
        ((num / 256) % 256) as u8,
        (num / 256 / 256) as u8,
      ]);
      return;
    }

    panic!();
  }

  #[test]
  fn valid_long() {
    let mut expected = String::new();
    let mut script: Vec<Vec<u8>> = Vec::new();
    script.push(vec![3]);
    script.push(b"ord".to_vec());
    const LEN: usize = 100000;
    push_number(&mut script, LEN as u64);
    script.push(vec![24]);
    script.push(b"text/plain;charset=utf-8".to_vec());
    for i in 0..LEN {
      let text = format!("{}", i % 10);
      expected += text.as_str();
      push_number(&mut script, (LEN - i - 1) as u64);
      script.push(vec![1]);
      script.push(text.as_bytes().to_vec());
    }
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", expected))
    );
  }

  #[test]
  fn duplicate_field() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial,
    );
  }

  #[test]
  fn invalid_tag() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[82]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial,
    );
  }

  #[test]
  fn no_content() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial,
    );
  }

  #[test]
  fn no_content_type() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::None,
    );
  }

  #[test]
  fn valid_with_extra_data() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    script.push(&[9]);
    script.push(b"woof woof");
    script.push(&[14]);
    script.push(b"woof woof woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof"))
    );
  }

  #[test]
  fn prefix_data() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[4]);
    script.push(b"woof");
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::None,
    );
  }

  #[test]
  fn wrong_protocol() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"dog");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::None
    );
  }

  #[test]
  fn incomplete_multipart() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[82]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[81]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial
    );
  }

  #[test]
  fn bad_npieces() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[82]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[83]);
    script.push(&[4]);
    script.push(b"woof");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial
    );
  }

  #[test]
  fn extract_from_transaction() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");

    let tx = Transaction {
      version: 0,
      lock_time: bitcoin::PackedLockTime(0),
      input: vec![TxIn {
        previous_output: OutPoint::null(),
        script_sig: Script::from(script.concat()),
        sequence: Sequence(0),
        witness: Witness::new(),
      }],
      output: Vec::new(),
    };

    assert_eq!(
      Inscription::from_transactions(vec![tx]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof")),
    );
  }

  #[test]
  fn do_not_extract_from_second_input() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");

    let tx = Transaction {
      version: 0,
      lock_time: PackedLockTime(0),
      input: vec![
        TxIn {
          previous_output: OutPoint::null(),
          script_sig: Script::new(),
          sequence: Sequence(0),
          witness: Witness::new(),
        },
        TxIn {
          previous_output: OutPoint::null(),
          script_sig: Script::from(script.concat()),
          sequence: Sequence(0),
          witness: Witness::new(),
        },
      ],
      output: Vec::new(),
    };

    assert_eq!(
      Inscription::from_transactions(vec![tx]),
      ParsedInscription::None
    );
  }

  /*
  #[test]
  fn reveal_script_chunks_data() {
    assert_eq!(
      inscription("foo", [])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      7
    );

    assert_eq!(
      inscription("foo", [0; 1])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      8
    );

    assert_eq!(
      inscription("foo", [0; 520])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      8
    );

    assert_eq!(
      inscription("foo", [0; 521])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      9
    );

    assert_eq!(
      inscription("foo", [0; 1040])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      9
    );

    assert_eq!(
      inscription("foo", [0; 1041])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      10
    );
  }
  */
}
