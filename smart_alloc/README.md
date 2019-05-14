# smart_alloc

A macro that makes strongly typed allocation easier.

## Usage

### Implicit Resource-Kinds

Here we assume that `cslots` is of type `ferros::userland::CNodeSlots<Size, Role>`
and that `untypeds` is of type `ferros::alloc::UTBuddy<PoolSizes>`. The generic
type parameters of both are allowed to vary.

In this example call, `cs` will be used as a place-marker for a spot that requests
some CNodeSlots capacity, and `ut` for a spot that requires some untyped memory capacity,
a.k.a. `LocalCap<Untyped<BitSize>>`. The macro will generate `alloc` calls that reserve
some resource capacity and bind it to an id for each request.
That id will be placed in the location of the requesting id (e.g. one of the `cs` or `ut` here).

```rust
smart_alloc!(|cs: cslots, ut: untypeds| {
    let id_that_will_leak = something_requiring_slots(cs);
    op_requiring_memory(ut);
    top_fn(cs, nested_fn(cs, ut));
});
```

### Explicit Resource-Kinds

```rust
smart_alloc!(|cs: cslots<CNodeSlots>, ut: untypeds<UntypedBuddy> | {
    let id_that_will_leak = something_requiring_slots(cs);
    op_requiring_memory(ut);
    top_fn(cs, nested_fn(cs, ut));
});
```

### Nested Invocations

Note the use of bracket-style macro invocation of the nested macro call.
This is currently needed for a nested invocation to gain access to resources
directly from the parent invocation.

```rust
smart_alloc!(|slots: local_slots, ut: uts| {
    let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
    
    smart_alloc!{ |slots_c: child_slots| {
        let (cnode_for_child, slots_for_child) =
            child_cnode.generate_self_reference(&root_cnode, slots_c)?;
        let child_ut = ut.move_to_slot(&root_cnode, slots_c)?;
    }};
});
```
