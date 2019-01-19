use crate::pow::Pow;
use crate::userland::{self, role, CNode, CNodeRole, Cap, Endpoint, LocalCap, Untyped};
use typenum::operator_aliases::Diff;
use typenum::{U12, U2, U6};

pub struct OverRegisterSizeParams {
    pub nums: [usize; 140],
}

impl userland::RetypeForSetup for OverRegisterSizeParams {
    type Output = OverRegisterSizeParams;
}

// 'extern' to force C calling conventions
pub extern "C" fn param_size_run(p: OverRegisterSizeParams) {
    debug_println!("");
    debug_println!("*** Hello from the param_size_run feL4 process!");
    for i in p.nums.iter() {
        debug_println!("  {:08x}", i);
    }

    debug_println!("");
}

#[derive(Debug)]
pub struct CapManagementParams<Role: CNodeRole> {
    pub num: usize,
    pub my_cnode: Cap<CNode<Diff<Pow<U12>, U2>, Role>, Role>,
    pub data_source: Cap<Untyped<U6>, Role>,
}

impl userland::RetypeForSetup for CapManagementParams<role::Local> {
    type Output = CapManagementParams<role::Child>;
}

// 'extern' to force C calling conventions
pub extern "C" fn cap_management_run(p: CapManagementParams<role::Local>) {
    debug_println!("");
    debug_println!("--- Hello from the cap_management_run feL4 process!");

    debug_println!("Let's split an untyped inside child process");
    let (ut_kid_a, ut_kid_b, cnode) = p
        .data_source
        .split(p.my_cnode)
        .expect("child process split untyped");
    debug_println!("We got past the split in a child process\n");

    debug_println!("Let's make an Endpoint");
    let (_endpoint, cnode): (LocalCap<Endpoint>, _) = ut_kid_a
        .retype_local(cnode)
        .expect("Retype local in a child process failure");
    debug_println!("Successfully built an Endpoint\n");

    debug_println!("And now for a delete in a child process");
    ut_kid_b.delete(&cnode).expect("child process delete a cap");
    debug_println!("Hey, we deleted a cap in a child process");
    debug_println!("Split, retyped, and deleted caps in a child process");
}
