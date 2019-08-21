use super::{Error, ErrorKind, RefCache, Storage};
use crate::blockcfg::{Block, Header, HeaderHash};
use crate::start_up::NodeStorage;
use chain_core::property::{ChainLength as _, HasHeader};

use futures::future::{self, Either};
use futures::prelude::*;

use std::convert::Infallible;
use std::time::Duration;

#[derive(Clone)]
enum Quarantined {
    Header(Header),
    Block(Block),
}

pub struct Quarantine {
    ref_cache: RefCache<Quarantined>,
    storage: Storage,
}

pub enum HeaderChainTriage {
    AlreadyPresent,
    Quarantined(HeaderHash),
}

impl Quarantine {
    pub fn new(storage: NodeStorage, ref_cache_ttl: Duration) -> Self {
        Quarantine {
            ref_cache: RefCache::new(ref_cache_ttl),
            storage: Storage::new(storage),
        }
    }

    pub fn get_header(
        &mut self,
        header_hash: HeaderHash,
    ) -> impl Future<Item = Option<Header>, Error = Error> {
        let storage = self.storage.clone();
        self.ref_cache
            .get(header_hash.clone())
            .map_err(|_: Infallible| unreachable!())
            .and_then(move |maybe_entry| match maybe_entry {
                None => Either::A(
                    storage
                        .get(header_hash)
                        .map(|maybe_block| maybe_block.map(|block| block.header()))
                        .map_err(|e| e.into()),
                ),
                Some(Quarantined::Header(header)) => Either::B(future::ok(Some(header.clone()))),
                Some(Quarantined::Block(block)) => Either::B(future::ok(Some(block.header()))),
            })
    }

    pub fn get_block(
        &mut self,
        header_hash: HeaderHash,
    ) -> impl Future<Item = Option<Block>, Error = Error> {
        let storage = self.storage.clone();
        self.ref_cache
            .get(header_hash.clone())
            .map_err(|_: Infallible| unreachable!())
            .and_then(move |maybe_entry| match maybe_entry {
                None => Either::A(storage.get(header_hash).map_err(|e| e.into())),
                Some(Quarantined::Header(_header)) => {
                    // FIXME: this should be an error as get_block should only
                    // be used to extract previously committed blocks.
                    Either::B(future::ok(None))
                }
                Some(Quarantined::Block(block)) => Either::B(future::ok(Some(block))),
            })
    }

    pub fn apply_header(
        &mut self,
        header: Header,
    ) -> impl Future<Item = HeaderChainTriage, Error = Error> {
        // TODO: before fetching the parent's header we can check
        //       the crypto of the header (i.e. check that they
        //       actually sign the header signing data against
        //       the public key).

        let block_id = header.hash();
        let parent_block_id = header.block_parent_hash().clone();
        let ref_cache = self.ref_cache.clone();

        let get_header_parent = self.get_header(parent_block_id);

        self.storage
            .block_exists(block_id)
            .map_err(|e| e.into())
            .and_then(move |exists| {
                if exists {
                    Either::A(future::ok(HeaderChainTriage::AlreadyPresent))
                } else {
                    Either::B(
                        get_header_parent
                            .and_then(move |maybe_parent| match maybe_parent {
                                None => Err(ErrorKind::MissingParentBlockFromStorage(header).into()),
                                Some(parent) => {
                                    if header.block_date() <= parent.block_date() {
                                        return Err(ErrorKind::BlockHeaderVerificationFailed(
                                            "block is not valid, date is set before parent's".into(),
                                        )
                                        .into());
                                    }
                                    if header.chain_length() != parent.chain_length().next() {
                                        return Err(ErrorKind::BlockHeaderVerificationFailed(
                                            "block is not valid, chain length is not monotonically increasing"
                                                .into(),
                                        )
                                        .into());
                                    }
                                    Ok(header)
                                }
                            })
                            .and_then(move |header| {
                                ref_cache
                                    .insert(header.hash(), Quarantined::Header(header))
                                    .map_err(|_: Infallible| unreachable!())
                            })
                            .map(move |()| HeaderChainTriage::Quarantined(block_id))
                    )
                }
            })
    }
}
