use std::str::FromStr;

pub mod utils;

use {
    crate::utils::{
        assert_derivation, assert_initialized, assert_metadata_valid, assert_owned_by,
        assert_part_of_namespace, create_or_allocate_account_raw, get_mask_and_index_for_seq,
        inverse_indexed_bool_for_namespace, spl_token_burn, spl_token_mint_to, spl_token_transfer,
        TokenBurnParams, TokenTransferParams,
    },
    anchor_lang::{
        prelude::*,
        solana_program::{
            program::{invoke, invoke_signed},
            program_option::COption,
            program_pack::Pack,
            system_instruction, system_program,
        },
        AnchorDeserialize, AnchorSerialize,
    },
    anchor_spl::token::{Mint, Token, TokenAccount},
    metaplex_token_metadata::instruction::{
        create_master_edition, create_metadata_accounts,
        mint_new_edition_from_master_edition_via_token, update_metadata_accounts,
    },
    spl_token::instruction::{initialize_account2, mint_to},
};
anchor_lang::declare_id!("p1exdMJcjVao65QdewkaZRUnU6VPSXhus9n2GzWfh98");
pub const PLAYER_ID: Pubkey = Pubkey::from_str("p1exdMJcjVao65QdewkaZRUnU6VPSXhus9n2GzWfh98");
pub const MATCH_ID: Pubkey = Pubkey::from_str("p1exdMJcjVao65QdewkaZRUnU6VPSXhus9n2GzWfh98");
pub const ITEM_ID: Pubkey = Pubkey::from_str("p1exdMJcjVao65QdewkaZRUnU6VPSXhus9n2GzWfh98");
pub const MAX_NAMESPACES: usize = 5;

const PREFIX: &str = "namespace";
const MAX_WHITELIST: usize = 5;
const MAX_CACHED_ITEMS: usize = 100;
#[program]
pub mod namespace {
    use super::*;
    pub fn initialize_namespace<'info>(
        ctx: Context<'_, '_, '_, 'info, InitializeNamespace<'info>>,
        bump: u8,
        uuid: String,
        pretty_name: String,
        namespace_permissiveness: Permissiveness,
        item_permissiveness: Permissiveness,
        player_permissiveness: Permissiveness,
        match_permissiveness: Permissiveness,
        mission_permissiveness: Permissiveness,
        cache_permissiveness: Permissiveness,
        whitelisted_staking_mints: Vec<Pubkey>,
    ) -> ProgramResult {
        if uuid.len() > 6 {
            return Err(ErrorCode::UUIDTooLong.into());
        }

        if pretty_name.len() > 32 {
            return Err(ErrorCode::PrettyNameTooLong.into());
        }

        if whitelisted_staking_mints.len() > MAX_WHITELIST {
            return Err(ErrorCode::WhitelistStakeListTooLong.into());
        }

        for n in 0..whitelisted_staking_mints.len() {
            let mint_account = &ctx.remaining_accounts[n];
            // Assert they are all real mints.
            let _mint: spl_token::state::Mint = assert_initialized(&mint_account)?;
        }

        let namespace = &mut ctx.accounts.namespace;
        let metadata = &ctx.accounts.metadata;
        let mint = &ctx.accounts.mint;
        let master_edition = &ctx.accounts.master_edition;

        assert_metadata_valid(metadata, Some(master_edition), &mint.key())?;

        namespace.bump = bump;
        namespace.uuid = uuid;
        namespace.whitelisted_staking_mints = whitelisted_staking_mints;
        namespace.pretty_name = namespace.pretty_name.to_string();
        namespace.namespace_permissiveness = namespace_permissiveness;
        namespace.item_permissiveness = item_permissiveness;
        namespace.player_permissiveness = player_permissiveness;
        namespace.match_permissiveness = match_permissiveness;
        namespace.mission_permissiveness = mission_permissiveness;
        namespace.cache_permissiveness = cache_permissiveness;
        namespace.metadata = metadata.key();
        namespace.master_edition = master_edition.key();
        namespace.highest_page = 0;
        namespace.artifacts_cached = 0;
        namespace.artifacts_added = 0;

        Ok(())
    }

    pub fn update_namespace<'info>(
        ctx: Context<'_, '_, '_, 'info, UpdateNamespace<'info>>,
        pretty_name: Option<String>,
        namespace_permissiveness: Option<Permissiveness>,
        item_permissiveness: Option<Permissiveness>,
        player_permissiveness: Option<Permissiveness>,
        match_permissiveness: Option<Permissiveness>,
        mission_permissiveness: Option<Permissiveness>,
        cache_permissiveness: Option<Permissiveness>,
        whitelisted_staking_mints: Option<Vec<Pubkey>>,
    ) -> ProgramResult {
        let namespace = &mut ctx.accounts.namespace;

        if let Some(ws_mints) = whitelisted_staking_mints {
            if ws_mints.len() > MAX_WHITELIST {
                return Err(ErrorCode::WhitelistStakeListTooLong.into());
            }
            for n in 0..ws_mints.len() {
                let mint_account = &ctx.remaining_accounts[n];
                // Assert they are all real mints.
                let _mint: spl_token::state::Mint = assert_initialized(&mint_account)?;
            }
            namespace.whitelisted_staking_mints = ws_mints;
        }
        if let Some(pn) = pretty_name {
            if pn.len() > 32 {
                return Err(ErrorCode::PrettyNameTooLong.into());
            }
            namespace.pretty_name = pn.to_string();
        }

        if let Some(permissiveness) = namespace_permissiveness {
            namespace.namespace_permissiveness = permissiveness;
        }

        if let Some(permissiveness) = item_permissiveness {
            namespace.item_permissiveness = permissiveness;
        }

        if let Some(permissiveness) = player_permissiveness {
            namespace.player_permissiveness = permissiveness;
        }

        if let Some(permissiveness) = match_permissiveness {
            namespace.match_permissiveness = permissiveness;
        }

        if let Some(permissiveness) = mission_permissiveness {
            namespace.mission_permissiveness = permissiveness;
        }

        if let Some(permissiveness) = cache_permissiveness {
            namespace.cache_permissiveness = permissiveness;
        }
        Ok(())
    }

    pub fn cache_artifact<'info>(
        ctx: Context<'_, '_, '_, 'info, CacheArtifact<'info>>,
        index_bump: u8,
        prior_index_bump: u8,
        page: u64,
    ) -> ProgramResult {
        let namespace = &mut ctx.accounts.namespace;
        let index = &mut ctx.accounts.index;
        let prior_index = &ctx.accounts.prior_index;
        let artifact = &mut ctx.accounts.artifact;
        let index_info = index.to_account_info();
        let prior_index_info = prior_index.to_account_info();
        let artifact_info = artifact.to_account_info();

        assert_part_of_namespace(artifact, namespace)?;

        if artifact.owner != &raindrops_player::id()
            && artifact.owner != &raindrops_matches::id()
            && artifact.owner != &raindrops_item::id()
            && artifact.owner != &id()
        {
            return Err(ErrorCode::CanOnlyCacheValidRaindropsObjects.into());
        }

        if index_info.data_is_empty() {
            if prior_index_info.data_is_empty() {
                return Err(ErrorCode::PreviousIndexNeedsToExistBeforeCreatingThisOne.into());
            } else if prior_index.caches.len() < MAX_CACHED_ITEMS {
                return Err(ErrorCode::PreviousIndexNotFull.into());
            }
            let namespace_key = namespace.key();
            let page_str = page.to_string();
            let signer_seeds = [
                PREFIX.as_bytes(),
                namespace_key.as_ref(),
                page_str.as_bytes(),
                &[index_bump],
            ];
            create_or_allocate_account_raw(
                *ctx.program_id,
                &index_info,
                &ctx.accounts.rent.to_account_info(),
                &ctx.accounts.system_program,
                &ctx.accounts.payer,
                INDEX_SIZE,
                &signer_seeds,
            )?;
        } else if index.caches.len() >= MAX_CACHED_ITEMS {
            return Err(ErrorCode::IndexFull.into());
        }

        namespace.artifacts_cached = namespace
            .artifacts_cached
            .checked_add(1)
            .ok_or(ErrorCode::NumericalOverflowError)?;
        if page > namespace.highest_page {
            namespace.highest_page = page
        }
        index.page = page;
        index.namespace = namespace.key();
        index.caches.push(artifact.key());

        // The 9th byte is customarily the cached byte, and we check that you are owned
        // by one of our whitelisted programs

        let old_val = inverse_indexed_bool_for_namespace(artifact, namespace.key())?;
        if old_val == 1 {
            return Err(ErrorCode::AlreadyCached.into());
        }
        Ok(())
    }

    pub fn uncache_artifact<'info>(
        ctx: Context<'_, '_, '_, 'info, UncacheArtifact<'info>>,
        page: u64,
    ) -> ProgramResult {
        let namespace = &mut ctx.accounts.namespace;
        let index = &mut ctx.accounts.index;
        let artifact = &mut ctx.accounts.artifact;
        let receiver = &mut ctx.accounts.receiver;
        let artifact_info = artifact.to_account_info();

        let mut found = false;
        let mut new_arr = vec![];
        for obj in &index.caches {
            if obj != &artifact.key() {
                new_arr.push(obj);
            } else {
                found = true;
            }
        }

        if found {
            inverse_indexed_bool_for_namespace(artifact, namespace.key())?;
            namespace.artifacts_cached = namespace
                .artifacts_cached
                .checked_sub(1)
                .ok_or(ErrorCode::NumericalOverflowError)?;
        }

        if page == namespace.highest_page && index.caches.len() == 0 {
            namespace.highest_page = page
                .checked_sub(1)
                .ok_or(ErrorCode::NumericalOverflowError)?;
            let curr_lamp = index.to_account_info().lamports();
            **index.to_account_info().lamports.borrow_mut() = 0;

            **receiver.lamports.borrow_mut() = receiver
                .lamports()
                .checked_add(curr_lamp)
                .ok_or(ErrorCode::NumericalOverflowError)?;
        }

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum Permissiveness {
    All,
    Whitelist,
    Blacklist,
    Namespace,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum ArtifactType {
    Player,
    Item,
    Mission,
    Namespace,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum Filter {
    None {
        padding: [u8; FILTER_SIZE],
    },
    Category {
        namespace: Pubkey,
        category: Option<String>,
        padding: [u8; FILTER_SIZE - 26 - 32],
    },
    Key {
        key: Pubkey,
        mint: Pubkey,
        metadata: Pubkey,
        edition: Option<Pubkey>,
        padding: [u8; 32],
    },
}

pub const FILTER_SIZE: usize = 32 + // key
32 + // mint
32 + // metadata
33 + // edition
32; //padding

#[account]
pub struct ArtifactFilter {
    // namespace will always be set, just following the caching convention in case
    // user decides to cache these, too
    indexed: bool,
    namespace: Option<Pubkey>,
    is_blacklist: bool,
    filter: Filter,
    token_type: ArtifactType,
    filter_index: u64,
    bump: u8,
}

/// seed ['namespace', namespace program, mint]
#[account]
pub struct Namespace {
    indexed: bool,
    namespace: Option<Pubkey>,
    mint: Pubkey,
    metadata: Pubkey,
    master_edition: Pubkey,
    uuid: String,
    pretty_name: String,
    artifacts_added: u64,
    highest_page: u64,
    artifacts_cached: u64,
    namespace_permissiveness: Permissiveness,
    item_permissiveness: Permissiveness,
    player_permissiveness: Permissiveness,
    match_permissiveness: Permissiveness,
    mission_permissiveness: Permissiveness,
    cache_permissiveness: Permissiveness,
    bump: u8,
    whitelisted_staking_mints: Vec<Pubkey>,
}

/// seed ['namespace', namespace program, mint, page number]
#[account]
pub struct NamespaceIndex {
    pub namespace: Pubkey,
    pub bump: u8,
    pub page: u64,
    pub caches: Vec<Pubkey>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct NamespaceAndIndex {
    namespace: Pubkey,
    indexed: bool,
}
#[account]
pub struct ArtifactNamespaceSetting {
    namespaces: Vec<NamespaceAndIndex>,
}

pub const NAMESPACE_SIZE: usize = 8 + // key
1 + // indexed
33 + // namespace
32 + // mint
32 + // metadata
32 + // edition
6 + // uuid
32 + // pretty name
8 + // artifacts_added
8 + // highest page
8 + // artifacts cached
6 + // permissivenesses
1 + // bump
5 + // whitelist staking mints
200; // padding

pub const INDEX_SIZE: usize = 8 + // key
32 + // namespace
1 + // bump
8 + // page
4 + // amount in vec
32*MAX_CACHED_ITEMS + // array space
100; //padding

pub const ARTIFACT_FILTER_SIZE: usize = 8 + // key
1 + // indexed
33 + // namespace
1 + // is blacklist
FILTER_SIZE + //
1 + // artifact type
32 + // artifact
8 + // filter index
1 + // bump
100; //padding

#[derive(Accounts)]
#[instruction(bump: u8)]
pub struct InitializeNamespace<'info> {
    #[account(init, seeds=[PREFIX.as_bytes(), mint.key().as_ref()], payer=payer, bump=bump, space=NAMESPACE_SIZE)]
    namespace: Account<'info, Namespace>,
    mint: Account<'info, Mint>,
    metadata: UncheckedAccount<'info>,
    master_edition: UncheckedAccount<'info>,
    payer: Signer<'info>,
    token_program: Program<'info, Token>,
    system_program: Program<'info, System>,
    rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct UpdateNamespace<'info> {
    #[account(mut, seeds=[PREFIX.as_bytes(), namespace_token.mint.as_ref()], bump=namespace.bump)]
    namespace: Account<'info, Namespace>,
    #[account(constraint=namespace_token.owner == token_holder.key())]
    namespace_token: Account<'info, TokenAccount>,
    token_holder: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(index_bump: u8, prior_index_bump: u8, page: u64)]
pub struct CacheArtifact<'info> {
    #[account(mut, seeds=[PREFIX.as_bytes(), namespace_token.mint.as_ref()], bump=namespace.bump)]
    namespace: Account<'info, Namespace>,
    #[account(constraint=namespace_token.owner == token_holder.key())]
    namespace_token: Account<'info, TokenAccount>,
    #[account(mut, seeds=[PREFIX.as_bytes(), namespace.key().as_ref(), page.to_string().as_bytes()], bump=index_bump)]
    index: Account<'info, NamespaceIndex>,
    #[account(mut, seeds=[PREFIX.as_bytes(), namespace.key().as_ref(), page.checked_sub(1).ok_or(0)?.to_string().as_bytes()], bump=prior_index_bump)]
    prior_index: Account<'info, NamespaceIndex>,
    #[account(mut)]
    artifact: UncheckedAccount<'info>,
    token_holder: UncheckedAccount<'info>,
    payer: Signer<'info>,
    system_program: Program<'info, System>,
    rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(index_bump: u8, prior_index_bump: u8, page: u64)]
pub struct JoinArtifact<'info> {
    #[account(mut, seeds=[PREFIX.as_bytes(), namespace_token.mint.as_ref()], bump=namespace.bump)]
    namespace: Account<'info, Namespace>,
    #[account(constraint=namespace_token.owner == token_holder.key())]
    namespace_token: Account<'info, TokenAccount>,
    #[account(mut)]
    artifact: UncheckedAccount<'info>,

    token_holder: UncheckedAccount<'info>,
    payer: Signer<'info>,
    system_program: Program<'info, System>,
    rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction( page: u64)]
pub struct UncacheArtifact<'info> {
    #[account(mut, seeds=[PREFIX.as_bytes(), namespace_token.mint.as_ref()], bump=namespace.bump)]
    namespace: Account<'info, Namespace>,
    #[account(constraint=namespace_token.owner == token_holder.key())]
    namespace_token: Account<'info, TokenAccount>,
    #[account(mut, seeds=[PREFIX.as_bytes(), namespace.key().as_ref(), page.to_string().as_bytes()], bump=index.bump)]
    index: Account<'info, NamespaceIndex>,
    #[account(mut)]
    artifact: UncheckedAccount<'info>,
    #[account(mut, seeds=[PREFIX.as_bytes(), namespace.key().as_ref(), artifact_filter.filter_index.to_string().as_bytes()], bump=filter_index.bump)]
    artifact_filter: Account<'info, ArtifactFilter>,
    token_holder: UncheckedAccount<'info>,
    // Received of funds
    receiver: UncheckedAccount<'info>,
    system_program: Program<'info, System>,
    rent: Sysvar<'info, Rent>,
}

#[error]
pub enum ErrorCode {
    #[msg("Account does not have correct owner!")]
    IncorrectOwner,
    #[msg("Account is not initialized!")]
    Uninitialized,
    #[msg("Mint Mismatch!")]
    MintMismatch,
    #[msg("Token transfer failed")]
    TokenTransferFailed,
    #[msg("Numerical overflow error")]
    NumericalOverflowError,
    #[msg("Token mint to failed")]
    TokenMintToFailed,
    #[msg("TokenBurnFailed")]
    TokenBurnFailed,
    #[msg("Derived key is invalid")]
    DerivedKeyInvalid,
    #[msg("UUID too long, 6 char max")]
    UUIDTooLong,
    #[msg("Pretty name too long, 32 char max")]
    PrettyNameTooLong,
    #[msg("Whitelist stake list too long, 5 max")]
    WhitelistStakeListTooLong,
    #[msg("Metadata doesnt exist")]
    MetadataDoesntExist,
    #[msg("Edition doesnt exist")]
    EditionDoesntExist,
    #[msg("Previous index needs to exist before creating this one")]
    PreviousIndexNeedsToExistBeforeCreatingThisOne,
    #[msg("The previous index is not full yet, so you cannot make a new one")]
    PreviousIndexNotFull,
    #[msg("Index is full")]
    IndexFull,
    #[msg("Can only cache valid raindrops artifacts (players, items, matches)")]
    CanOnlyCacheValidRaindropsObjects,
    #[msg("This artifact has already been cached")]
    AlreadyCached,
    #[msg("This artifact is not cached!")]
    NotCached,
    #[msg("This artifact is cached but not on this page")]
    NotCachedHere,
    #[msg("Artifact lacks namespace!")]
    ArtifactLacksNamespace,
    #[msg("Artifact not part of namespace!")]
    ArtifactNotPartOfNamespace,
}