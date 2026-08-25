//! One trade a merchant offers, and the list of them.

use std::io::{Cursor, Error, Result, Write};
use std::ops::{Deref, DerefMut};

use steel_utils::codec::VarInt;
use steel_utils::serial::{ReadFrom, WriteTo};

use crate::item_stack::ItemStack;
use crate::trading::ItemCost;

/// A single trade: what the merchant wants, what it gives, and how its price moves.
///
/// Vanilla parity: `net.minecraft.world.item.trading.MerchantOffer`.
#[derive(Debug, Clone, PartialEq)]
pub struct MerchantOffer {
    base_cost_a: ItemCost,
    cost_b: Option<ItemCost>,
    result: ItemStack,
    uses: i32,
    max_uses: i32,
    reward_exp: bool,
    special_price_diff: i32,
    demand: i32,
    price_multiplier: f32,
    xp: i32,
}

impl MerchantOffer {
    /// The `rewardExp` every constructed offer starts with.
    ///
    /// Vanilla parity: the `true` the public constructors pass; only the codec
    /// can produce an offer that awards no experience.
    const DEFAULT_REWARD_EXP: bool = true;

    /// Builds an unused offer.
    ///
    /// Vanilla parity: `MerchantOffer(ItemCost, Optional<ItemCost>, ItemStack, int maxUses, int xp, float priceMultiplier)`.
    #[must_use]
    pub const fn new(
        base_cost_a: ItemCost,
        cost_b: Option<ItemCost>,
        result: ItemStack,
        max_uses: i32,
        xp: i32,
        price_multiplier: f32,
    ) -> Self {
        Self::with_uses(
            base_cost_a,
            cost_b,
            result,
            0,
            max_uses,
            xp,
            price_multiplier,
            0,
        )
    }

    /// Builds an offer that has already been traded, or that starts under demand.
    ///
    /// Vanilla parity: the widest public constructor, the one the stream codec feeds.
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the vanilla constructor the stream codec feeds"
    )]
    #[must_use]
    pub const fn with_uses(
        base_cost_a: ItemCost,
        cost_b: Option<ItemCost>,
        result: ItemStack,
        uses: i32,
        max_uses: i32,
        xp: i32,
        price_multiplier: f32,
        demand: i32,
    ) -> Self {
        Self {
            base_cost_a,
            cost_b,
            result,
            uses,
            max_uses,
            reward_exp: Self::DEFAULT_REWARD_EXP,
            special_price_diff: 0,
            demand,
            price_multiplier,
            xp,
        }
    }

    /// The first price before demand and reputation move it.
    ///
    /// Vanilla parity: `getBaseCostA`.
    #[must_use]
    pub const fn base_cost_a(&self) -> &ItemStack {
        self.base_cost_a.cost_stack()
    }

    /// The first price as it stands right now.
    ///
    /// Vanilla parity: `getCostA`.
    #[must_use]
    pub fn cost_a(&self) -> ItemStack {
        self.base_cost_a
            .cost_stack()
            .copy_with_count(self.modified_cost_count(&self.base_cost_a))
    }

    /// The second price, which demand and reputation never touch.
    ///
    /// Vanilla parity: `getCostB`.
    #[must_use]
    pub fn cost_b(&self) -> ItemStack {
        self.cost_b
            .as_ref()
            .map_or_else(ItemStack::empty, |cost| cost.cost_stack().clone())
    }

    /// Vanilla parity: `getItemCostA`.
    #[must_use]
    pub const fn item_cost_a(&self) -> &ItemCost {
        &self.base_cost_a
    }

    /// Vanilla parity: `getItemCostB`.
    #[must_use]
    pub const fn item_cost_b(&self) -> Option<&ItemCost> {
        self.cost_b.as_ref()
    }

    /// What the merchant hands over.
    ///
    /// Vanilla parity: `getResult`.
    #[must_use]
    pub const fn result(&self) -> &ItemStack {
        &self.result
    }

    /// A fresh copy of the result, for actually giving to the player.
    ///
    /// Vanilla parity: `assemble`.
    #[must_use]
    pub fn assemble(&self) -> ItemStack {
        self.result.clone()
    }

    /// The count a price actually asks for once demand and reputation apply.
    ///
    /// Vanilla parity: the private `getModifiedCostCount`. Demand only ever
    /// raises the price -- the `max(0, ..)` is what stops a negative demand from
    /// discounting a trade -- while `specialPriceDiff` carries reputation and can
    /// go either way, down to a floor of one item.
    fn modified_cost_count(&self, cost: &ItemCost) -> i32 {
        let base_price = cost.count();
        #[expect(
            clippy::cast_precision_loss,
            reason = "vanilla computes this product in float"
        )]
        let demand_diff = ((base_price * self.demand) as f32 * self.price_multiplier).floor();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla's Mth.floor(float) truncates to int the same way"
        )]
        let demand_diff = (demand_diff as i32).max(0);
        (base_price + demand_diff + self.special_price_diff)
            .clamp(1, cost.cost_stack().max_stack_size())
    }

    /// Folds this trade's use count into its standing demand.
    ///
    /// Vanilla parity: `updateDemand`, run once a day per villager.
    pub const fn update_demand(&mut self) {
        self.demand = self.demand + self.uses - (self.max_uses - self.uses);
    }

    /// Vanilla parity: `getUses`.
    #[must_use]
    pub const fn uses(&self) -> i32 {
        self.uses
    }

    /// Vanilla parity: `resetUses`.
    pub const fn reset_uses(&mut self) {
        self.uses = 0;
    }

    /// Vanilla parity: `getMaxUses`.
    #[must_use]
    pub const fn max_uses(&self) -> i32 {
        self.max_uses
    }

    /// Vanilla parity: `increaseUses`.
    pub const fn increase_uses(&mut self) {
        self.uses += 1;
    }

    /// Vanilla parity: `getDemand`.
    #[must_use]
    pub const fn demand(&self) -> i32 {
        self.demand
    }

    /// Vanilla parity: `addToSpecialPriceDiff`.
    pub const fn add_to_special_price_diff(&mut self, add: i32) {
        self.special_price_diff += add;
    }

    /// Vanilla parity: `resetSpecialPriceDiff`.
    pub const fn reset_special_price_diff(&mut self) {
        self.special_price_diff = 0;
    }

    /// Vanilla parity: `getSpecialPriceDiff`.
    #[must_use]
    pub const fn special_price_diff(&self) -> i32 {
        self.special_price_diff
    }

    /// Vanilla parity: `setSpecialPriceDiff`.
    pub const fn set_special_price_diff(&mut self, value: i32) {
        self.special_price_diff = value;
    }

    /// Vanilla parity: `getPriceMultiplier`.
    #[must_use]
    pub const fn price_multiplier(&self) -> f32 {
        self.price_multiplier
    }

    /// Vanilla parity: `getXp`.
    #[must_use]
    pub const fn xp(&self) -> i32 {
        self.xp
    }

    /// Vanilla parity: `isOutOfStock`.
    #[must_use]
    pub const fn is_out_of_stock(&self) -> bool {
        self.uses >= self.max_uses
    }

    /// Vanilla parity: `setToOutOfStock`.
    pub const fn set_to_out_of_stock(&mut self) {
        self.uses = self.max_uses;
    }

    /// Vanilla parity: `needsRestock`.
    #[must_use]
    pub const fn needs_restock(&self) -> bool {
        self.uses > 0
    }

    /// Vanilla parity: `shouldRewardExp`.
    #[must_use]
    pub const fn should_reward_exp(&self) -> bool {
        self.reward_exp
    }

    /// Returns `true` if these two stacks pay for this trade.
    ///
    /// Vanilla parity: `satisfiedBy`. The first price is compared against its
    /// *modified* count, the second against its plain one -- demand moves only
    /// the primary cost.
    #[must_use]
    pub fn satisfied_by(&self, buy_a: &ItemStack, buy_b: &ItemStack) -> bool {
        if !self.base_cost_a.test(buy_a)
            || buy_a.count() < self.modified_cost_count(&self.base_cost_a)
        {
            return false;
        }

        match &self.cost_b {
            None => buy_b.is_empty(),
            Some(cost_b) => cost_b.test(buy_b) && buy_b.count() >= cost_b.count(),
        }
    }

    /// Takes the price out of the two stacks, if they cover it.
    ///
    /// Vanilla parity: `take`.
    pub fn take(&self, buy_a: &mut ItemStack, buy_b: &mut ItemStack) -> bool {
        if !self.satisfied_by(buy_a, buy_b) {
            return false;
        }

        buy_a.shrink(self.cost_a().count());
        let cost_b = self.cost_b();
        if !cost_b.is_empty() {
            buy_b.shrink(cost_b.count());
        }

        true
    }
}

impl WriteTo for MerchantOffer {
    /// Vanilla parity: `MerchantOffer.writeToStream`. Note the plain `writeInt`s:
    /// uses, maxUses, xp, specialPriceDiff and demand all go out as fixed-width
    /// big-endian, not as varints, and `specialPriceDiff` is signed.
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.base_cost_a.write(writer)?;
        self.result.write(writer)?;
        self.cost_b.write(writer)?;
        self.is_out_of_stock().write(writer)?;
        self.uses.write(writer)?;
        self.max_uses.write(writer)?;
        self.xp.write(writer)?;
        self.special_price_diff.write(writer)?;
        self.price_multiplier.write(writer)?;
        self.demand.write(writer)
    }
}

impl ReadFrom for MerchantOffer {
    /// Vanilla parity: `MerchantOffer.createFromStream`.
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let base_cost_a = ItemCost::read(data)?;
        let result = ItemStack::read(data)?;
        let cost_b = Option::<ItemCost>::read(data)?;
        let is_exhausted = bool::read(data)?;
        let uses = i32::read(data)?;
        let max_uses = i32::read(data)?;
        let xp = i32::read(data)?;
        let special_price_diff = i32::read(data)?;
        let price_multiplier = f32::read(data)?;
        let demand = i32::read(data)?;

        let mut offer = Self::with_uses(
            base_cost_a,
            cost_b,
            result,
            uses,
            max_uses,
            xp,
            price_multiplier,
            demand,
        );
        if is_exhausted {
            offer.set_to_out_of_stock();
        }
        offer.set_special_price_diff(special_price_diff);
        Ok(offer)
    }
}

/// Everything a merchant is currently willing to trade.
///
/// Vanilla parity: `net.minecraft.world.item.trading.MerchantOffers`, which is
/// an `ArrayList<MerchantOffer>`; the `Deref` here is what stands in for that
/// inheritance.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MerchantOffers(Vec<MerchantOffer>);

impl MerchantOffers {
    /// An empty offer list.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Finds the index of the trade these two stacks pay for.
    ///
    /// Vanilla parity: `getRecipeFor`, which returns the offer itself; the
    /// index is what a caller needs to reach back into the merchant's own list
    /// and change the trade it just sold.
    ///
    /// `selection_hint` is the trade the player clicked: when it is in range it
    /// is the *only* one considered, so two trades wanting the same item stay
    /// distinguishable.
    #[must_use]
    pub fn recipe_index_for(
        &self,
        buy_a: &ItemStack,
        buy_b: &ItemStack,
        selection_hint: i32,
    ) -> Option<usize> {
        // Vanilla's bound is `> 0`, not `>= 0`: trade zero never gets the
        // shortcut and always falls through to the scan, which reaches it first
        // anyway.
        if selection_hint > 0 && (selection_hint as usize) < self.0.len() {
            let index = selection_hint as usize;
            return self.0[index].satisfied_by(buy_a, buy_b).then_some(index);
        }

        self.0
            .iter()
            .position(|offer| offer.satisfied_by(buy_a, buy_b))
    }

    /// The trade these two stacks pay for.
    ///
    /// Vanilla parity: `getRecipeFor`.
    #[must_use]
    pub fn recipe_for(
        &self,
        buy_a: &ItemStack,
        buy_b: &ItemStack,
        selection_hint: i32,
    ) -> Option<&MerchantOffer> {
        self.0
            .get(self.recipe_index_for(buy_a, buy_b, selection_hint)?)
    }
}

impl From<Vec<MerchantOffer>> for MerchantOffers {
    fn from(offers: Vec<MerchantOffer>) -> Self {
        Self(offers)
    }
}

impl Deref for MerchantOffers {
    type Target = Vec<MerchantOffer>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MerchantOffers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromIterator<MerchantOffer> for MerchantOffers {
    fn from_iter<T: IntoIterator<Item = MerchantOffer>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl WriteTo for MerchantOffers {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.0.write(writer)
    }
}

impl ReadFrom for MerchantOffers {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let count = VarInt::read(data)?.0;
        let count = usize::try_from(count)
            .map_err(|_| Error::other(format!("Negative merchant offer count: {count}")))?;
        // The list is bounded only by what the peer claims, so the capacity is
        // not trusted; the reads themselves run out of buffer soon enough.
        let mut offers = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            offers.push(MerchantOffer::read(data)?);
        }
        Ok(Self(offers))
    }
}
