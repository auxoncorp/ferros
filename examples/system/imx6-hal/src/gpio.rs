use crate::embedded_hal::digital::v2::OutputPin;
use crate::pac::gpio::*;
use core::{convert::Infallible, marker::PhantomData};

pub trait GpioExt {
    type Parts;

    fn split(self) -> Self::Parts;
}

/// Output mode
pub struct Output<MODE> {
    _mode: PhantomData<MODE>,
}

/// Push pull output (type state)
pub struct PushPull;

/// Disabled mode
pub struct Disabled;

pub struct Gpio {
    pub bank3: GpioBank3,
}

pub struct GpioBank3 {
    pub p3_19: P3_19<Disabled>,
}

pub struct P3_19<MODE> {
    bank: GPIO3,
    _mode: PhantomData<MODE>,
}

impl GpioExt for GPIO3 {
    type Parts = Gpio;

    fn split(self) -> Self::Parts {
        Gpio {
            bank3: GpioBank3 {
                p3_19: P3_19 {
                    bank: self,
                    _mode: PhantomData,
                },
            },
        }
    }
}

// TODO - macro gen all the pins/etc
impl<MODE> P3_19<MODE> {
    const OFFSET: usize = 19;
}

impl P3_19<Disabled> {
    #[inline]
    pub fn into_push_pull_output(mut self) -> P3_19<Output<PushPull>> {
        let val = self.bank.data.read();
        unsafe { self.bank.data.write(val | (1 << Self::OFFSET)) };

        let val = self.bank.direction.read();
        unsafe { self.bank.direction.write(val | (1 << Self::OFFSET)) };

        P3_19 {
            bank: self.bank,
            _mode: PhantomData,
        }
    }
}

impl OutputPin for P3_19<Output<PushPull>> {
    type Error = Infallible;

    fn set_high(&mut self) -> Result<(), Self::Error> {
        let val = self.bank.data.read();
        unsafe { self.bank.data.write(val | (1 << Self::OFFSET)) };
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        let val = self.bank.data.read();
        unsafe { self.bank.data.write(val & !(1 << Self::OFFSET)) };
        Ok(())
    }
}
