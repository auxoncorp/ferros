use core::marker::PhantomData;
use core::ops::{Add, Sub};
use sel4_sys::{
    api_object_seL4_UntypedObject, seL4_BootInfo, seL4_CPtr, seL4_CapInitThreadCNode,
    seL4_Untyped_Retype, seL4_Word, seL4_WordBits,
};
use typenum::operator_aliases::{Add1, Diff, Shleft, Sub1};
use typenum::{
    Bit, Exp, IsGreaterOrEqual, UInt, UTerm, Unsigned, B0, B1, U1, U2, U24, U3, U32, U5, U8
};

// Type parameter glossary:
// AS: Available Slots
// B: Bits
// W: Watermark

struct CNode<AS>
where
    AS: Unsigned,
{
    cptr: seL4_CPtr,
    depth: usize,
    offset: usize,
    _slots: PhantomData<AS>,
}

struct Untyped<B, W>
where
    B: Unsigned,
    W: Unsigned,
{
    root_cnode: seL4_CPtr,
    root_cnode_depth: usize,
    root_cnode_offset: usize,
    _bits: PhantomData<B>,
    _watermark: PhantomData<W>,
}

trait Wow {
    type Output;
}

// 2 ^ 0 = 1
impl Wow for UTerm {
    type Output = U1;
}

// 2 ^ 1 = 2
impl Wow for UInt<UTerm, B1> {
    type Output = U2;
}

// 2 ^ 0 = 1 (crazy version)
impl Wow for UInt<UTerm, B0> {
    type Output = U1;
}

impl<U: Unsigned, BA: Bit, BB: Bit> Wow for UInt<UInt<U, BB>, BA>
where
    Self: Sub<U1>,
    Diff<Self, U1>: Wow,
{
    type Output = UInt<<Diff<Self, U1> as Wow>::Output, B0>;
}

// shortcut
type Wowow<A> = <A as Wow>::Output;

// fn testit() {
//     let x: <U3 as Wow>::Output = 12;
// }

trait UntypedRetype<B, W, AS, AllocBits>
where
    B: Unsigned,
    W: Unsigned,
    AS: Unsigned,
    AllocBits: Unsigned,
    <Self as UntypedRetype<B, W, AS, AllocBits>>::OutputW: typenum::Unsigned,
    <Self as UntypedRetype<B, W, AS, AllocBits>>::OutputAS: typenum::Unsigned,
    <Self as UntypedRetype<B, W, AS, AllocBits>>::AllocationBytes: typenum::Unsigned,
{
    type AllocationBytes;
    type OutputW;
    type OutputAS;

    fn retype(
        self,
        dest_cnode: CNode<AS>,
    ) -> (
        Untyped<AllocBits, Self::AllocationBytes>,
        Untyped<B, Self::OutputW>,
        CNode<Self::OutputAS>,
    );
}

impl<B, W, AS, AllocBits> UntypedRetype<B, W, AS, AllocBits> for Untyped<B, W>
where
    B: Unsigned,
    W: Unsigned,
    AS: Unsigned + Sub<B1>,

    AllocBits: Unsigned + Wow,
    Sub1<AS>: Unsigned,

    W: Sub<Wowow<AllocBits>>,
    Diff<W, Wowow<AllocBits>>: Unsigned,

    Wowow<AllocBits>: typenum::Unsigned,

{
    type AllocationBytes = Wowow<AllocBits>;
    type OutputW = Diff<W, Self::AllocationBytes>;
    type OutputAS = Sub1<AS>;

    fn retype(
        self,
        dest_cnode: CNode<AS>,
    ) -> (
        Untyped<AllocBits, Self::AllocationBytes>,
        Untyped<B, Self::OutputW>,
        CNode<Self::OutputAS>,
    ) {
        unimplemented!()
    }
}

fn testittoo() {
    let cnode: CNode<U3> = unimplemented!();
    let mem: Untyped<U5, U32> = unimplemented!();

    // let (new_one, mem, cnode): (Untyped<U3, U8>, Untyped<U5, U24>, CNode<U2>) = mem.retype(cnode);
    let (new_one, mem, cnode): (Untyped<U3, U8>, _, _) = mem.retype(cnode);
}
