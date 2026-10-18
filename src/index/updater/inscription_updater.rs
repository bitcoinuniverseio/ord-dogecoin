use crate::drc20::operation::{Action, InscriptionOp};
use crate::index::dogemap_feed::{self, JournalRecorder};
use crate::inscription::ParsedInscription;
use crate::sat::Sat;
use crate::sat_point::SatPoint;

use super::*;

#[cfg(feature = "tap")]
mod tap;
/// TAP_KV row holding the lean replay's next inscription number (u64 LE).
#[cfg(feature = "tap")]
const TAP_LEAN_NEXT_NUMBER_KEY: &[u8] = b"_lean/next_number";
#[cfg(feature = "tap")]
mod tap_glue;
#[cfg(feature = "tap")]
#[allow(unused_imports)]
pub(crate) use tap::{
  tap_js_json_stringify_str,
  tap_js_json_stringify_value,
  tap_js_preprocess_json_for_serde,
  tap_js_to_lowercase,
  // records
  BitmapRecord,
  DeployRecord,
  DmtElementRecord,
  MintFlatRecord,
  MintRecord,
  MintSuperflatRecord,
  PrivilegeVerifiedRecord,
  TapAccumulatorEntry,
  TapBatch,
  TapDeltaBatch,
  TapFeature,
  TapRoute,
  TapRouteIndex,
  TapRouteRebuildStats,
  TokenAuthCreateRecord,
  TokenAuthRedeemRecord,
  TradeBuyBuyerRecord,
  TradeBuySellerRecord,
  TradeOfferRecord,
  TransferInitFlatRecord,
  TransferInitRecord,
  TransferInitSuperflatRecord,
  TransferSendFlatRecord,
  TransferSendReceiverRecord,
  TransferSendSenderRecord,
  TransferSendSuperflatRecord,
  BURN_ADDRESS,
  MAX_DEC_U64_STR,
};

pub(super) struct Flotsam {
  txid: Txid,
  /// Position of `txid` in its block.
  tx_index: u32,
  inscription_id: InscriptionId,
  offset: u64,
  old_satpoint: SatPoint,
  origin: Origin,
}

#[derive(Debug, Clone)]
enum Origin {
  New { fee: u64, inscription: Inscription },
  Old(SatPoint),
}

pub(super) struct InscriptionUpdater<'a, 'tx> {
  flotsam: Vec<Flotsam>,
  pub(super) operations: HashMap<Txid, Vec<InscriptionOp>>,
  height: u32,
  id_to_satpoint: &'a mut Table<'tx, &'static InscriptionIdValue, &'static SatPointValue>,
  id_to_txids: &'a mut Table<'tx, &'static InscriptionIdValue, &'static [u8]>,
  txid_to_tx: &'a mut Table<'tx, &'static [u8], &'static [u8]>,
  partial_txid_to_txids: &'a mut Table<'tx, &'static [u8], &'static [u8]>,
  value_receiver: &'a mut Receiver<u64>,
  index_transactions: bool,
  transaction_buffer: Vec<u8>,
  transaction_id_to_transaction: &'a mut Table<'tx, &'static TxidValue, &'static [u8]>,
  id_to_entry: &'a mut Table<'tx, &'static InscriptionIdValue, InscriptionEntryValue>,
  lost_sats: u64,
  next_number: u64,
  number_to_id: &'a mut Table<'tx, u64, &'static InscriptionIdValue>,
  outpoint_to_value: &'a mut Table<'tx, &'static OutPointValue, u64>,
  outpoint_to_address: &'a mut Table<'tx, &'static OutPointValue, &'static [u8]>,
  address_to_outpoint: &'a mut MultimapTable<'tx, &'static [u8], &'static OutPointValue>,
  reward: u64,
  sat_to_inscription_id: &'a mut Table<'tx, u64, &'static InscriptionIdValue>,
  satpoint_to_id: &'a mut Table<'tx, &'static SatPointValue, &'static InscriptionIdValue>,
  timestamp: u32,
  value_cache: &'a mut HashMap<OutPoint, OutPointMapValue>,
  chain: Chain,
  /// The transaction whose outputs are being assigned and its block position.
  current_txid: Txid,
  current_tx_index: u32,
  /// Dogemap feed journal rows for this block.
  pub(super) journal: JournalRecorder,
  // TAP-DOGE engine state (ported from ord-tap; see docs/TAP-DOGE-ENGINE-PLAN.md).
  #[cfg(feature = "tap")]
  pub(super) tap_db: TapBatch<'a, 'tx>,
  #[cfg(feature = "tap")]
  pub(super) tap_delta_db: Option<TapDeltaBatch<'a, 'tx>>,
  #[cfg(feature = "tap")]
  pub(super) tap_atomic_writes: Option<Vec<String>>,
  #[cfg(feature = "tap")]
  pub(super) tap_atomic_overlay: Option<HashMap<String, Option<(Vec<u8>, serde_json::Value)>>>,
  #[cfg(feature = "tap")]
  pub(super) tap_atomic_list_len_cache: Option<HashMap<String, usize>>,
  #[cfg(feature = "tap")]
  pub(super) tap_route_index: Option<std::rc::Rc<std::cell::RefCell<TapRouteIndex>>>,
  #[cfg(feature = "tap")]
  pub(super) tap_route_index_verify: bool,
  #[cfg(feature = "tap")]
  pub(super) list_len_cache: HashMap<String, usize>,
  #[cfg(feature = "tap")]
  pub(super) block_availability_cache: HashMap<String, bool>,
  #[cfg(feature = "tap")]
  pub(super) btc_network: bitcoin::Network,
  #[cfg(feature = "tap")]
  pub(super) tap_index: &'a Index,
  /// Lean replay: outpoints whose values were requested from the fetcher this
  /// block, consumed in request order.
  #[cfg(feature = "tap")]
  pub(super) tap_prefetched: HashSet<OutPoint>,
  #[cfg(feature = "tap")]
  pub(super) profile: bool,
  #[cfg(feature = "tap")]
  pub(super) delegate_cache: HashMap<InscriptionId, bool>,
  #[cfg(feature = "tap")]
  pub(super) delegate_payload_cache: HashMap<InscriptionId, Inscription>,
  #[cfg(feature = "tap")]
  pub(super) prof_bm_tr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_bm_tr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_dmt_tr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_dmt_tr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_prv_tr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_prv_tr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_ttr_ex_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_ttr_ex_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_tsend_ex_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_tsend_ex_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_ttrade_ex_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_ttrade_ex_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_tauth_ex_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_tauth_ex_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_pra_ex_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_pra_ex_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_blk_ex_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_blk_ex_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_unblk_ex_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_unblk_ex_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_created_total_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_created_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_bm_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_bm_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_dmt_el_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_dmt_el_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_dpl_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_dpl_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_dmtmint_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_dmtmint_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_mint_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_mint_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_ttr_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_ttr_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_tsend_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_tsend_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_ttrade_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_ttrade_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_tauth_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_tauth_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_dmtdep_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_dmtdep_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_pra_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_pra_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_prv_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_prv_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_blk_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_blk_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_unblk_cr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_unblk_cr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_core_env_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_old_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_new_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_parent_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_txdb_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_addr_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_update_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_event_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_event_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_core_old_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_core_new_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_core_txdb_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_core_addr_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_core_update_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_old_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_old_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_new_ms: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_new_ct: u64,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_new_parents_us: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_new_entry_us: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_new_serialize_us: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_new_maps_us: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_new_num_us: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_new_sat_us: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_new_delegate_us: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_tap_us: u128,
  #[cfg(feature = "tap")]
  pub(super) prof_core_up_utxo_us: u128,
}

impl<'a, 'db, 'tx> InscriptionUpdater<'a, 'tx> {
  pub(super) fn new(
    height: u32,
    id_to_satpoint: &'a mut Table<'tx, &'static InscriptionIdValue, &'static SatPointValue>,
    id_to_txids: &'a mut Table<'tx, &'static InscriptionIdValue, &'static [u8]>,
    txid_to_tx: &'a mut Table<'tx, &'static [u8], &'static [u8]>,
    partial_txid_to_txids: &'a mut Table<'tx, &'static [u8], &'static [u8]>,
    value_receiver: &'a mut Receiver<u64>,
    index_transactions: bool,
    transaction_buffer: Vec<u8>,
    transaction_id_to_transaction: &'a mut Table<'tx, &'static TxidValue, &'static [u8]>,
    id_to_entry: &'a mut Table<'tx, &'static InscriptionIdValue, InscriptionEntryValue>,
    lost_sats: u64,
    number_to_id: &'a mut Table<'tx, u64, &'static InscriptionIdValue>,
    outpoint_to_value: &'a mut Table<'tx, &'static OutPointValue, u64>,
    outpoint_to_address: &'a mut Table<'tx, &'static OutPointValue, &'static [u8]>,
    address_to_outpoint: &'a mut MultimapTable<'tx, &'static [u8], &'static OutPointValue>,
    sat_to_inscription_id: &'a mut Table<'tx, u64, &'static InscriptionIdValue>,
    satpoint_to_id: &'a mut Table<'tx, &'static SatPointValue, &'static InscriptionIdValue>,
    timestamp: u32,
    value_cache: &'a mut HashMap<OutPoint, OutPointMapValue>,
    chain: Chain,
    #[cfg(feature = "tap")] tap_kv: &'a mut Table<'tx, &'static [u8], &'static [u8]>,
    #[cfg(feature = "tap")] tap_index: &'a Index,
  ) -> Result<Self> {
    let next_number = number_to_id
      .iter()?
      .rev()
      .map(|result| result.map(|(number, _id)| number.value() + 1))
      .next()
      .transpose()?
      .unwrap_or(0);

    // Lean replay records numbers only for TAP-readable inscriptions, so the
    // running counter is also kept in TAP_KV and the larger of the two wins.
    #[cfg(feature = "tap")]
    let next_number = next_number.max(
      tap_kv
        .get(TAP_LEAN_NEXT_NUMBER_KEY)?
        .and_then(|value| <[u8; 8]>::try_from(value.value()).ok())
        .map(u64::from_le_bytes)
        .unwrap_or(0),
    );

    Ok(Self {
      flotsam: Vec::new(),
      operations: HashMap::new(),
      height,
      id_to_satpoint,
      id_to_txids,
      txid_to_tx,
      partial_txid_to_txids,
      value_receiver,
      index_transactions,
      transaction_buffer,
      transaction_id_to_transaction,
      id_to_entry,
      lost_sats,
      next_number,
      number_to_id,
      outpoint_to_value,
      outpoint_to_address,
      address_to_outpoint,
      reward: Height(height).subsidy(),
      sat_to_inscription_id,
      satpoint_to_id,
      timestamp,
      value_cache,
      chain,
      current_txid: Txid::all_zeros(),
      current_tx_index: 0,
      journal: JournalRecorder::default(),
      #[cfg(feature = "tap")]
      tap_db: TapBatch::new(tap_kv),
      #[cfg(feature = "tap")]
      tap_delta_db: None,
      #[cfg(feature = "tap")]
      tap_atomic_writes: None,
      #[cfg(feature = "tap")]
      tap_atomic_overlay: None,
      #[cfg(feature = "tap")]
      tap_atomic_list_len_cache: None,
      #[cfg(feature = "tap")]
      tap_route_index: None,
      #[cfg(feature = "tap")]
      tap_route_index_verify: false,
      #[cfg(feature = "tap")]
      list_len_cache: HashMap::new(),
      #[cfg(feature = "tap")]
      block_availability_cache: HashMap::new(),
      #[cfg(feature = "tap")]
      btc_network: chain.network(),
      #[cfg(feature = "tap")]
      tap_index,
      #[cfg(feature = "tap")]
      tap_prefetched: HashSet::new(),
      #[cfg(feature = "tap")]
      profile: false,
      #[cfg(feature = "tap")]
      delegate_cache: HashMap::new(),
      #[cfg(feature = "tap")]
      delegate_payload_cache: HashMap::new(),
      #[cfg(feature = "tap")]
      prof_bm_tr_ms: 0,
      #[cfg(feature = "tap")]
      prof_bm_tr_ct: 0,
      #[cfg(feature = "tap")]
      prof_dmt_tr_ms: 0,
      #[cfg(feature = "tap")]
      prof_dmt_tr_ct: 0,
      #[cfg(feature = "tap")]
      prof_prv_tr_ms: 0,
      #[cfg(feature = "tap")]
      prof_prv_tr_ct: 0,
      #[cfg(feature = "tap")]
      prof_ttr_ex_ms: 0,
      #[cfg(feature = "tap")]
      prof_ttr_ex_ct: 0,
      #[cfg(feature = "tap")]
      prof_tsend_ex_ms: 0,
      #[cfg(feature = "tap")]
      prof_tsend_ex_ct: 0,
      #[cfg(feature = "tap")]
      prof_ttrade_ex_ms: 0,
      #[cfg(feature = "tap")]
      prof_ttrade_ex_ct: 0,
      #[cfg(feature = "tap")]
      prof_tauth_ex_ms: 0,
      #[cfg(feature = "tap")]
      prof_tauth_ex_ct: 0,
      #[cfg(feature = "tap")]
      prof_pra_ex_ms: 0,
      #[cfg(feature = "tap")]
      prof_pra_ex_ct: 0,
      #[cfg(feature = "tap")]
      prof_blk_ex_ms: 0,
      #[cfg(feature = "tap")]
      prof_blk_ex_ct: 0,
      #[cfg(feature = "tap")]
      prof_unblk_ex_ms: 0,
      #[cfg(feature = "tap")]
      prof_unblk_ex_ct: 0,
      #[cfg(feature = "tap")]
      prof_created_total_ms: 0,
      #[cfg(feature = "tap")]
      prof_created_ct: 0,
      #[cfg(feature = "tap")]
      prof_bm_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_bm_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_dmt_el_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_dmt_el_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_dpl_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_dpl_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_dmtmint_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_dmtmint_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_mint_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_mint_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_ttr_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_ttr_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_tsend_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_tsend_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_ttrade_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_ttrade_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_tauth_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_tauth_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_dmtdep_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_dmtdep_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_pra_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_pra_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_prv_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_prv_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_blk_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_blk_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_unblk_cr_ms: 0,
      #[cfg(feature = "tap")]
      prof_unblk_cr_ct: 0,
      #[cfg(feature = "tap")]
      prof_core_env_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_old_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_new_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_parent_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_txdb_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_addr_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_update_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_event_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_event_ct: 0,
      #[cfg(feature = "tap")]
      prof_core_old_ct: 0,
      #[cfg(feature = "tap")]
      prof_core_new_ct: 0,
      #[cfg(feature = "tap")]
      prof_core_txdb_ct: 0,
      #[cfg(feature = "tap")]
      prof_core_addr_ct: 0,
      #[cfg(feature = "tap")]
      prof_core_update_ct: 0,
      #[cfg(feature = "tap")]
      prof_core_up_old_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_up_old_ct: 0,
      #[cfg(feature = "tap")]
      prof_core_up_new_ms: 0,
      #[cfg(feature = "tap")]
      prof_core_up_new_ct: 0,
      #[cfg(feature = "tap")]
      prof_core_up_new_parents_us: 0,
      #[cfg(feature = "tap")]
      prof_core_up_new_entry_us: 0,
      #[cfg(feature = "tap")]
      prof_core_up_new_serialize_us: 0,
      #[cfg(feature = "tap")]
      prof_core_up_new_maps_us: 0,
      #[cfg(feature = "tap")]
      prof_core_up_new_num_us: 0,
      #[cfg(feature = "tap")]
      prof_core_up_new_sat_us: 0,
      #[cfg(feature = "tap")]
      prof_core_up_new_delegate_us: 0,
      #[cfg(feature = "tap")]
      prof_core_up_tap_us: 0,
      #[cfg(feature = "tap")]
      prof_core_up_utxo_us: 0,
    })
  }

  pub(super) fn index_transaction_inscriptions(
    &mut self,
    tx: &Transaction,
    txid: Txid,
    tx_index: u32,
    input_sat_ranges: Option<&VecDeque<(u64, u64)>>,
  ) -> Result<u64> {
    let mut inscriptions = Vec::new();
    self.current_txid = txid;
    self.current_tx_index = tx_index;

    if self.index_transactions {
      tx.consensus_encode(&mut self.transaction_buffer)
        .expect("in-memory writers don't error");
      self
        .transaction_id_to_transaction
        .insert(&txid.store(), self.transaction_buffer.as_slice())?;

      self.transaction_buffer.clear();
    }

    // Lean replay: only a transaction that moves, creates or continues an
    // inscription needs its input values; others are skipped (they only feed
    // the fee total behind the offset of an inscription lost to fees).
    #[cfg(feature = "tap")]
    let lean_relevant = !self.tap_index.tap_lean || self.tap_lean_tx_relevant(tx)?;

    let mut input_value = 0;
    for tx_in in &tx.input {
      if tx_in.previous_output.is_null() {
        input_value += Height(self.height).subsidy();
      } else {
        let result: Result<(), _> = (|| {
          for result in Index::inscriptions_on_output(self.satpoint_to_id, tx_in.previous_output)? {
            let (old_satpoint, inscription_id) = result?;
            inscriptions.push(Flotsam {
              txid,
              tx_index,
              offset: input_value + old_satpoint.offset,
              old_satpoint,
              inscription_id,
              origin: Origin::Old(old_satpoint),
            });
          }
          Ok(())
        })();

        if let Err(e) = result {
          // Propagate the error e
          return Err(e);
        }

        input_value += if let Some(map) = self.value_cache.remove(&tx_in.previous_output) {
          map.0
        } else if let Some(map) = self
          .outpoint_to_value
          .remove(&tx_in.previous_output.store())?
        {
          if let Some(address) = self
            .outpoint_to_address
            .remove(&tx_in.previous_output.store())?
          {
            self.address_to_outpoint.remove(
              address.value(),
              &tx_in.previous_output.store(),
            )?;
          } else if let Some(transaction) = self
            .transaction_id_to_transaction
            .get(&tx_in.previous_output.txid.store())?
          {
            let tx: Transaction = consensus::encode::deserialize(transaction.value())?;
            let output = tx.output[tx_in.previous_output.vout as usize].clone();
            if let Some(address_from_script) =
              self.chain.address_from_script(&output.script_pubkey).ok()
            {
              self.address_to_outpoint.remove(
                address_from_script.to_string().as_bytes(),
                &tx_in.previous_output.store(),
              )?;
            }
          }
          map.value()
        } else {
          #[cfg(feature = "tap")]
          let from_fetcher =
            !self.tap_index.tap_lean || self.tap_prefetched.remove(&tx_in.previous_output);
          #[cfg(not(feature = "tap"))]
          let from_fetcher = true;

          if from_fetcher {
            self.value_receiver.blocking_recv().ok_or_else(|| {
              anyhow!(
                "failed to get transaction for {}",
                tx_in.previous_output.txid
              )
            })?
          } else {
            #[cfg(feature = "tap")]
            {
              if lean_relevant {
                Self::tap_fetch_output_value(self.tap_index, tx_in.previous_output)?
              } else {
                0
              }
            }
            #[cfg(not(feature = "tap"))]
            {
              unreachable!()
            }
          }
        }
      }
    }

    // Profile doginals-trac-1.0.2-compat-v1 continuation rules, kept exactly
    // because every indexed inscription was decided by them: a new envelope
    // is parsed only when no existing inscription sits at input offset zero;
    // a partial chain is keyed by the previous txid of input 0, whatever its
    // vout, so any output of a partial reveal continues it (the first spender
    // processed wins); the id is the first reveal's txid with index 0; the
    // number is assigned when the completed inscription's location is
    // written, in block processing order. The first reveal's block is not
    // stored; the feed reads it from the node for multipart creations.
    // Vectors: tests/dogemap_parser_compatibility.rs.
    if inscriptions.iter().all(|flotsam| flotsam.offset != 0) {
      let previous_txid = tx.input[0].previous_output.txid;
      let previous_vout = tx.input[0].previous_output.vout;
      let previous_txid_bytes: [u8; 32] = previous_txid.into_inner();
      let mut txids_vec = vec![];

      let txs = match self
        .partial_txid_to_txids
        .get(&previous_txid_bytes.as_slice())?
      {
        Some(partial_txids) => {
          let txids = partial_txids.value();
          let mut txs = vec![];
          txids_vec = txids.to_vec();
          for i in 0..txids.len() / 32 {
            let txid = &txids[i * 32..i * 32 + 32];
            let tx_result = self.txid_to_tx.get(txid)?;
            let tx_result = tx_result.unwrap();
            let tx_buf = tx_result.value();
            let mut cursor = std::io::Cursor::new(tx_buf);
            let tx = bitcoin::Transaction::consensus_decode(&mut cursor)?;
            txs.push(tx);
          }
          txs.push(tx.clone());
          txs
        }
        None => {
          vec![tx.clone()]
        }
      };

      match Inscription::from_transactions(txs) {
        ParsedInscription::None => {
          // The database cleanup is deferred until all dependent rows are removed.
        }

        ParsedInscription::Partial => {
          let mut txid_vec = txid.into_inner().to_vec();
          txids_vec.append(&mut txid_vec);

          self
            .partial_txid_to_txids
            .remove(&previous_txid_bytes.as_slice())?;
          self
            .partial_txid_to_txids
            .insert(&txid.into_inner().as_slice(), txids_vec.as_slice())?;

          let mut tx_buf = vec![];
          tx.consensus_encode(&mut tx_buf)?;
          self
            .txid_to_tx
            .insert(&txid.into_inner().as_slice(), tx_buf.as_slice())?;
        }

        ParsedInscription::Complete(_inscription) => {
          self
            .partial_txid_to_txids
            .remove(&previous_txid_bytes.as_slice())?;

          // TAP-DOGE lean replay: stored content is only read back to resolve
          // a TAP op's delegate, and a delegate target must itself be a TAP op
          // (or a dogemap claim), so only such content is kept.
          #[cfg(feature = "tap")]
          let keep_content = !self.tap_index.tap_lean
            || Self::tap_lean_keeps_content(&_inscription);
          #[cfg(not(feature = "tap"))]
          let keep_content = true;

          if keep_content {
            let mut tx_buf = vec![];
            tx.consensus_encode(&mut tx_buf)?;
            self
              .txid_to_tx
              .insert(&txid.into_inner().as_slice(), tx_buf.as_slice())?;
          } else {
            for i in 0..txids_vec.len() / 32 {
              self.txid_to_tx.remove(&txids_vec[i * 32..i * 32 + 32])?;
            }
          }

          let mut txid_vec = txid.into_inner().to_vec();
          txids_vec.append(&mut txid_vec);

          let mut inscription_id = [0_u8; 36];
          unsafe {
            std::ptr::copy_nonoverlapping(txids_vec.as_ptr(), inscription_id.as_mut_ptr(), 32)
          }
          // Lean replay: the txid list only serves content lookups.
          if keep_content {
            self
              .id_to_txids
              .insert(&inscription_id, txids_vec.as_slice())?;
          }

          let og_inscription_id = InscriptionId {
            txid: Txid::from_slice(&txids_vec[0..32]).unwrap(),
            index: 0,
          };

          inscriptions.push(Flotsam {
            txid,
            tx_index,
            inscription_id: og_inscription_id,
            offset: 0,
            old_satpoint: SatPoint {
              outpoint: OutPoint {
                txid: previous_txid,
                vout: previous_vout,
              },
              offset: 0,
            },
            origin: Origin::New {
              fee: input_value - tx.output.iter().map(|txout| txout.value).sum::<u64>(),
              inscription: _inscription.clone(),
            },
          });
        }
      }
    };

    let is_coinbase = tx
      .input
      .first()
      .map(|tx_in| tx_in.previous_output.is_null())
      .unwrap_or_default();

    if is_coinbase {
      inscriptions.append(&mut self.flotsam);
    }

    inscriptions.sort_by_key(|flotsam| flotsam.offset);
    let mut inscriptions = inscriptions.into_iter().peekable();

    let mut output_value = 0;
    for (vout, tx_out) in tx.output.iter().enumerate() {
      let end = output_value + tx_out.value;

      while let Some(flotsam) = inscriptions.peek() {
        if flotsam.offset >= end {
          break;
        }

        let new_satpoint = SatPoint {
          outpoint: OutPoint {
            txid,
            vout: vout.try_into().unwrap(),
          },
          offset: flotsam.offset - output_value,
        };

        self.update_inscription_location(
          input_sat_ranges,
          inscriptions.next().unwrap(),
          new_satpoint,
          tx
        )?;
      }

      output_value = end;

      // Lean replay writes no address index, so the per-output Base58
      // encoding (most of its CPU time in DRC-20-era blocks) is skipped.
      #[cfg(feature = "tap")]
      let encode_address = !self.tap_index.tap_lean;
      #[cfg(not(feature = "tap"))]
      let encode_address = true;

      let address = if encode_address {
        match self.chain.address_from_script(&tx_out.script_pubkey) {
          Ok(address) => address
            .to_string()
            .as_bytes()
            .try_into()
            .unwrap_or([0u8; 34]),
          Err(_) => [0u8; 34],
        }
      } else {
        [0u8; 34]
      };

      self.value_cache.insert(
        OutPoint {
          vout: vout.try_into().unwrap(),
          txid,
        },
        (tx_out.value, address),
      );
    }

    if is_coinbase {
      for flotsam in inscriptions {
        let new_satpoint = SatPoint {
          outpoint: OutPoint::null(),
          offset: self.lost_sats + flotsam.offset - output_value,
        };
        self.update_inscription_location(input_sat_ranges, flotsam, new_satpoint, tx)?;
      }

      Ok(self.reward - output_value)
    } else {
      self.flotsam.extend(inscriptions.map(|flotsam| Flotsam {
        offset: self.reward + flotsam.offset - output_value,
        ..flotsam
      }));
      self.reward += input_value - output_value;
      Ok(0)
    }
  }

  /// Move an inscription to `new_satpoint`, an output of `tx` (the coinbase
  /// for a fee) or the lost range, and record the move in the Dogemap feed
  /// journal: the destination script and value are read from `tx` here,
  /// before a later transaction of the block can move the inscription again.
  /// A creation is journaled only when its body passes the candidate
  /// prefilter; every other move is a transfer record.
  fn update_inscription_location(
    &mut self,
    input_sat_ranges: Option<&VecDeque<(u64, u64)>>,
    flotsam: Flotsam,
    new_satpoint: SatPoint,
    tx: &Transaction,
  ) -> Result {
    let inscription_id = flotsam.inscription_id.store();

    match flotsam.origin {
      Origin::Old(old_satpoint) => {
        self.satpoint_to_id.remove(&old_satpoint.store())?;
      }
      Origin::New {
        fee,
        inscription: _,
      } => {
        // Lean replay: number->id only for TAP-readable inscriptions; the
        // counter itself persists in TAP_KV (TAP_LEAN_NEXT_NUMBER_KEY).
        #[cfg(feature = "tap")]
        let record_number = !self.tap_index.tap_lean
          || match &flotsam.origin {
            Origin::New { inscription, .. } => Self::tap_lean_keeps_content(inscription),
            Origin::Old(_) => true,
          };
        #[cfg(not(feature = "tap"))]
        let record_number = true;
        if record_number {
          self
            .number_to_id
            .insert(&self.next_number, &inscription_id)?;
        }

        let mut sat = None;
        if let Some(input_sat_ranges) = input_sat_ranges {
          let mut offset = 0;
          for (start, end) in input_sat_ranges {
            let size = end - start;
            if offset + size > flotsam.offset {
              let n = start + flotsam.offset - offset;
              self.sat_to_inscription_id.insert(&n, &inscription_id)?;
              sat = Some(Sat(n));
              break;
            }
            offset += size;
          }
        }

        // Lean replay: full entries only for inscriptions TAP can read back
        // (TAP ops and dogemap claims); every inscription keeps its number and
        // location, so numbering and the offset-zero rule stay exact.
        #[cfg(feature = "tap")]
        let store_entry = !self.tap_index.tap_lean
          || match &flotsam.origin {
            Origin::New { inscription, .. } => Self::tap_lean_keeps_content(inscription),
            Origin::Old(_) => true,
          };
        #[cfg(not(feature = "tap"))]
        let store_entry = true;
        if store_entry {
          self.id_to_entry.insert(
            &inscription_id,
            &InscriptionEntry {
              fee,
              height: self.height,
              inscription_number: self.next_number,
              sat,
              sequence_number: 0,
              timestamp: self.timestamp,
            }
            .store(),
          )?;
        }

        self.next_number += 1;
      }
    }

    // hack
    let _satpoint = new_satpoint.clone();
    // hack

    let new_satpoint = new_satpoint.store();

    self
        .operations
        .entry(flotsam.txid)
        .or_default()
        .push(InscriptionOp {
          txid: flotsam.txid,
          inscription_number: self
              .id_to_entry
              .get(&flotsam.inscription_id.store())?
              .map(|entry| InscriptionEntry::load(entry.value()).inscription_number),
          inscription_id: flotsam.inscription_id,
          action: match flotsam.origin {
            Origin::Old(_) => Action::Transfer,
            Origin::New {
              fee: _,
              ref inscription, // hack: ref added
            } => Action::New {
              inscription : inscription.clone(), // hack: clone added
            },
          },
          old_satpoint: flotsam.old_satpoint,
          new_satpoint: Some(Entry::load(new_satpoint)),
        });

    let destination = dogemap_feed::destination(tx, _satpoint)?;

    // TAP-DOGE hooks (ord-tap parity): owner "-" and value 0 for an inscription
    // lost to fees, "-" for an OP_RETURN output, else the output's address.
    #[cfg(feature = "tap")]
    {
      let (owner_address, output_value_sat, op_return) = match &destination {
        dogemap_feed::JournalDestination::Assigned {
          value,
          script_pubkey,
          ..
        } => {
          let script = Script::from(script_pubkey.clone());
          if script.is_op_return() {
            ("-".to_string(), *value, true)
          } else {
            (
              self
                .chain
                .address_from_script(&script)
                .map(|address| address.to_string())
                .unwrap_or_else(|_| "-".to_string()),
              *value,
              false,
            )
          }
        }
        dogemap_feed::JournalDestination::Lost { .. } => ("-".to_string(), 0, false),
      };
      let index = self.tap_index;
      match &flotsam.origin {
        Origin::Old(old_satpoint) => self.tap_on_inscription_transferred(
          flotsam.inscription_id,
          0,
          *old_satpoint,
          _satpoint,
          op_return,
          &owner_address,
          output_value_sat,
        ),
        Origin::New { fee, inscription } => self.tap_on_inscription_created(
          flotsam.inscription_id,
          0,
          i32::try_from(self.next_number - 1).unwrap_or(i32::MAX),
          _satpoint,
          inscription,
          &[],
          false,
          false,
          false,
          false,
          false,
          op_return,
          *fee,
          &owner_address,
          output_value_sat,
          index,
        ),
      }
    }
    match &flotsam.origin {
      Origin::Old(_) => {
        let inscription_number = self
          .id_to_entry
          .get(&inscription_id)?
          .map(|entry| InscriptionEntry::load(entry.value()).inscription_number);
        self.journal.transfer(
          flotsam.inscription_id,
          inscription_number,
          flotsam.old_satpoint,
          destination,
          self.current_txid,
          self.current_tx_index,
        );
      }
      Origin::New { inscription, .. } => self.journal.creation(
        inscription,
        flotsam.inscription_id,
        self.next_number - 1,
        flotsam.old_satpoint,
        destination,
        flotsam.txid,
        flotsam.tx_index,
      ),
    }

    self.satpoint_to_id.insert(&new_satpoint, &inscription_id)?;
    // Lean replay: the id->location lookup is kept only for inscriptions TAP
    // reads back (those with a stored entry); location tracking itself is the
    // satpoint->id table above, kept for every inscription.
    #[cfg(feature = "tap")]
    let track_id = !self.tap_index.tap_lean || self.id_to_entry.get(&inscription_id)?.is_some();
    #[cfg(not(feature = "tap"))]
    let track_id = true;
    if track_id {
      self.id_to_satpoint.insert(&inscription_id, &new_satpoint)?;
    }

    Ok(())
  }
}
