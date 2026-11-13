//! A handle borrowed from the owner that gave it out, and the one picture an
//! owner can have out at a time.
//!
//! Everything the card hands a stream — a capture's surface, the picture a
//! scaler made, a share's own texture — is a COM object a safe caller must not
//! be able to use after the thing that made it has gone or replaced it. A raw
//! pointer cannot say that, and a comment asking the caller to keep the owner
//! alive is not a promise the compiler keeps. [`Lease`] is: it is the borrow of
//! that owner, carried beside the handle, so a use of the handle after the owner
//! has gone is a compile error instead of a read of freed memory.
//!
//! The rule lives here, apart from the Direct3D types it is applied to in
//! [`crate::d3d11`], so that a machine without a card and without Windows can
//! hold the compiler to it: `cargo test -p yapfast-core lease`.
//!
//! A lease is a borrow and nothing else — no reference count, no allocation and
//! nothing to free when it goes — which is what makes it usable on a path that
//! runs thirty times a second. [`Loans`] is the one piece that has to count: a
//! scaler keeps one picture and frees it when it is asked for another size, so
//! it counts the leases of that picture and refuses a size that would free the
//! texture under one.

use std::{cell::Cell, marker::PhantomData};

/// Proof that an owner is borrowed, carried by the handle it gave out: `'a` is
/// that borrow, and what the handle names is good only for as long as it lasts.
///
/// It is `Copy`, because it is a number the owner has already answered with and
/// copying it takes nothing of the owner's; a copy is the same borrow, and lives
/// no longer than the one it was copied from. Nothing is added to it and nothing
/// removed: a texture is its handle, its subresource and this.
///
/// Escaping the owner is the failure it exists for — a lease made from an owner
/// inside a block and used after it is `E0597`, "borrowed value does not live
/// long enough":
///
/// ```compile_fail
/// use yapfast_core::lease::Lease;
///
/// let lease = {
///     // A capture, or a scaler, or a share: whatever made the texture.
///     let owner = String::from("the capture a picture came from");
///     Lease::of(&owner)
/// };
/// // `owner` went at the end of the block, and the lease did not: the borrow it
/// // holds can't be satisfied, so the compiler says so here.
/// drop(lease);
/// ```
///
/// A lease of an owner that is still there is the whole of the safe use: the
/// handle it travels with is read only while this borrow is alive.
#[derive(Clone, Copy, Debug)]
pub struct Lease<'a> {
    owner: PhantomData<&'a ()>,
}

impl<'a> Lease<'a> {
    /// The lease of `owner`, which only an owner of the thing can make: the
    /// borrow is the `&'a` this is given, so a lease never outlives what it was
    /// made from.
    pub fn of<O: ?Sized>(_owner: &'a O) -> Self {
        Self { owner: PhantomData }
    }
}

/// The one picture an owner can have out at a time, and how many leases of it
/// are out now: a scaler's picture, which is made at one size and freed when it
/// is asked for another.
///
/// A picture of the size that is out may be made again — that writes over the
/// picture rather than freeing it, so the leases that name it keep naming a
/// texture, which is what a stream does frame after frame. A picture of another
/// size frees the one that is out, which a borrow cannot stop: it is the owner
/// that would free it, while the borrow is still alive. So the count refuses
/// that size while a lease of it lives.
///
/// The count is one [`Cell`] read and write per picture made and one per lease
/// dropped, and no allocation: the steady state of a stream is the same size,
/// which is a compare and nothing else.
#[derive(Debug, Default)]
pub struct Loans {
    /// The size of the picture that was handed out last.
    size: Cell<(u32, u32)>,
    /// How many leases of it are out: nothing is out while this is zero.
    out: Cell<u32>,
}

impl Loans {
    /// A lender with nothing out.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a picture of `size` can be made now: `Ok` while nothing is out,
    /// or while what is out is that same size, and `Err` with the size that is
    /// out, which is one that would be freed with a lease on it.
    pub fn free_at(&self, size: (u32, u32)) -> Result<(), (u32, u32)> {
        let out = self.size.get();
        if self.out.get() == 0 || out == size {
            Ok(())
        } else {
            Err(out)
        }
    }

    /// Hands a picture of `size` out: the count of leases rises until the
    /// [`Loan`] answered here goes, and until every other one of that picture
    /// goes with it.
    pub fn lend(&self, size: (u32, u32)) -> Loan<'_> {
        self.size.set(size);
        self.out.set(self.out.get() + 1);
        Loan { loans: self }
    }

    /// How many leases of the picture are out now.
    pub fn out(&self) -> u32 {
        self.out.get()
    }
}

/// One lease of a [`Loans`]'s picture: what [`Loans::lend`] answers, and what
/// lets the count fall when it goes.
#[derive(Debug)]
pub struct Loan<'a> {
    loans: &'a Loans,
}

impl Drop for Loan<'_> {
    fn drop(&mut self) {
        self.loans.out.set(self.loans.out.get() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lease is a borrow and a copy of it is the same borrow: neither outlives
    /// the owner, and an owner handed every one of them back is its own again.
    /// The compiler holds the failing side of this rule — the `compile_fail`
    /// example above is one of its errors — so what is checked here is that a
    /// lease does not take the owner away, which is what makes it free to hand
    /// out on every frame.
    #[test]
    fn a_lease_is_a_borrow_and_a_copy_of_it_is_the_same_borrow() {
        let mut owner = String::from("a capture");
        {
            let lease = Lease::of(&owner);
            let same = lease;
            assert_eq!(format!("{same:?}"), format!("{lease:?}"));
        }
        // Every copy gone is the borrow over, and the owner is the caller's to
        // change again: a lease takes nothing and gives nothing back.
        owner.push_str(" that has stopped");
        assert_eq!(owner, "a capture that has stopped");
    }

    /// A picture of the size that is out may be made again — it is written over
    /// rather than freed — and a picture of another size waits until every lease
    /// of the one that is out has gone, since making it frees the texture they
    /// name.
    #[test]
    fn a_picture_of_another_size_waits_for_the_leases_of_the_one_that_is_out() {
        let loans = Loans::new();
        let (tile, screen) = ((480, 270), (1920, 1080));
        // Nothing is out: either size may be made.
        assert_eq!(loans.free_at(tile), Ok(()));
        assert_eq!(loans.free_at(screen), Ok(()));
        let first = loans.lend(tile);
        assert_eq!(loans.out(), 1);
        // Another picture of the same size is the frame after this one, and the
        // lease still names a texture.
        assert_eq!(loans.free_at(tile), Ok(()));
        assert_eq!(loans.free_at(screen), Err(tile));
        // Two leases of one picture are one picture out: the first going is not
        // the picture being free, and the second going is.
        let second = loans.lend(tile);
        assert_eq!(loans.out(), 2);
        drop(first);
        assert_eq!(loans.free_at(screen), Err(tile));
        drop(second);
        assert_eq!(loans.out(), 0);
        assert_eq!(loans.free_at(screen), Ok(()));
        // And the size that is out follows the lease that was handed out last.
        let bigger = loans.lend(screen);
        assert_eq!(loans.free_at(tile), Err(screen));
        drop(bigger);
        assert_eq!(loans.free_at(tile), Ok(()));
    }
}
