use crate::embedded_hal::timer;
use crate::{asm, pac::gpt::*};
use void::Void;

// Using the 24MHz clock
const CLOCK_FREQ: u32 = 24_000_000;

pub struct Hertz(pub u32);

impl From<u32> for Hertz {
    fn from(hz: u32) -> Self {
        Hertz(hz)
    }
}

/// Interrupt events
pub enum Event {
    TimeOut,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Error {
    /// Timer is disabled
    Disabled,
}

pub struct Timer {
    gpt: GPT,
}

impl Timer {
    pub fn new(gpt: GPT) -> Self {
        let mut t = Timer { gpt };
        t.reset();
        t
    }

    fn reset(&mut self) {
        self.gpt.cr.modify(Control::Enable::Clear);
        self.gpt.ir.modify(
            Interrupt::OutputCompare1::Clear
                + Interrupt::OutputCompare2::Clear
                + Interrupt::OutputCompare3::Clear
                + Interrupt::InputCapture1::Clear
                + Interrupt::InputCapture2::Clear
                + Interrupt::RollOver::Clear,
        );
        self.gpt.cr.modify(Control::SwReset::Set);
        while self.gpt.cr.is_set(Control::SwReset::Set) {
            asm::nop();
        }
        self.gpt.sr.modify(
            Status::OutputCompare1::Set
                + Status::OutputCompare2::Set
                + Status::OutputCompare3::Set
                + Status::InputCapture1::Set
                + Status::InputCapture2::Set
                + Status::RollOver::Set,
        );
        self.gpt.cr.modify(
            Control::EnableMode::Set
                + Control::DebugMode::Set
                + Control::WaitMode::Set
                + Control::DozeMode::Set
                + Control::StopMode::Set
                + Control::ClockSource::CrystalOsc
                + Control::FreeRunRestartMode::RestartMode
                + Control::Enable24MClock::Set,
        );
        self.gpt
            .pr
            .modify(Prescale::Prescaler::Div1 + Prescale::Prescaler24M::Div1);
    }

    pub fn listen(&mut self, event: Event) {
        match event {
            Event::TimeOut => self.gpt.ir.modify(Interrupt::OutputCompare1::Set),
        }
    }
}

impl timer::CountDown for Timer {
    type Time = Hertz;

    fn start<T>(&mut self, timeout: T)
    where
        T: Into<Hertz>,
    {
        let timeout = timeout.into();
        debug_assert_ne!(timeout.0, 0);
        self.reset();
        let cmp = (CLOCK_FREQ / timeout.0) - 1;
        unsafe { self.gpt.ocr1.write(cmp) };
        self.gpt.cr.modify(Control::Enable::Set);
    }

    fn wait(&mut self) -> nb::Result<(), Void> {
        if self.gpt.sr.is_set(Status::OutputCompare1::Set) {
            self.gpt.sr.modify(Status::OutputCompare1::Set);
            Ok(())
        } else {
            Err(nb::Error::WouldBlock)
        }
    }
}

impl timer::Periodic for Timer {}

impl timer::Cancel for Timer {
    type Error = Error;

    fn cancel(&mut self) -> Result<(), Self::Error> {
        if !self.gpt.cr.is_set(Control::Enable::Set) {
            Err(Self::Error::Disabled)
        } else {
            self.reset();
            Ok(())
        }
    }
}
