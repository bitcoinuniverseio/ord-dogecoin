//! TAP glue methods on InscriptionUpdater, ported from ord-tap b8f6ea3
//! src/index/updater/inscription_updater.rs (lines 215-500, 1229-2167; CC0).

use super::*;

#[allow(dead_code)]
pub(crate) const BRC20_PRIVILEGE_AUTHORITY: &str =
  "c14d3de97cecc573d86592240ef38bf5ba298c8c2eaf68e17b99dbbeedbab7e4i0";
use super::tap::*;
use secp256k1::{
  ecdsa::{RecoverableSignature, RecoveryId, Signature as SecpSignature},
  Message, Secp256k1,
};

/// Compact header snapshot DMT mints read for any past height (ord-tap parity).
#[derive(Serialize)]
pub(crate) struct TapHeaderSnapshot {
  pub(crate) bits: u32,
  pub(crate) nonce: u32,
  pub(crate) ntx: u32,
  pub(crate) time: u32,
}

impl InscriptionUpdater<'_, '_> {
  pub(crate) fn is_dmt_nat_rewards_enabled(&self) -> bool {
    self.tap_feature_enabled(TapFeature::DmtNatRewards)
  }
  pub(crate) fn parse_inscription_id_str(s: &str) -> Option<InscriptionId> {
    InscriptionId::from_str(s).ok()
  }

  pub(crate) fn ordinal_available(&mut self, s: &str, index: &Index) -> bool {
    // Writer semantics: treat an inscription as available if it exists
    // in our local index OR if it is part of the current block being
    // processed (same-block references). This avoids rejecting valid
    // same-block references due to update ordering.
    let Some(id) = Self::parse_inscription_id_str(s) else {
      return false;
    };
    let key_s = id.to_string();
    if let Some(hit) = self.block_availability_cache.get(&key_s) {
      return *hit;
    }
    // Lean replay keeps a location for every inscription but a full entry
    // only for TAP-readable ones, so either proves the inscription exists.
    if self
      .id_to_entry
      .get(&id.store())
      .ok()
      .flatten()
      .is_some()
      || self
        .id_to_satpoint
        .get(&id.store())
        .ok()
        .flatten()
        .is_some()
    {
      self.block_availability_cache.insert(key_s, true);
      return true;
    }
    // Fallback: check flotsam entries (new inscriptions in this block so far)
    for f in &self.flotsam {
      if f.inscription_id == id {
        self.block_availability_cache.insert(key_s, true);
        return true;
      }
    }
    // Targeted fallback: check if this inscription id exists in this block by
    // scanning txids; only parse envelopes for the matching tx.
    let ok = self.id_in_current_block(&id, index);
    self.block_availability_cache.insert(key_s, ok);
    ok
  }
  pub(crate) fn id_in_current_block(&self, id: &InscriptionId, index: &Index) -> bool {
    if let Ok(Some(block)) = index.get_block_by_height(self.height) {
      for tx in &block.txdata {
        // Doginals: one inscription per reveal transaction, always index 0.
        if tx.txid() == id.txid {
          return id.index == 0;
        }
      }
    }
    false
  }

  pub(crate) fn index_dmt_nat_rewards_for_block(
    &mut self,
    coinbase: &bitcoin::Transaction,
    bits: u32,
    index: &Index,
  ) -> Result {
    // Only apply on/after the network-specific NAT rewards activation height.
    if !self.tap_feature_enabled(TapFeature::DmtNatRewards) {
      return Ok(());
    }
    // NAT ticker and keys
    let tick_lower = "dmt-nat".to_string();
    let tick_key = Self::json_stringify_lower(&tick_lower);

    // Must have a deployment and tokens-left counter
    let Some(deployed) = self
      .tap_get::<DeployRecord>(&format!("d/{}", tick_key))
      .ok()
      .flatten()
    else {
      return Ok(());
    };
    // Verify deployment inscription exists in ord view
    if !self.ordinal_available(&deployed.ins, index) {
      return Ok(());
    }
    // Tokens left
    let mut tokens_left: i128 = match self
      .tap_get::<String>(&format!("dc/{}", tick_key))
      .ok()
      .flatten()
      .and_then(|s| s.parse::<i128>().ok())
    {
      Some(v) => v,
      None => return Ok(()),
    };

    // DMT compendium sums every coinbase output value into the denominator.
    // Address resolution is a separate row filter.
    let mut tot_btc = 0.0_f64;
    let mut outs: Vec<(usize, String, u64)> = Vec::new();
    for (i, txout) in coinbase.output.iter().enumerate() {
      let val_sat = txout.value;
      tot_btc += val_sat as f64 / 100_000_000.0;
      let addr = self.resolve_owner_address(txout, index);
      if Self::trim_js_whitespace(&addr) == "-" {
        continue;
      }
      outs.push((i, addr, val_sat));
    }
    if tot_btc == 0.0 {
      return Ok(());
    }

    for (vout, address, val_sat) in outs {
      let mut amount: i128 =
        Self::compendium_nat_reward_amount(bits, val_sat, tot_btc).unwrap_or(0);

      let mut fail = false;
      // Limit and tokens-left
      let limit: i128 = deployed.lim.parse::<i128>().unwrap_or(0);
      if limit > 0 && amount > limit {
        fail = true;
      }
      if !fail {
        if tokens_left - amount < 0 {
          amount = tokens_left;
        }
        if amount <= 0 {
          fail = true;
        }
      }

      // Balance update and holder updates
      let bal_key = format!("b/{}/{}", address, tick_key);
      let mut balance: i128 = self
        .tap_get::<String>(&bal_key)
        .ok()
        .flatten()
        .and_then(|s| s.parse::<i128>().ok())
        .unwrap_or(0);
      if !fail {
        tokens_left = tokens_left.saturating_sub(amount);
        let _ = self.tap_put(&format!("dc/{}", tick_key), &tokens_left.to_string());
        balance = balance.saturating_add(amount);
        let _ = self.tap_put(&bal_key, &balance.to_string());
        // holders list
        if self
          .tap_get::<String>(&format!("he/{}/{}", address, tick_key))
          .ok()
          .flatten()
          .is_none()
        {
          let _ = self.tap_put(&format!("he/{}/{}", address, tick_key), &"".to_string());
          let _ = self.tap_set_list_record(
            &format!("h/{}", tick_key),
            &format!("hi/{}", tick_key),
            &address,
          );
        }
        // account token owned
        if self
          .tap_get::<String>(&format!("ato/{}/{}", address, tick_key))
          .ok()
          .flatten()
          .is_none()
        {
          let tick_lower_for_list =
            serde_json::from_str::<String>(&tick_key).unwrap_or_else(|_| tick_lower.clone());
          let _ = self.tap_set_list_record(
            &format!("atl/{}", address),
            &format!("atli/{}", address),
            &tick_lower_for_list,
          );
          let _ = self.tap_put(&format!("ato/{}/{}", address, tick_key), &"".to_string());
        }
        // START MINER-REWARD-SHIELD
        self.tap_mark_dmt_reward_address(&address);
        // END MINER-REWARD-SHIELD
        // mark block as minted to prevent duplicates
        let _ = self.tap_put(
          &format!("dmt-blk/{}/{}", tick_lower, self.height),
          &"".to_string(),
        );
      }

      // Record shapes (typed CBOR structs) — ins/num are None for rewards.
      // tap-writer writes rows even when the reward amount is zero and marked failed.
      let ts = self.timestamp;
      let txid = coinbase.txid().to_string();
      let val_str = (val_sat as u128).to_string();
      let mint_rec = MintRecord {
        addr: address.clone(),
        blck: self.height,
        amt: amount.to_string(),
        bal: balance.to_string(),
        tx: Some(txid.clone()),
        vo: vout as u32,
        val: val_str.clone(),
        ins: None,
        num: None,
        ts,
        fail,
        dmtblck: Some(self.height),
        dta: None,
      };
      let _ = self.tap_set_list_record(
        &format!("aml/{}/{}", address, tick_key),
        &format!("amli/{}/{}", address, tick_key),
        &mint_rec,
      );
      let flat_rec = MintFlatRecord {
        addr: mint_rec.addr.clone(),
        blck: mint_rec.blck,
        amt: mint_rec.amt.clone(),
        bal: mint_rec.bal.clone(),
        tx: Some(txid.clone()),
        vo: mint_rec.vo,
        val: mint_rec.val.clone(),
        ins: None,
        num: None,
        ts: mint_rec.ts,
        fail: mint_rec.fail,
        dmtblck: mint_rec.dmtblck,
        dta: None,
      };
      let _ = self.tap_set_list_record(
        &format!("fml/{}", tick_key),
        &format!("fmli/{}", tick_key),
        &flat_rec,
      );
      let super_rec = MintSuperflatRecord {
        tick: tick_lower.clone(),
        addr: address.clone(),
        blck: self.height,
        amt: amount.to_string(),
        bal: balance.to_string(),
        tx: Some(txid.clone()),
        vo: vout as u32,
        val: val_str,
        ins: None,
        num: None,
        ts,
        fail,
        dmtblck: Some(self.height),
        dta: None,
      };
      if let Ok(list_len) = self.tap_set_list_record("sfml", "sfmli", &super_rec) {
        let ptr = format!("sfmli/{}", list_len - 1);
        // mirror writer pointers for NAT rewards too
        let _ = self.tap_set_list_record(
          &format!("tx/mnt/{}", txid),
          &format!("txi/mnt/{}", txid),
          &ptr,
        );
        let _ = self.tap_set_list_record(
          &format!("txt/mnt/{}/{}", tick_key, txid),
          &format!("txti/mnt/{}/{}", tick_key, txid),
          &ptr,
        );
        let _ = self.tap_set_list_record(
          &format!("blck/mnt/{}", self.height),
          &format!("blcki/mnt/{}", self.height),
          &ptr,
        );
        let _ = self.tap_set_list_record(
          &format!("blckt/mnt/{}/{}", tick_key, self.height),
          &format!("blckti/mnt/{}/{}", tick_key, self.height),
          &ptr,
        );
      }
    }

    Ok(())
  }

  pub(crate) fn resolve_owner_address(&self, txout: &TxOut, index: &Index) -> String {
    if txout.script_pubkey.is_op_return() {
      return "-".to_string();
    }

    match index.chain.address_from_script(&txout.script_pubkey)
    {
      Ok(addr) => addr.to_string(),
      Err(_) => "-".to_string(),
    }
  }

  /// The TAP_KV value for `hdr/<height>`; Dogecoin DMT uses fields 4, 10 and 11.
  pub(crate) fn tap_header_row(header: &bitcoin::BlockHeader, ntx: u32) -> Result<Vec<u8>> {
    Ok(
      Self::tap_encode_value(&TapHeaderSnapshot {
        bits: header.bits,
        nonce: header.nonce,
        ntx,
        time: header.time,
      })?
      .0,
    )
  }

  /// Lean replay relevance of `tx` at the time it is indexed: an input carries
  /// an inscription, or it starts or continues a Doginal envelope (the
  /// doginals-trac-1.0.2-compat-v1 rule: only without an inscription at input
  /// offset zero).
  pub(crate) fn tap_lean_tx_relevant(&self, tx: &Transaction) -> Result<bool> {
    let mut zero_offset_inscription = false;
    for (i, tx_in) in tx.input.iter().enumerate() {
      if tx_in.previous_output.is_null() {
        continue;
      }
      for result in Index::inscriptions_on_output(self.satpoint_to_id, tx_in.previous_output)? {
        let (satpoint, _) = result?;
        if i > 0 || satpoint.offset > 0 {
          return Ok(true);
        }
        zero_offset_inscription = true;
      }
    }
    if zero_offset_inscription {
      return Ok(true);
    }
    Self::tap_lean_may_inscribe(self.partial_txid_to_txids, tx)
  }

  /// Lean replay keeps only content a TAP op could resolve through a delegate:
  /// text/plain* or application/json* whose body mentions "tap" or "dmt" in
  /// any case, or a ".dogemap" claim.
  pub(crate) fn tap_lean_keeps_content(inscription: &Inscription) -> bool {
    let text = inscription
      .content_type()
      .map(|ct| ct.starts_with("text/plain") || ct.starts_with("application/json"))
      .unwrap_or(false);
    if !text {
      return false;
    }
    let Some(body) = inscription.body() else {
      return false;
    };
    let lower = body.to_ascii_lowercase();
    lower.windows(3).any(|w| w == b"tap" || w == b"dmt") || lower.ends_with(b".dogemap")
  }

  /// Whether `tx` starts a Doginal envelope or continues a stored partial one.
  pub(crate) fn tap_lean_may_inscribe(
    partial_txid_to_txids: &impl ReadableTable<&'static [u8], &'static [u8]>,
    tx: &Transaction,
  ) -> Result<bool> {
    let Some(first) = tx.input.first() else {
      return Ok(false);
    };
    let previous: [u8; 32] = first.previous_output.txid.into_inner();
    if partial_txid_to_txids.get(&previous.as_slice())?.is_some() {
      return Ok(true);
    }
    Ok(!matches!(
      Inscription::from_transactions(vec![tx.clone()]),
      ParsedInscription::None
    ))
  }

  /// One output value read directly from the node, for a relevant input the
  /// lean prefetch did not request (its inscription arrived within this block).
  pub(crate) fn tap_fetch_output_value(index: &Index, outpoint: OutPoint) -> Result<u64> {
    let tx = index
      .client()
      .get_raw_transaction(&outpoint.txid)
      .map_err(|err| anyhow!("failed to get transaction for {}: {err}", outpoint.txid))?;
    Ok(
      tx.output
        .get(usize::try_from(outpoint.vout)?)
        .ok_or_else(|| anyhow!("missing output {outpoint}"))?
        .value,
    )
  }

  pub(crate) fn tap_finalize_block(
    &mut self,
  ) -> Result<Option<crate::index::TapExportRollingState>> {
    self.tap_db.flush()?;
    let rolling_state = self
      .tap_delta_db
      .as_mut()
      .map(|delta_db| delta_db.finalize_block())
      .transpose()?
      .flatten();
    // Clear per-block caches
    self.list_len_cache.clear();
    self.delegate_payload_cache.clear();
    Ok(rolling_state)
  }

  pub(crate) fn tap_on_inscription_created(
    &mut self,
    inscription_id: InscriptionId,
    _sequence_number: u32,
    inscription_number: i32,
    satpoint: SatPoint,
    payload: &Inscription,
    parents: &[InscriptionId],
    _cursed: bool,
    _hidden: bool,
    _unbound: bool,
    _vindicated: bool,
    _reinscription: bool,
    _op_return: bool,
    _fee: u64,
    owner_address: &str,
    output_value_sat: u64,
    index: &Index,
  ) {
    let __cr_total = std::time::Instant::now();
    // No TAP work before bitmap activation
    if !self.tap_feature_enabled(TapFeature::Bitmap) {
      return;
    }

    // Resolve effective payload by following a single-level delegate if present.
    // tap-writer fetches content via ord's content API which resolves one level of delegation.
    let mut payload_eff: Inscription = payload.clone();
    if let Some(delegate_id) = payload.delegate() {
      let __upd_new_delegate_start = if self.profile {
        Some(std::time::Instant::now())
      } else {
        None
      };
      let has_nested = if let Some(hit) = self.delegate_cache.get(&delegate_id) {
        *hit
      } else {
        let nested = match index.get_inscription_by_id(delegate_id) {
          Ok(Some(insc)) => insc.delegate().is_some(),
          _ => false,
        };
        self.delegate_cache.insert(delegate_id, nested);
        nested
      };
      if let Some(st) = __upd_new_delegate_start {
        if self.profile {
          self.prof_core_up_new_delegate_us += st.elapsed().as_micros();
        }
      }
      if has_nested {
        return;
      }
      // Cache resolved delegate payloads within the block to avoid repeated DB fetch+decode
      if let Some(cached) = self.delegate_payload_cache.get(&delegate_id) {
        payload_eff = cached.clone();
      } else if let Ok(Some(insc)) = index.get_inscription_by_id(delegate_id) {
        payload_eff = insc.clone();
        self.delegate_payload_cache.insert(delegate_id, insc);
      }
    }

    // content-type guard: parity with tap-writer
    // Accept only content types that start with 'text/plain' or 'application/json'
    let ct_ok = payload_eff
      .content_type()
      .map(|ct| ct.starts_with("text/plain") || ct.starts_with("application/json"))
      .unwrap_or(false);
    if !ct_ok {
      return;
    }

    // Before TAP start, index bitmap and BRC-20 deployments only; skip other TAP logic
    if !self.tap_feature_enabled(TapFeature::TapStart) {
      // bitmap
      let __st = std::time::Instant::now();
      self.index_bitmap_created(
        inscription_id,
        inscription_number,
        satpoint,
        &payload_eff,
        owner_address,
        output_value_sat,
      );
      if self.profile {
        self.prof_bm_cr_ms += __st.elapsed().as_millis();
        self.prof_bm_cr_ct += 1;
      }

      // BRC-20 deployments
      let __st = std::time::Instant::now();
      self.index_deployments(
        inscription_id,
        inscription_number,
        satpoint,
        &payload_eff,
        owner_address,
        output_value_sat,
      );
      if self.profile {
        self.prof_dpl_cr_ms += __st.elapsed().as_millis();
        self.prof_dpl_cr_ct += 1;
      }
      return;
    }

    self.apply_token_trade_emergency_patch();

    let __st = std::time::Instant::now();
    self.index_bitmap_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_bm_cr_ms += __st.elapsed().as_millis();
      self.prof_bm_cr_ct += 1;
    }

    // DMT element creation (string inscriptions ending with .element)
    let __st = std::time::Instant::now();
    self.index_dmt_element_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_dmt_el_cr_ms += __st.elapsed().as_millis();
      self.prof_dmt_el_cr_ct += 1;
    }

    let __st = std::time::Instant::now();
    self.index_deployments(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_dpl_cr_ms += __st.elapsed().as_millis();
      self.prof_dpl_cr_ct += 1;
    }

    // DMT mint
    let __st = std::time::Instant::now();
    self.index_dmt_mint(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
      parents,
      index,
    );
    if self.profile {
      self.prof_dmtmint_cr_ms += __st.elapsed().as_millis();
      self.prof_dmtmint_cr_ct += 1;
    }

    // TAP token mints (new inscriptions only)
    let __st = std::time::Instant::now();
    self.index_mints(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_mint_cr_ms += __st.elapsed().as_millis();
      self.prof_mint_cr_ct += 1;
    }

    // TAP token transfers (initial transfer inscription)
    let __st = std::time::Instant::now();
    self.index_token_transfer_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_ttr_cr_ms += __st.elapsed().as_millis();
      self.prof_ttr_cr_ct += 1;
    }

    // TAP token send (internal send intent)
    let __st = std::time::Instant::now();
    self.index_token_send_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_tsend_cr_ms += __st.elapsed().as_millis();
      self.prof_tsend_cr_ct += 1;
    }

    // TAP token trade (internal trade intent)
    let __st = std::time::Instant::now();
    self.index_token_trade_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_ttrade_cr_ms += __st.elapsed().as_millis();
      self.prof_ttrade_cr_ct += 1;
    }

    // TAP token auth (create/cancel or immediate redeem)
    let __st = std::time::Instant::now();
    self.index_token_auth_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_tauth_cr_ms += __st.elapsed().as_millis();
      self.prof_tauth_cr_ct += 1;
    }

    // DMT deploy
    let __st = std::time::Instant::now();
    self.index_dmt_deploy(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
      index,
    );
    if self.profile {
      self.prof_dmtdep_cr_ms += __st.elapsed().as_millis();
      self.prof_dmtdep_cr_ct += 1;
    }

    // TAP privilege auth (create/cancel; accumulate only on creation)
    let __st = std::time::Instant::now();
    self.index_privilege_auth_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_pra_cr_ms += __st.elapsed().as_millis();
      self.prof_pra_cr_ct += 1;
    }

    // TAP transferables block/unblock (accumulate on creation)
    let __st = std::time::Instant::now();
    self.index_block_transferables_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_blk_cr_ms += __st.elapsed().as_millis();
      self.prof_blk_cr_ct += 1;
    }
    let __st = std::time::Instant::now();
    self.index_unblock_transferables_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_unblk_cr_ms += __st.elapsed().as_millis();
      self.prof_unblk_cr_ct += 1;
    }

    // TAP privilege verification (verify on creation without accumulator)
    let __st = std::time::Instant::now();
    self.index_privilege_verify_created(
      inscription_id,
      inscription_number,
      satpoint,
      &payload_eff,
      owner_address,
      output_value_sat,
    );
    if self.profile {
      self.prof_prv_cr_ms += __st.elapsed().as_millis();
      self.prof_prv_cr_ct += 1;
    }
    if self.profile {
      self.prof_created_total_ms += __cr_total.elapsed().as_millis();
      self.prof_created_ct += 1;
    }
  }

  pub(crate) fn tap_on_inscription_transferred(
    &mut self,
    inscription_id: InscriptionId,
    _sequence_number: u32,
    _old_satpoint: SatPoint,
    new_satpoint: SatPoint,
    _op_return: bool,
    owner_address: &str,
    output_value_sat: u64,
  ) {
    // No TAP work before bitmap activation
    if !self.tap_feature_enabled(TapFeature::Bitmap) {
      return;
    }

    // Before TAP start, only bitmap transfers are relevant; route directly
    if !self.tap_feature_enabled(TapFeature::TapStart) {
      self.index_bitmap_transferred(
        inscription_id,
        _sequence_number,
        new_satpoint,
        owner_address,
        output_value_sat,
        None,
      );
      return;
    }

    self.apply_token_trade_emergency_patch();

    if let Some(route_index) = &self.tap_route_index {
      let (route, ready) = {
        let route_index = route_index.borrow();
        (
          route_index.route_for(inscription_id),
          route_index.is_ready(),
        )
      };
      if self.tap_route_index_verify && ready {
        let slow_route = self.tap_classify_transfer_route_slow(inscription_id);
        assert_eq!(
          route, slow_route,
          "tap route index verification failed at height {} for inscription {}: fast={:?} slow={:?}",
          self.height, inscription_id, route, slow_route
        );
        self.tap_dispatch_transfer_slow(
          inscription_id,
          _sequence_number,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        return;
      }
      if let Some(route) = route {
        self.tap_dispatch_transfer_route(
          route,
          inscription_id,
          _sequence_number,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        return;
      }
      if ready {
        return;
      }
    }

    // Slow fallback for tests and safety if exact route index is unavailable.
    self.tap_dispatch_transfer_slow(
      inscription_id,
      _sequence_number,
      new_satpoint,
      owner_address,
      output_value_sat,
    );
  }

  pub(crate) fn tap_classify_transfer_route_slow(
    &mut self,
    inscription_id: InscriptionId,
  ) -> Option<TapRoute> {
    if let Some(kind) = self
      .tap_get::<String>(&format!("kind/{}", inscription_id))
      .ok()
      .flatten()
    {
      match kind.as_str() {
        "bm" => {
          let block = self
            .tap_get::<String>(&format!("bmh/{}", inscription_id))
            .ok()
            .flatten()
            .as_deref()
            .and_then(TapRouteIndex::bitmap_block_from_mapping);
          return Some(TapRoute::Bitmap { block });
        }
        "dmtmh" => return Some(TapRoute::DmtMint),
        "prvins" => return Some(TapRoute::Privilege),
        "tl" => return Some(TapRoute::TransferLink),
        _ => {}
      }
    } else {
      if let Some(mapped) = self
        .tap_get::<String>(&format!("bmh/{}", inscription_id))
        .ok()
        .flatten()
      {
        return Some(TapRoute::Bitmap {
          block: TapRouteIndex::bitmap_block_from_mapping(&mapped),
        });
      }
      if self
        .tap_db
        .get(format!("dmtmh/{}", inscription_id).as_bytes())
        .ok()
        .flatten()
        .is_some()
      {
        return Some(TapRoute::DmtMint);
      }
      if self
        .tap_db
        .get(format!("prvins/{}", inscription_id).as_bytes())
        .ok()
        .flatten()
        .is_some()
      {
        return Some(TapRoute::Privilege);
      }
      if let Some(val) = self
        .tap_db
        .get(format!("tl/{}", inscription_id).as_bytes())
        .ok()
        .flatten()
      {
        if let Ok(ptr) = ciborium::de::from_reader::<String, _>(std::io::Cursor::new(&val)) {
          if !ptr.is_empty() {
            return Some(TapRoute::TransferLink);
          }
        }
      }
    }

    if self
      .tap_get::<TapAccumulatorEntry>(&format!("a/{}", inscription_id))
      .ok()
      .flatten()
      .is_some()
    {
      return Some(TapRoute::Accumulator);
    }

    None
  }

  pub(crate) fn tap_dispatch_transfer_slow(
    &mut self,
    inscription_id: InscriptionId,
    _sequence_number: u32,
    new_satpoint: SatPoint,
    owner_address: &str,
    output_value_sat: u64,
  ) {
    if let Some(kind) = self
      .tap_get::<String>(&format!("kind/{}", inscription_id))
      .ok()
      .flatten()
    {
      match kind.as_str() {
        "bm" => {
          let __st = std::time::Instant::now();
          self.index_bitmap_transferred(
            inscription_id,
            _sequence_number,
            new_satpoint,
            owner_address,
            output_value_sat,
            None,
          );
          if self.profile {
            self.prof_bm_tr_ms += __st.elapsed().as_millis();
            self.prof_bm_tr_ct += 1;
          }
          return;
        }
        "dmtmh" => {
          let __st = std::time::Instant::now();
          self.index_dmt_mint_transferred(
            inscription_id,
            _sequence_number,
            new_satpoint,
            owner_address,
            output_value_sat,
          );
          if self.profile {
            self.prof_dmt_tr_ms += __st.elapsed().as_millis();
            self.prof_dmt_tr_ct += 1;
          }
          return;
        }
        "prvins" => {
          let __st = std::time::Instant::now();
          self.index_privilege_verify_transferred(
            inscription_id,
            _sequence_number,
            new_satpoint,
            owner_address,
            output_value_sat,
          );
          if self.profile {
            self.prof_prv_tr_ms += __st.elapsed().as_millis();
            self.prof_prv_tr_ct += 1;
          }
          return;
        }
        "tl" => {
          let __st = std::time::Instant::now();
          self.index_token_transfer_executed(
            inscription_id,
            _sequence_number,
            new_satpoint,
            owner_address,
            output_value_sat,
          );
          if self.profile {
            self.prof_ttr_ex_ms += __st.elapsed().as_millis();
            self.prof_ttr_ex_ct += 1;
          }
          return;
        }
        _ => {}
      }
    } else {
      // Lazy detection by presence; set kind for future fast routing
      if self
        .tap_db
        .get(format!("bmh/{}", inscription_id).as_bytes())
        .ok()
        .flatten()
        .is_some()
      {
        let _ = self.tap_put(&format!("kind/{}", inscription_id), &"bm".to_string());
        let __st = std::time::Instant::now();
        self.index_bitmap_transferred(
          inscription_id,
          _sequence_number,
          new_satpoint,
          owner_address,
          output_value_sat,
          None,
        );
        if self.profile {
          self.prof_bm_tr_ms += __st.elapsed().as_millis();
          self.prof_bm_tr_ct += 1;
        }
        return;
      }
      if self
        .tap_db
        .get(format!("dmtmh/{}", inscription_id).as_bytes())
        .ok()
        .flatten()
        .is_some()
      {
        let _ = self.tap_put(&format!("kind/{}", inscription_id), &"dmtmh".to_string());
        let __st = std::time::Instant::now();
        self.index_dmt_mint_transferred(
          inscription_id,
          _sequence_number,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_dmt_tr_ms += __st.elapsed().as_millis();
          self.prof_dmt_tr_ct += 1;
        }
        return;
      }
      if self
        .tap_db
        .get(format!("prvins/{}", inscription_id).as_bytes())
        .ok()
        .flatten()
        .is_some()
      {
        let _ = self.tap_put(&format!("kind/{}", inscription_id), &"prvins".to_string());
        let __st = std::time::Instant::now();
        self.index_privilege_verify_transferred(
          inscription_id,
          _sequence_number,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_prv_tr_ms += __st.elapsed().as_millis();
          self.prof_prv_tr_ct += 1;
        }
        return;
      }
      if let Some(val) = self
        .tap_db
        .get(format!("tl/{}", inscription_id).as_bytes())
        .ok()
        .flatten()
      {
        if let Ok(ptr) = ciborium::de::from_reader::<String, _>(std::io::Cursor::new(&val)) {
          if !ptr.is_empty() {
            let _ = self.tap_put(&format!("kind/{}", inscription_id), &"tl".to_string());
            let __st = std::time::Instant::now();
            self.index_token_transfer_executed(
              inscription_id,
              _sequence_number,
              new_satpoint,
              owner_address,
              output_value_sat,
            );
            if self.profile {
              self.prof_ttr_ex_ms += __st.elapsed().as_millis();
              self.prof_ttr_ex_ct += 1;
            }
            return;
          }
        }
      }
    }

    self.tap_execute_accumulator_for_inscription(
      inscription_id,
      new_satpoint,
      owner_address,
      output_value_sat,
    );

    // Fallback: nothing else to do here. Accumulator dispatch above takes care of
    // single-op execution and deletion when applicable.
  }

  pub(crate) fn tap_dispatch_transfer_route(
    &mut self,
    route: TapRoute,
    inscription_id: InscriptionId,
    sequence_number: u32,
    new_satpoint: SatPoint,
    owner_address: &str,
    output_value_sat: u64,
  ) {
    match route {
      TapRoute::Bitmap { block } => {
        let __st = std::time::Instant::now();
        self.index_bitmap_transferred(
          inscription_id,
          sequence_number,
          new_satpoint,
          owner_address,
          output_value_sat,
          block,
        );
        if self.profile {
          self.prof_bm_tr_ms += __st.elapsed().as_millis();
          self.prof_bm_tr_ct += 1;
        }
      }
      TapRoute::DmtMint => {
        let __st = std::time::Instant::now();
        self.index_dmt_mint_transferred(
          inscription_id,
          sequence_number,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_dmt_tr_ms += __st.elapsed().as_millis();
          self.prof_dmt_tr_ct += 1;
        }
      }
      TapRoute::Privilege => {
        let __st = std::time::Instant::now();
        self.index_privilege_verify_transferred(
          inscription_id,
          sequence_number,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_prv_tr_ms += __st.elapsed().as_millis();
          self.prof_prv_tr_ct += 1;
        }
      }
      TapRoute::TransferLink => {
        let __st = std::time::Instant::now();
        self.index_token_transfer_executed(
          inscription_id,
          sequence_number,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_ttr_ex_ms += __st.elapsed().as_millis();
          self.prof_ttr_ex_ct += 1;
        }
      }
      TapRoute::Accumulator => {
        self.tap_execute_accumulator_for_inscription(
          inscription_id,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
      }
    }
  }

  // Dispatch a single accumulator-backed op for this inscription, mirroring tap-writer:
  // - If an accumulator exists and owner mismatches, delete a/<ins> and return.
  // - If it matches, call exactly one executed handler based on acc.op, then return.
  pub(crate) fn tap_execute_accumulator_for_inscription(
    &mut self,
    inscription_id: InscriptionId,
    new_satpoint: SatPoint,
    owner_address: &str,
    output_value_sat: u64,
  ) {
    let key = format!("a/{}", inscription_id);
    let acc_opt = self.tap_get::<TapAccumulatorEntry>(&key).ok().flatten();
    let Some(acc) = acc_opt else {
      return;
    };

    // Writer parity: even on owner mismatch, drop the accumulator for this tx.
    if acc.addr != owner_address {
      let _ = self.tap_del(&key);
      return;
    }

    // Dispatch exactly one executed handler by op and then unconditionally delete a/<ins>
    // to mirror tap-writer's lifecycle (even if the inner handler returned early).
    match acc.op.to_lowercase().as_str() {
      "token-send" => {
        let __st = std::time::Instant::now();
        self.index_token_send_executed(
          inscription_id,
          0,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_tsend_ex_ms += __st.elapsed().as_millis();
          self.prof_tsend_ex_ct += 1;
        }
      }
      "token-trade" => {
        let __st = std::time::Instant::now();
        self.index_token_trade_executed(
          inscription_id,
          0,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_ttrade_ex_ms += __st.elapsed().as_millis();
          self.prof_ttrade_ex_ct += 1;
        }
      }
      "token-auth" => {
        let __st = std::time::Instant::now();
        self.index_token_auth_executed(
          inscription_id,
          0,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_tauth_ex_ms += __st.elapsed().as_millis();
          self.prof_tauth_ex_ct += 1;
        }
      }
      "privilege-auth" => {
        let __st = std::time::Instant::now();
        self.index_privilege_auth_executed(
          inscription_id,
          0,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_pra_ex_ms += __st.elapsed().as_millis();
          self.prof_pra_ex_ct += 1;
        }
      }
      "block-transferables" => {
        let __st = std::time::Instant::now();
        self.index_block_transferables_executed(
          inscription_id,
          0,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_blk_ex_ms += __st.elapsed().as_millis();
          self.prof_blk_ex_ct += 1;
        }
      }
      "unblock-transferables" => {
        let __st = std::time::Instant::now();
        self.index_unblock_transferables_executed(
          inscription_id,
          0,
          new_satpoint,
          owner_address,
          output_value_sat,
        );
        if self.profile {
          self.prof_unblk_ex_ms += __st.elapsed().as_millis();
          self.prof_unblk_ex_ct += 1;
        }
      }
      _ => {
        // Unknown op: drop accumulator to prevent reprocessing drift.
        let _ = self.tap_del(&key);
      }
    }

    // Writer parity: ensure accumulator is removed after attempted execution.
    let _ = self.tap_del(&key);
  }

  // --- Token trade (Internal) ---
  pub(crate) fn validate_trade_main_ticker_len(&self, tick: &str) -> bool {
    let vis_len = Self::visible_length(tick);
    Self::valid_transfer_ticker_visible_len(
      self.feature_height(TapFeature::FullTicker),
      self.height,
      self.feature_height(TapFeature::Jubilee),
      tick,
      vis_len,
    )
  }

  pub(crate) fn validate_trade_accept_ticker_len(&self, tick: &str) -> bool {
    let t = Self::strip_prefix_for_len_check(tick);
    let vis_len = Self::visible_length(t);
    Self::valid_tap_ticker_visible_len(
      self.feature_height(TapFeature::FullTicker),
      self.height,
      vis_len,
    )
  }

  pub(crate) fn verify_sig_obj_against_msg_with_hash(
    &self,
    sig_obj: &serde_json::Value,
    recovery_hash_hex: &str,
    msg_hash: &[u8; 32],
  ) -> Option<(bool, String, String)> {
    // returns (is_valid, compact_sig_hex_lower, recovered_pubkey_hex)
    let sig = sig_obj.get("v")?;
    let r_val = sig_obj.get("r")?;
    let s_val = sig_obj.get("s")?;
    let v_i = Self::js_parse_int_i32(sig)?;
    let r_bytes = Self::js_bigint_value_to_32(r_val)?;
    let s_bytes = Self::js_bigint_value_to_32(s_val)?;
    let compact_sig_hex = Self::secp_compact_hex(&r_bytes, &s_bytes).to_lowercase();

    // Recover pubkey from provided recovery hash (32-byte hex)
    let rec_hash_bytes = hex::decode(recovery_hash_hex.trim_start_matches("0x")).ok()?;
    if rec_hash_bytes.len() != 32 {
      return None;
    }
    let mut rec_hash_arr = [0u8; 32];
    rec_hash_arr.copy_from_slice(&rec_hash_bytes);
    let secp = Secp256k1::new();
    let rec_id = if let Ok(id) = RecoveryId::from_i32(v_i) {
      id
    } else {
      RecoveryId::from_i32(v_i - 27).ok()?
    };
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&r_bytes);
    sig_bytes[32..].copy_from_slice(&s_bytes);
    let rec_sig = RecoverableSignature::from_compact(&sig_bytes, rec_id).ok()?;
    let rec_msg = Message::from_digest_slice(&rec_hash_arr).ok()?;
    let pubkey = secp.recover_ecdsa(&rec_msg, &rec_sig).ok()?;
    let pubkey_uncompressed = pubkey.serialize_uncompressed();
    let pubkey_hex = hex::encode(pubkey_uncompressed);
    let norm_sig = SecpSignature::from_compact(&sig_bytes).ok()?;
    let verify_msg = Message::from_digest_slice(msg_hash).ok()?;
    let ok = secp.verify_ecdsa(&verify_msg, &norm_sig, &pubkey).is_ok();
    Some((ok, compact_sig_hex, pubkey_hex))
  }
}
