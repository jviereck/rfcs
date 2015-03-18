- Start Date: 2014-11-21
- RFC PR:
- Rust Issue:

# Summary

> One para explanation of the feature.

Add two new pointer reference types `&init T` and `&init? T`, which allow

- to create immutable object structures with multiple references to the same object (e.g. for linked lists, trees with back pointers to their parents), without runtime checks
- to have multiple mutable references to an object during a so-called "initialization phase"
- to fall back to the existing type system after the "initialization phase", at which point the references become immutable (`&init T` becomes then `&T`)

# Motivation

> Why are we doing this? What use cases does it support? What is the expected outcome?

Currently, Rust has two basic reference pointer types `&mut T` as well as `&T`. The first one allows to mutate the target object and the invariant holds that there can be only one mutable reference to the same object at a time. In the case of `&T` there can be multiple references to the same object, but these references do not allow mutation of the target object. These invariants on the reference pointers ensure the desired security features in Rust of memory safety and prevent data races in parallel settings. However, these invariants also prevent the construction of object structures that contain a reference cycle. Examples of such object structures are linked list data structures and tress with back-pointers to their parents.

The Rust standard library provides an implementation for a linked list, which uses `unsafe` blocks internally. To support a tree structure with back pointers to the parent node, reference counted objects `Rc` and `RefCell`s can be used (for an example of this, see the example in the [std:rc](http://doc.rust-lang.org/std/rc/index.html)), which requires dynamic runtime checks. Both, the use of `unsafe` blocks and dynamic runtime checks are indications of limitations of the existing type system in Rust. This RFC attempts to work around these limitations by making the type system more flexible during the initialization / construction of object structures.

Note: This RFC does not enable the full ability of the current linked list or `Rc` and `RefCell` combo available in Rust. While the creation of the object structure is (hopefully) possible with the ideas outlined in this RFC, the resulting object structure has either to become immutable to allow sharing of the involved objects eventually or allows mutation but then not sharing of the involved objects to other tasks.


# Detailed design

## The idea : temporary initialisation for cyclic dependencies

Let's consider first what prevents the creation of cyclic reference structures in the current type system. Consider the following code example:

``` rust
struct Node<'a> {
    next: Option<&'a Node<'a>>
}

{
  let mut a = Node({ next: None })
  let mut b = Node({ next: None })

  {
      let a_ref = &mut a;
      let b_ref = &mut b;

      a_ref.next = Some(b_ref); // (1)
      b_ref.next = Some(a_ref); // (2)

      // ... More code which uses the data structure.
  }
}
```

Initialising the above example in rust is not possible for two reasons: first a borrowed reference makes the original reference immutable and second because of the conflicting object's lifetime. On line (1), a borrowed pointer is taken from the `b_ref` pointer, which makes the `b_ref` pointer immutable so long as the `a_ref` reference is alive. As the lifetime of `a_ref` extends to the end of the block, `b_ref` is still borrowed and as it is therefore also immutable the assignment in (2) is prevented.

The key idea we propose is that it is OK to allow a temporary relaxation of the borrowing rules, effectively allowing multiple mutable borrows in order to set up such structures, so long as:
  (a) during this relaxed phase, the borrowed references cannot escape the current thread (as this could lead to race conditions).
  (b) after this relaxed phase, the borrowed references revert to types which are handled by the current type system (with which the resulting heap structure must still be compatible).

More concretely, we introduce a new pointer type that we call `&init T`, with which a second kind of lifetime is associated: its `init-time`. Rather than specifying when the reference will be deallocated (we will explain later how this is handled), a reference's init-time defines the duration of its init status, during which the relaxed borrow rules can be exploited.

An owning `T` reference can be converted to an `&init T` reference, whose associated init-time can be any scope, as usual for rust lifetimes. At the end of this scope, the reference implicitly changes type to an `&T` (immutably borrowed) type. In the example above, we would use an additional scope around the lines (1) and (2); our relaxed rules for borrowing will allow these assignments to type-check. We will explain the detailed rules in the rest of this document. Observe that initialising such structures is only part of the problem; once we create a cyclic dependency between these two references, we also need to be sure that they can be deallocated safely, avoiding dangling pointers. The current type system would force these references to have identical lifetimes.  In fact, the lifetime analysis in rust requires references that are part of a reference cycle, to either have the same lifetime or rely on weak references (which in turn require runtime checks again).

## Using TypedArenas to create |&init T| references

To fix one of the probem outlined in the last paragraph about the conflicting lifetimes of two references, we make use of the [(Typed)Arenas](http://doc.rust-lang.org/arena/struct.Arena.html) provided by the rust standard library. Using a (Typed)Arena it is possible to create multiple objects with identical lifetimes to the arena they were created from. E.g. the references `a_ref` and `b_ref` have the same lifetime in the following example:

``` rust
extern crate arena;

use arena::TypedArena;

struct Node<'a> {
    next: Option<&'a Node<'a>>
}

fn main() {
    let mut arena: TypedArena<Node> = TypedArena::with_capacity(16us);

    let a_ref : &mut Node = arena.alloc(Node { next: None });
    let b_ref : &mut Node = arena.alloc(Node { next: None });

    a_ref.next = Some(b_ref); // (3)
    b_ref.next = Some(a_ref); // (4)
}
```

To get around the problem with borrowed reference being immutable and therefore preventing the construction of reference cycles, we propose a new method on the arenas `.alloc_init` that returns a new type of reference pointer that we call `&init T`. The complete signature of this function reads shown in the following, where the lifetime is added in explicit for clearity:

```rust
fn alloc_init<'a>(&'a self, object: T) -> &'a init T
```

The above example changes then to:

``` rust
extern crate arena;

use arena::TypedArena;

struct Node<'a> {
    next: Option<&'a Node<'a>>
}

fn setup_cycle<'a>(arena: &'a TypedArena<Node>) -> &'a Node<'a>
{
    // Create two nodes of refernece type `&init`.
    let a_ref : &init Node = arena.alloc_init(Node { next: None }); // (5)
    let b_ref : &init Node = arena.alloc_init(Node { next: None });

    // Setup the cycle.
    a_ref.next = Some(b_ref); // (6)
    b_ref.next = Some(a_ref);

    // Return one of the `&init` referneces. Similar to returning an `&mut T`
    // reference, this converst the `&init T` refernece to an `&T` reference
    // while inheriting the lifetime of the original `&init T` refernece.
    //
    // SEE: https://bitbucket.org/j4c/eth-rust-cycles/src/9d8e1e0/20150317/arena_immut_return.rs
    //
    return a_ref; // (7)
}

fn main() {
    let mut arena: TypedArena<Node> = TypedArena::with_capacity(16us);
    let ref : &Node = setup_cycle(arena); // (8)

    // Can use the `ref` now as any normal immutable reference.
    ...
}
```

Note that the type annotations on line (5) and (8) are optional and only added here for
clearity.

In short, the `&init T` pointers are mutable, there can be multiple mutable borrows to the same referenced object, sharing a `&init T` reference to a different thread is not possible and there are further restrictions on the pointer when it comes to object field read and writes. The detailed semantics of the new `&init T` reference will be discussed in the next section. The conversion from an `&init T` reference to an `&T` reference happens at the return point of a function.

As an appetizer for a more complex example, here is how to setup a ring structure of size `i` using `&init T` types:

``` rust
// Reusing external, use and struct definitions from last example above.

// Create a ring structure with i nodes.
fn setup_ring<'a>(arena: &'a TypedArena<Node>, ring_size: isize): &'a Node
{
  let head_ref: &init Node = arena.alloc_init(Node { next: None });
  let prev_ref: &init Node = head_ref;
  let tmp_ref: &init Node = head_ref;

  for x in 1..ring_size {
    tmp_ref = arena.alloc_init(Node { next: None });
    prev_ref.next = Some(tmp_ref);
    prev_ref = tmp_ref;
  }

  prev_ref.next = Some(head_ref);

  return head_ref;
}

fn main() {
  let mut arena: TypedArena<Node> = TypedArena::with_capacity(16us);
  let head: &Node = setup_ring(arena);

  // More operations on the ring_head here.
}
```

## TODO: Explain why it is important to NEVER get hold on an `&mut T` from an `&init T`



## Definition of `init-time` and lifetime of `&init T` and `&init? T` references

To make these new pointers work with the existing type system, the idea is to build up the data structures with cyclic references during what we call "initialisation phase" (therefore we call the pointers `&init T` pointers) and after the initialization of the data structures it is possible to get hold of an immutable reference pointer `&T` of the data structure. The conversion from an `&init T` to an `&T` happens at the return statement of a function as shown on line (7) in above example.

Similar to the lifetime of an object in rust we denote the point earliest possible point at which a `&init T` can be converted to an `&T` the "init time" of the allocated object from the typed arena. We are speaking of earliest possible point here as there might be multiple return statements in a function and in this case the earliest in term of code-line should be used. This init time playes an important role when passing the `&init T` reference to other functions and then storing the references on other references fields. Where the lifetime of an reference ensures the reference is only stored on a field if the passed in reference lives long enough (and therefore prevents freed-memory-access issues), the init time is used to prevent storing an `&init T` reference on anothers `&init T` reference's field, where the reciever object might be converted to an `&T` reference before the target object is. TODO(jviereck): Add an example here to make this paragraph more clear. ALSO: Not sure if it makes sense to talk about the init time here when it turns out it is not that much of an requirement in this RFC anyway :/ Maybe discuss it later when discussing why the `&init T` reference cannot be passed to functions?

Beside the already discussed `&init T` reference we will introduce another `&init? T` reference later for certain kind of field reads on an `&init T` reference. The lifetime of the `&init? T` is the same reading e.g. a `&T` in rust.

The deallocation of objects created from `arena::alloc_init` follow the normal deallocation strategy of the arena: When the lifetime of the arena ends, all allocated objects (including the ones from the `alloc_init(...)` call) are deallocated before the arena object itself gets deallocated.

The `&init T` reference type cannot be used for struct fields. That is, as an `&init T` reference is only alive during the initialisation phase and this RFC does not define new syntax to annotate the initialisation phase to keep this RFC simple. The same argument holds for the `&init? T` reference type.


NOTE(jviereck): In the previous version of this RFC we talked about adjusting the lifetime of the `&init T` reference. However, this is not required AFAIKT. When drafting the last iteration of the RFC I was not aware how exactly the lifetimes work and how they are assigned but given my better understanding now I am pretty certain no special rules for the `&init T` references must be imposed. To make checking the previous draft easier, I keep it as quote in the following:

> (THE FOLLOWING should be outdated as described in the comment preampting this quote.) To make the above example work, the lifetime of the `&init T` and the `&Node` reference on line (8) are different than normally defined in rust. The lifetime of an `&init T` reference is defined to equal the one from its allocated arena. In the example above, the lifetime of `a_ref` does not end at line (7) but extends to the end of the `fn main()` body at line (10). This adjustment is necessary as a `&init T` reference can be assigned to an `&T` reference after the initialisation phase has finished and all the objects stored on the `&init T` object must have a long enough lifetime. All the objects on the `&init T` reference will have a large enough lifetime as in rust a reference stored on an object must have at least the same lifetime as the object itself. Defining the lifetime in this way is possible as objects allocated from an arena are deallocated together with the arena object itself. The lifetime of an `&T` reference assigned to from an `&init T` follows the normal lifetime definition in rust, e.g. the lifetime of the `a_normal_ref` starts at line (8) and ends (9).
>
> The "initialisation phase" of an `&init T` reference equals in the simple case the normal lifetime of a reference in rust. As an example, the "initialisation phase" of `a_ref` starts at line (5) and expands to line (7).
>
> However, this definition causes problems in the following example on line (12). To work around this issue, the initialisation phase of an `&init T` reference gets extended to be at least as large as all possible assigned `&init T` references to it. With this, the initialisation phase of `c_ref` gets extended to the one of `a_ref` due to the assignment on line (11) which then makes the assignment on line (12) invalid.
>
> ``` rust
> {
>     let a_ref : &init Node = arena.alloc_init(Node { next: None });
>
>     {
>         let c_ref : &init Node = arena.alloc_init(Node { next: None });
>         c_ref.next = a_ref; // (11)
>     }
>
>     let c_normal_ref : &Node = &c_ref; // (12)
>     // Share `b_normal_ref` to different thread although reachable `a_ref` is
>     // still in the initialisation phase and might be modified.
> }
> ```
>
> **PROBLEM:** The above additional rule does not solve the problem completly as the `c_ref` instance can be aliased > :/ A possible solution is to make the initialisation phase a property of the arena itself but this makes traking the > beginning and the end of the initialisation phase really hard. E.g. if the arena is passed as argument to a > function, how to ensure statically, that there is no new call to `arena.alloc_init`, which requires the > initialisation phase to be extended? Other ideas:
> - Similar to the free and committed type system, can we restrict the construction of `&init T` types to be local to > a function and the conversion to an `&T` type happens when the function returns?
> - THIS SHOULD WORK: Restrict the assignment to the fields of an `&init T` type: The assignment is only valid if the > initialisation phase end of the passed in `&init T` is smaller or equal to the one of the target `&init T`. This > prevents the assignment on line (11). As the normal lifetime of a reference is determined in rust by the location of > the `let ...` definition, the developer is able to ensure the initialisation phase is of the right length even if e.> g. the call to `arena.alloc_init(Node { next: None });` is done from inside of a for loop:
>
> ``` rust
> // The following is pseudo code and I guess not really valid rust yet.
> // Create a ring buffer with 10 elements.
> {
>   let mut arena: TypedArena<Node> = TypedArena::with_capacity(16us);
>   let final_head_ref: &Node;
>
>   {
>     let head_ref: &init Node = arena.alloc_init(Node { next: None });
>     let prev_ref: &init Node = head_ref;
>     let tmp_ref: &init Node = head_ref;
>
>     for x in 0..10 {
>       tmp_ref = arena.alloc_init(Node { next: None });
>       prev_ref.next = Some(tmp_ref);
>       prev_ref = tmp_ref;
>     }
>
>     prev_ref.next = Some(head_ref);
>   }
>   final_head_ref = &head_ref;
> }
> ```


## Semantics of the `&init T` reference

In contrast to the previous sections, the following struct definitions for `Nodes` and `Leaf` are used throughout this section:

``` rust
struct Leaf {
    id: isize;
}

struct Node<'a> {
    next: Option<&'a Node<'a>>,

    // In addition to before, the node also has an immutable and mutable leaf
    // reference.
    leaf: &'a Leaf,
    mut_leaf: &'a mut Leaf,

    leaf_val: Leaf
}
```


The `&init T` point is similar to the `&mut T` pointer, as it allows mutation of the reference. However, instead of allowing only a single mutable reference, there can be multiple `&init T` reference during the initialisation phase of the reference. To still be confirm with the security features of rust, the following constraints must be enforced:

### Sharing properties:

- The `&init T` references cannot be shared, meaning they cannot be passed to different threads.

### Rules for borrowing of `&init T` reference

- Creating multiple `&int T` references from the same `&init T` reference pointing to the same object is possible and doesn't change any of the `&init T` properties. Similar to the mutable and immutalbe borrow we speak about a new kind of borrowing called "init borrow". The lifetime and the initialisation phase of the new obtained `&init T` reference are the same as the one borrowed from.

``` rust
    let a_ref_init : &init Node = some_arena.alloc_init( Node { ... });

    // Example of creating an "init borrow" from an `&init` refernece. Note that
    // no special `&init` prefix is required on the RHS of the assignment. That
    // means the default type inference for an assignment with the RHS being an
    // `&init T` reference is to expect an `&init T` on the LHS as well.
    let another_a_ref_init /*: &init Node */ = a_ref_init; // This works.
```

NOTE(jviereck): In the last iteration of this RFC it was not possible to take a mutable or immutable borrow of an `&init T` reference. Not sure if this is such a problem anymore. For the immutable borrow it should not be be a problem at all. For the mutable borrow to get a `&mut T` from an `&init T` it depends on how the field assignments of the object are defined in the following. E.g. if it is possible to get hold on an `&mut T`, with

``` rust
struct T<'a> {
  lhs: Option<&'a mut T<'a>>,
  rhs: Option<&'a mut T<'a>>
}
```

and it is possible to assign to the `lhs` and the `rhs` field the same object (via an `&init T` reference), than this is not sound, as it is possible to get hold of a mutable reference to the `lhs` and `rhs`, that rust treats as independent, though they end up at the same object, and therefore it is possible to e.g. share the `lhs` to a different thread while keeping an mutable reference to the same object via the `rhs` field.

DECISION(jviereck): For now, let's allow taking an immutable borrow and disallow taking an mutable borrow.

- Taking an immutable borrow from an `&init T` reference is possible and yields an `&T` reference. Same to the taking an immutable borrow from an `&mut T` reference it is possible to make multiple immutable borrows from an `&init T` that is already immutable borrowed once. Note that taking an immutable borrow from an `&init T` reference marks all `&init T` references in scope as immutable borrowed.

QUESTION(jviereck): Does this only markes all references of the same type `T` as immutable borrowed or all `&init ?` from all possible types (denoted via `?` here) as immutable borrowed?

QUESTION(jviereck): Previous iterations only allowed to take an immutable borrow after the initialisation phase has ended. Is this really a problem? I think it is not a problem, as taking an immutable borrow will cause all `&init T` references to be immutable borrowed for the lifetime of the borrow and therefore no updates to any of the `&init T` references can happen in the meantime. In addition, rust will complain if it is not able to enforce long enough lifetimes on the immutable borrow and will therefore prevent situations where the immutable borrow might not life long.

``` rust
// Attempt to creeat a counter example why taking an immutable borrow from an
// `&init T` reference is not sound. The problem is due to the violation of the
// init time on the `&init T` reference.
// HOWEVER: Turns out, this is not a problem as the lifetime requirements for
//          field assignments require the immutalbe borrow on the `&init T`
//          to last long enough then.

fn setup_b<'a>(arena: &'a TypedArena<Node>, node_for_field: &'a Node<'a>) -> &'a Node<'a>
{
    let a_ref_init : &init Node = arena.alloc_init(Node { next: None });

    // Assign the passed in node reference on an immutable field of the just
    // allocated node.
    a_ref_init.some_immutable_field = node_for_field; // (1)

    // Return
    return a_ref_init;
}

fn setup_a<'a>(arena: &'a TypedArena<Node>) -> &'a Node<'a>
{
    let a_ref_init : &init Node = arena.alloc_init(Node { next: None });

    {
        // Taking an immutable borrow of the `a_ref_init` reference.
        let a_ref /* : &'a Node<'a> */ = &a_ref_init;

        // Call the second helper function which consumes the immutable borrowed
        // reference of the `&init Node` allocated in this function.
        let res /* : &'a Node<'a> */ = setup_b(arena, a_ref);
    }

    // At the first look, it might seem like there is an issuer here now. E.g.
    // there is an immutable reference `res`, which can be shared to different
    // threads and this object contains an `&init Node` which can be mutated here.
    //
    // TURNS OUT this is not a problem:
    // To make the assignment on line (1) work, the lifetime of `node_for_field`
    // must be at least as long as the one of the reciever object. This is the
    // case given the lifetime annotations on the `setup_b` function. But exactly
    // these lifetime constraints cause the `a_ref` refernece to be borrowed
    // after the call to `setup_b` has ended (for the lifetime of `'a`), which
    // in turn mean the `a_ref_init` is immutable borrowed when the following
    // line is reached. Therefore, there is no problem due to the immutable borrow
    // that prevents any updates to an `&init T` reference.

    a_ref_init.id = 1; // Some mutable update on the Node struct.
}
```

- Taking a mutable borrow from an `&init T` reference to get hold on an `&mut T` reference is not permitted. TODO(jviereck): Check in a later iteration of this RFC if the NOTE above still applies and this statement should stay or if taking an mutable borrow can be allowed.

```rust
    let a_ref_init : &init Node = ...

    let a_ref /*: &Node */ = &a_ref_init; // IS allowed.
    let a_ref_mut /*: &mut Node */ = &mut a_ref_init; // IS NOT allowed.
```

## Moving and borrowing a value field from an `&init T` reference

This section discusses reading and borrowing of a filed like the `id` of an `&init Leaf` type.

```rust
struct Leaf {
    id: isize;
}
```

- As the only way to introduce new `&init T` reference into the system is for objects allocated from an arena via the call to `alloc_init` it should not be possible to get hold to a reference of a field via an `&init T` reference. Therefore, taking an init borrow from an value field `S` of an `&init T` reference is not allowed. (More on this point in the notes below.)

- Taking an immutable or mutable borrow from a field `f` of type `S` of an `&init T` reference is possible. As with `&init T` references there can be multiple references to the same object it becomes hard to track which values are on which `&init T` references are affected by the borrow to the field `f`. Therefore, an immutable or mutable borrow of the field `f` on some `&init T` reference causes a borrow on all fiels `f` of all `&init T` references with exactly the type `T`.

``` rust
  let a_init_0 : &init Node = arena.alloc_init(Node { id: 0, ... /* other fields */ });
  let a_init_1 : &init Node = arena.alloc_init(Node { id: 1, ... /* other fields */ });

  let id_ref_0 = &mut a_init_0.id;

  // Borrow the content of `Node.id` is rejected here as there is already a borrow
  // to the field `Node.id` above. This is necessary as there might be multiple `&init Node`
  // that point to the same object and therefore taking another mutable borrow
  // can cause two mutable borrowes to the same value, which should be prevented
  // at all cost as `&mut T` implies an unique borrow to the content.
  let id_ref_1 = &mut a_init_1.id;
```

- Similar to the behavior of `&T` and `&mut T` borrowes it is not possible to move a field value out of an `&init T` reference. Moving new values into the field is possible as long as the field `f` is not (mutable or immutable) borrowed. (SEE: https://bitbucket.org/j4c/eth-rust-cycles/src/47f28a2f2aa1dc/20150317/struct_move.rs#cl-27.)

``` rust
  let a_init : &init Node = ...
  let leaf = a_init.leaf_val; // IS NOT allowed - tries to move value from borrowed content.

  let new_leaf = Leaf { id : 42 };
  a_init.leaf_val = new_leaf; // IS allowed - updates value of borrowed content.
```

### NOTES on what's the problem with `&init S` for a value field of type `S` on a reference `&init T`

TODO(jviereck): Might want to remove this section in the final RFC version.

Taking an init borrow from a value field of an `&init T` reference without any further restrictions is not possible. The problem is, that there could be another `&init Leaf` reference to the same object and updating the field with a new value causes the destructor of the first value to run.

``` rust
let a_init_0 = ...
let a_init_1 = a_init_0;

let some_ref /* : &init Box<Leaf> */ = &init a_init_0.some_boxed_value; // (1)
let id_ref   /* : &isize */ = &some_ref.id; // (2)

// Update the boxed value on the field of the `a` object.
// This causes the destructor for the previous boxed leaf value to run and
// the above `id_ref` becomes a dangeling pointer.
//
// HOWEVER: There is no problem here, as taking an immutable borrow on `some_ref`
//   on line (2) causes all the `&init T` references to be immutable borrowed
//   (see the rules borrowing of `&init T` references in the previous section)
//   and therefore the update of the boxed value on the next line is not possible!
a_init_0.some_boxed_value = box<Leaf { id: 42 }>
```

While there is not a problem in the last example per see, the line (1) exhibits
a problem: This line introduces a `&init T` reference from a plain value type `T`.
In general this (might) mean that for any `T` a `&init T` reference can be borrowed
from, but this should not be the case, as the only way to get hold of an `&init T`
should be from an typed arena. (This is in particular important as otherwise it is
possible to do the following borrowings `T -> &init T -> T -> &mut T`, which cause
problems.)

Recall that in the current setup of rust it is possible to take a mutable reference to two distinct fields of the same mutable object and mutate them via mutable references.

## Assigning to an immutable or mutable reference field on an `&init T` reference

Throught this section we will use the following `Node` example definition:

```
struct Node<'a> {
    lhs: &'a Leaf,
    rhs: &'a mut Leaf,

    maybe: Option<&'a mut Leaf>
}

fn main() {
    let leaf_init /* : &init Leaf */ = arena.alloc_init(Leaf { ... });
    let node_init /* : &init Node */ = arena.alloc_init(Node { ... });

    // Should the following be valid?
    node_init.rhs = leaf_init;

    // Should the following be valid? (Assuming the pervious line was not executed?)
    node_init.maybe = Some(leaf_init);
}

```

- Assigning to an immutable reference field of type `&U` on an `&init T` reference is only possible for an `&U` reference. Assigning an `&init T` or an `&mut T` reference performes an immutable borrow on the reference.

PROBLEM(jviereck): Assigning to an `&mut T` is problematic. Recall that taking an mutable borrow from an `&init T` is not possible. Therefore, assigning to the `node_init.rhs = leaf_init` causes a type missmatch as `node_init.rhs : &mut Leaf` and `leaf_init : &init Leaf`. When not allowing the conversion of the `leaf_init` to an mutable borrow, this problem can be resolved by introducing the rule, that the type of an `&mut U` field on an `&init T` reference gets converted to an `&init U`. For the above example the type of `node_init.rhs` would then change to `node_init.rhs : &init Leaf`.

The assignment to the `node_init.maybe` should not be possible and it should also not be allowed to change the typing rules, e.g. make `Option<&mut Leaf>` become `Option<&init Leaf>`, as this can be exploited for unsoundness. The problem is, that `Option<&mut Leaf>` is a value of type `U` on the struct and given the rules before for borrowing of values on an `&init T` reference it is possible to take an mutable borrow for such an value. The `Option<T>` type has a method `unwrap()` which returns the content of the option and replaces the option with `None`. This way it would be possible to get hold on an `&mut T` reference that is still an ongoing `&init T`.

``` rust
    let leaf_init /* : &init Leaf */ = arena.alloc_init(Leaf { ... });
    let node_init /* : &init Node */ = arena.alloc_init(Node { ... });

    // Assume the following is possible.
    node_init.maybe = Some(leaf_init);

    // Can get hold on the `leaf_init` reference via calls to unwrap.
    let maybe_ref : &mut &mut Leaf = node_init.maybe.as_mut().unwrap();
```



Assigning an `&init T` reference via `node_init.maybe = Some(leaf_init)`

### Borrowing a field from an `&init T` reference:

- In general, reading a field of an `&init T` reference is only allowed during the initialisation phase of the reference. To get hold of the data after the initialisation phase, the `&init T` reference can be assigned to an `&T` reference and then the normal rules for reading from an `&T` reference apply.

- Reading a reference field `&T` from an `&init T` reference yields the very weak `&init? T` type.

- Reading a reference field `&mut T` from an `&init T` reference yields an `&init T` type, where the

- Reading a `T` field from an `&init T` is allowed.

### Updating and assigning a field from an `&init T` reference:

- In general, updating or assigning a field of an `&init T` reference is only allowed during the initialisation phase of the reference.

- Assigning to a mutable field `&mut T` from an `&init T` reference is possible by assigning an `&mut T` or `&init T` reference.

- Assigning to an immutable field `&T` from an `&init T` reference is possible by either assigning an `&init T`, `&init? T`, a `&T` or a `&mut T` reference. As with other rust code, the assignment of the `&mut T` reference performs a `&T` borrowing, which makes the `&mut T` immutable for the time of the borrow. As the lifetime of the `&init T` reference is defined as the lifetime of the arena it is created from, the `&mut T` reference becomes borrowed until the lifetime of the arena for the `&init T` ends.

- As the `&init T` reference is mutable, updating the value of an `T` field is possible.

- As the `&init T` reference is mutable, updating the value of an `&mut T` field is possible.

``` rust
    let a_ref_init : &init Node = ...
    let leaf_ref : & Leaf = ...
    let leaf_ref_mut : & Leaf = ...
    let leaf_ref_init : &init Leaf = ...
    a_ref_init.leaf = leaf_ref; // This works.
    a_ref_init.leaf = leaf_ref_mut; // This works.
    a_ref_init.leaf = leaf_ref_init; // This works.
```

### Using `&init T` references for function argument types

In short, passing an `&init T` reference to a function as argument is not allowed.

- As the semantics of `&init T` is incompatible with `&T` or `&mut T` it is not possible to pass  a `&init T` reference to an argument expecting either an `&T` or `&mut T` reference

- Defining the type of a function argument as `&init T` causes trouble with the existing type system and is therefore not allowed: At this point rust has annotations for the lifetime of the passed in argument reference, however, recall that the `&init T` have not only a lifetime but also an initialisation phase. One could come up with a new notation for the initialisation phase (e.g. similar to the lifetime annotation `'a' along the lines of `''a`) but to keep this RFC simple, adding such an initialisation phase annotation is part of future work. The same argument holds for the `&init? T` reference type.

# Drawbacks

- Need an entire function to craete `&init T`. Would love to have smaller scopes local to a function.

- The `&init T` references become immutable when assigning to `&T` references later on. In contrast the already available `RefCells` are more powerful in rust, as they allow the mutation of objects participating in an reference cycle at the cost of dynamic runtime checks.

- The `&init T` references can only be created from arenas which is different to the normal allocation strategy in rust.

- The `&init T` reference is incompatible with the existing `&T` and `&mut T` references. This causes troubles, e.g. when passing an `&init T` reference to existing functions that expect an `&T` or `&mut T` reference. In fact, the current RFC disallow passing an `&init T` pointer as an argument to any function at all.

- While this RFC allows to build a double linked list, this list must become immutable before elements of the list can be shared to other threads. This is in contrast to the double linked list implementation available in the rust standard library, which allows the mutation of the double linked list even though it contained elements are shared to other threads.

# Alternatives

> What other designs have been considered? What is the impact of not doing this?

- Stay with dynamic runtime checks when building cyclic data structures and/or add a garbage collector to the language.

# Unresolved questions / Future Work

> What parts of the design are still TBD?

- Beside the `&T` and `&mut T` references, rust also has references to arrays/vectors. This proposal must be extended to cover these cases as well.

- A future goal might be to extend / restrict this RFC to enable the implementation of the double linked list from rust's standard library, that allows the mutation of the linked list structure later on.

