#![no_std]

extern crate typenum;

use core::marker::PhantomData;

use typenum::consts::{True, U0, U19, U3, U4};
use typenum::{IsGreaterOrEqual, IsLessOrEqual, Unsigned};

enum InterindustrySecureMessaging {
    None,
    Proprietary,
    Authenticated,
    NotAuthenticated,
}

enum InterindustryExtendedSecureMessaging {
    None,
    NotAuthenticated,
}

enum Class<Channel: Unsigned = U0, ChannelExtended: Unsigned = U4>
where
    Channel: IsLessOrEqual<U3, Output = True>,
    ChannelExtended: IsLessOrEqual<U19, Output = True>,
    ChannelExtended: IsGreaterOrEqual<U4, Output = True>,
{
    Proprietary,
    Interindustry(Interindustry<Channel>),
    InterindustryExtended(InterindustryExtended<ChannelExtended>),
}

struct Interindustry<Channel: Unsigned>
where
    Channel: IsLessOrEqual<U3, Output = True>,
{
    last: bool,
    secure_messaging: InterindustrySecureMessaging,
    _channel: PhantomData<Channel>,
}

struct InterindustryExtended<Channel: Unsigned>
where
    Channel: IsLessOrEqual<U19, Output = True>,
    Channel: IsGreaterOrEqual<U4, Output = True>,
{
    last: bool,
    secure_messaging: InterindustryExtendedSecureMessaging,
    _channel: PhantomData<Channel>,
}
