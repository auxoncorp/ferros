use core::cmp;
use core::marker::PhantomData;
use core::mem::{self, size_of};
use core::ops::Sub;
use core::ptr;
use crate::userland::{
    irq_state, memory_kind, role, AssignedPageDirectory, Badge, CNode, CNodeRole, Cap, FaultSource,
    IRQControl, IRQHandler, ImmobileIndelibleInertCapabilityReference, LocalCap, MappedPage,
    Notification, SeL4Error, ThreadControlBlock,
};
use sel4_sys::*;
use typenum::{IsLess, Sub1, True, Unsigned, B1, U0, U256};

impl LocalCap<ThreadControlBlock> {
    pub(super) fn configure<CNodeFreeSlots: Unsigned, VSpaceRole: CNodeRole>(
        &mut self,
        cspace_root: LocalCap<CNode<CNodeFreeSlots, role::Child>>,
        fault_source: Option<FaultSource<role::Child>>,
        vspace_cptr: ImmobileIndelibleInertCapabilityReference<
            AssignedPageDirectory<U0, VSpaceRole>,
        >, // vspace_root,
        ipc_buffer: LocalCap<MappedPage<VSpaceRole, memory_kind::General>>,
    ) -> Result<(), SeL4Error> {
        // Set up the cspace's guard to take the part of the cptr that's not
        // used by the radix.
        let cspace_root_data = unsafe {
            seL4_CNode_CapData_new(
                0,                                                   // guard
                seL4_WordBits - cspace_root.cap_data.radix as usize, // guard size in bits
            )
        }
        .words[0];

        let tcb_err = unsafe {
            seL4_TCB_Configure(
                self.cptr,
                fault_source.map_or(seL4_CapNull as usize, |source| source.endpoint.cptr), // fault_ep.cptr,
                cspace_root.cptr,
                cspace_root_data,
                vspace_cptr.get_capability_pointer(), //vspace_root.cptr,
                seL4_NilData as usize, // vspace_root_data, always 0, reserved by kernel?
                ipc_buffer.cap_data.vaddr, // buffer address
                ipc_buffer.cptr,       // bufferFrame capability
            )
        };

        if tcb_err != 0 {
            Err(SeL4Error::TCBConfigure(tcb_err))
        } else {
            Ok(())
        }
    }
}

// TODO - consider renaming for clarity
pub trait RetypeForSetup: Sized + Send + Sync {
    type Output: Sized + Send + Sync;
}

pub type SetupVer<X> = <X as RetypeForSetup>::Output;

#[derive(Debug)]
pub enum IRQError {
    UnavailableIRQ,
    SeL4Error(SeL4Error),
}
impl From<SeL4Error> for IRQError {
    fn from(e: SeL4Error) -> Self {
        IRQError::SeL4Error(e)
    }
}

impl LocalCap<IRQControl> {
    pub fn create_handler<IRQ: Unsigned, DestRole: CNodeRole, DestFreeSlots: Unsigned>(
        &mut self,
        dest_cnode: LocalCap<CNode<DestFreeSlots, DestRole>>,
    ) -> Result<
        (
            Cap<IRQHandler<IRQ, irq_state::Unset>, DestRole>,
            LocalCap<CNode<Sub1<DestFreeSlots>, DestRole>>,
        ),
        IRQError,
    >
    where
        DestFreeSlots: Sub<B1>,
        Sub1<DestFreeSlots>: Unsigned,

        IRQ: IsLess<U256, Output = True>,
    {
        if self.cap_data.known_handled[IRQ::USIZE] {
            return Err(IRQError::UnavailableIRQ);
        }
        let (dest_cnode_remainder, dest_slot) = dest_cnode.consume_slot();
        let err = unsafe {
            seL4_IRQControl_Get(
                self.cptr, // service/authority
                IRQ::USIZE,
                dest_slot.cptr,      //root
                dest_slot.offset,    //index
                seL4_WordBits as u8, //depth
            )
        };
        if err != 0 {
            return Err(IRQError::SeL4Error(SeL4Error::IRQControlGet(err)));
        }

        self.cap_data.known_handled[IRQ::USIZE] = true;

        Ok((
            Cap {
                cptr: dest_slot.offset,
                cap_data: IRQHandler {
                    _irq: PhantomData,
                    _set_state: PhantomData,
                },
                _role: PhantomData,
            },
            dest_cnode_remainder,
        ))
    }
}

impl<IRQ: Unsigned> LocalCap<IRQHandler<IRQ, irq_state::Unset>>
where
    IRQ: IsLess<U256, Output = True>,
{
    pub(crate) fn set_notification(
        self,
        notification: &LocalCap<Notification>,
    ) -> Result<(LocalCap<IRQHandler<IRQ, irq_state::Set>>), SeL4Error> {
        let err = unsafe { seL4_IRQHandler_SetNotification(self.cptr, notification.cptr) };
        if err != 0 {
            return Err(SeL4Error::IRQHandlerSetNotification(err));
        }
        Ok(Cap {
            cptr: self.cptr,
            _role: self._role,
            cap_data: IRQHandler {
                _irq: self.cap_data._irq,
                _set_state: PhantomData,
            },
        })
    }
}

impl<IRQ: Unsigned> LocalCap<IRQHandler<IRQ, irq_state::Set>>
where
    IRQ: IsLess<U256, Output = True>,
{
    pub fn ack(&self) -> Result<(), SeL4Error> {
        let err = unsafe { seL4_IRQHandler_Ack(self.cptr) };
        if err != 0 {
            return Err(SeL4Error::IRQHandlerAck(err));
        }
        Ok(())
    }
}

impl LocalCap<Notification> {
    /// Blocking wait on a notification
    pub(crate) fn wait(&self) -> Badge {
        let mut sender_badge: usize = 0;
        unsafe {
            seL4_Wait(self.cptr, &mut sender_badge as *mut usize);
        };
        Badge::from(sender_badge)
    }
}

/// Set up the target registers and stack to pass the parameter. See
/// http://infocenter.arm.com/help/topic/com.arm.doc.ihi0042f/IHI0042F_aapcs.pdf
/// "Procedure Call Standard for the ARM Architecture", Section 5.5
///
/// Returns a tuple of (regs, stack_extent), where regs only has r0-r3 set.
pub(crate) unsafe fn setup_initial_stack_and_regs(
    param: *const usize,
    param_size: usize,
    stack_top: *mut usize,
) -> (seL4_UserContext, usize) {
    let word_size = size_of::<usize>();

    // The 'tail' is the part of the parameter that doesn't fit in the
    // word-aligned part.
    let tail_size = param_size % word_size;

    // The parameter must be zero-padded, at the end, to a word boundary
    let padding_size = if tail_size == 0 {
        0
    } else {
        word_size - tail_size
    };
    let padded_param_size = param_size + padding_size;

    // 4 words are stored in registers, so only the remainder needs to go on the
    // stack
    let param_size_on_stack =
        cmp::max(0, padded_param_size as isize - (4 * word_size) as isize) as usize;

    let mut regs: seL4_UserContext = mem::zeroed();

    // The cursor pointer to traverse the parameter data word one word at a
    // time
    let mut p = param;

    // This is the pointer to the start of the tail.
    let tail = (p as *const u8).add(param_size).sub(tail_size);

    // Compute the tail word ahead of time, for easy use below.
    let mut tail_word = 0usize;
    if tail_size >= 1 {
        tail_word |= *tail.add(0) as usize;
    }

    if tail_size >= 2 {
        tail_word |= (*tail.add(1) as usize) << 8;
    }

    if tail_size >= 3 {
        tail_word |= (*tail.add(2) as usize) << 16;
    }

    // Fill up r0 - r3 with the first 4 words.

    if p < tail as *const usize {
        // If we've got a whole word worth of data, put the whole thing in
        // the register.
        regs.r0 = *p;
        p = p.add(1);
    } else {
        // If not, store the pre-computed tail word here and be done.
        regs.r0 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.r1 = *p;
        p = p.add(1);
    } else {
        regs.r1 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.r2 = *p;
        p = p.add(1);
    } else {
        regs.r2 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.r3 = *p;
        p = p.add(1);
    } else {
        regs.r3 = tail_word;
        return (regs, 0);
    }

    // The rest of the data goes on the stack.
    if param_size_on_stack > 0 {
        // TODO: stack pointer is supposed to be 8-byte aligned on ARM 32
        let sp = (stack_top as *mut u8).sub(param_size_on_stack);
        ptr::copy_nonoverlapping(p as *const u8, sp, param_size_on_stack);
    }

    (regs, param_size_on_stack)
}

#[cfg(feature = "test")]
pub mod test {
    use super::*;
    use proptest::test_runner::TestError;

    #[cfg(feature = "test")]
    fn check_equal(name: &str, expected: usize, actual: usize) -> Result<(), TestError<()>> {
        if (expected != actual) {
            Err(TestError::Fail(
                format!(
                    "{} didn't match. Expected: {:08x}, actual: {:08x}",
                    name, expected, actual
                )
                .into(),
                (),
            ))
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "test")]
    fn test_stack_setup_case<T: Sized>(
        param: T,
        r0: usize,
        r1: usize,
        r2: usize,
        r3: usize,
        stack0: usize,
        sp_offset: usize,
    ) -> Result<(), TestError<()>> {
        use core::mem::size_of_val;
        let mut fake_stack = [0usize; 1024];

        let param_size = size_of_val(&param);

        let (regs, stack_extent) = unsafe {
            setup_initial_stack_and_regs(
                &param as *const T as *const usize,
                param_size,
                (&mut fake_stack[0] as *mut usize).add(1024),
            )
        };

        check_equal("r0", r0, regs.r0)?;
        check_equal("r1", r1, regs.r1)?;
        check_equal("r2", r2, regs.r2)?;
        check_equal("r3", r3, regs.r3)?;
        check_equal("top stack word", stack0, fake_stack[1023])?;
        check_equal("sp_offset", sp_offset, stack_extent)?;

        Ok(())
    }

    #[cfg(feature = "test")]
    #[rustfmt::skip]
    pub fn test_stack_setup() -> Result<(), TestError<()>> {
        test_stack_setup_case(42u8,
                              42, 0, 0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8],
                              2 << 8 | 1, // r0
                              0, 0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8, 3u8],
                              3 << 16 | 2 << 8 | 1, // r0
                              0, 0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8, 3u8, 4u8],
                              4 << 24 | 3 << 16 | 2 << 8 | 1, // r0
                              0, 0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8, 3u8, 4u8, 5u8],
                              4 << 24 | 3 << 16 | 2 << 8 | 1, // r0
                                                           5, // r1
                              0, 0, 0, 0)?;

        test_stack_setup_case([1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 9u8],
                              4 << 24 | 3 << 16 | 2 << 8 | 1, // r0
                              8 << 24 | 7 << 16 | 6 << 8 | 5, // r1
                                                           9, // r2
                              0, 0, 0)?;

        test_stack_setup_case([ 1u8,  2u8,  3u8,  4u8,  5u8, 6u8, 7u8, 8u8,
                                9u8, 10u8, 11u8, 12u8, 13u8],
                                4 << 24 |  3 << 16 |  2 << 8 |  1,  // r0
                                8 << 24 |  7 << 16 |  6 << 8 |  5,  // r1
                               12 << 24 | 11 << 16 | 10 << 8 |  9,  // r2
                                                               13,  // r3
                              0, 0)?;

        test_stack_setup_case([ 1u8,  2u8,  3u8,  4u8,  5u8,  6u8,  7u8,  8u8,
                                9u8, 10u8, 11u8, 12u8, 13u8, 14u8, 15u8, 16u8,
                               17u8, 18u8],
                                4 << 24 |  3 << 16 |  2 << 8 |  1,   // r0
                                8 << 24 |  7 << 16 |  6 << 8 |  5,   // r1
                               12 << 24 | 11 << 16 | 10 << 8 |  9,   // r2
                               16 << 24 | 15 << 16 | 14 << 8 | 13,   // r3
                                                     18 << 8 | 17,   // stack top
                                                                4)?; // sp offset

        Ok(())
    }

}
