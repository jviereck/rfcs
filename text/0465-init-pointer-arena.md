- Start Date: 2014-11-21
- RFC PR:
- Rust Issue:

# Summary

> One para explanation of the feature.

This RFC is about proposing a new set of pointer reference types `&init T` and `&init? T`, which allow

- to create object structures with multiple references to the same object (e.g. for linked lists, trees with back pointers to their parents)
- to have multiple mutable references to an object during a so call "initialization phase"
- to fall back to the existing type system after the "initialization phase", at which point the references become immutable (`&init T` becomes then `&T`)

# Motivation

> Why are we doing this? What use cases does it support? What is the expected outcome?

At the point of writing, Rust has two basic reference pointer types `&mut T` as well as `&T`. The first one allows to mutate the target object and the invariant holds, that there can be only one mutable reference to the same object at a time. In the case of `&T` there can be multiple references to the same object, but these references do not allow mutation of the target object. These invariants on the reference pointers ensure the desired security features in Rust of memory safety and prevention of data races in parallel settings. However, these invariant are also preventing the construction of object structures that contain a reference cycle, that is, there exists a path following the fields of the objects and the fields target objects again, that lead back to the original object (Wrote before (*V1*), which is a false statement I think). Examples for such object structures are linked lists data structures and tress with back pointers to their parents.

The Rust standard library provides an implementation for a linked list, which uses `unsafe` blocks internally. To support a tree structure with back pointers to the parent node, reference counted objects `Rc` and `RefCell`s can be used (for an example on this, see the example in the [std:rc](http://doc.rust-lang.org/std/rc/index.html)), which requires dynamic runtime checks. Both, the use of `unsafe` blocks and dynamic runtime checks are indications of limitations of the existing type system in Rust. This RFC attempts to work around these limitations by making the type system more flexible during the initialization / construction of object structures.

Note: This RFC does not enable the full ability of the current linked list or `Rc` and `RefCell` combo available in Rust. While the creation of the object structure is (hopefully) possible with the ideas outlined in this RFC, the resulting object structure has either to become immutable to allow sharing of the involved objects eventually or allows mutation but then not sharing of the involved objects to other tasks.

*V1*: "the construction of object structures containing multiple references to the same object."" << this is not true. It is not a problem to build a object graph with e.g. A -> B and C -> B. Here, the object B is pointed to twice. The problem is more that with the two pointers to the object B, B cannot be modified anymore.

# Detailed design

## Using TypedArenas to create &init T references


## Code Example:

Small code example

```rust
struct Leaf {
    id: isize;
}

struct Node<'a> {
    next: Option<&'a Node<'a>>,
    leaf: &'a Leaf,
    mut_leaf: &'a mut Leaf
}

let mut arena: TypedArena<Node> = TypedArena::with_capacity(16us);

{
    // Create `&init T` references from the arena. The lifetime of these objects
    // is the same lifetime as the one of the arena. This is important to ensure
    // the objects assigned to the `&init T` objects have a long enough lifetime
    // in case the `&init T` reference is converted to an `& T` reference later on.
    //
    // To make the interaction with the `& T` references later work, these
    // references have a new kind of lifetime called `init-time`, which
    // corresponds to the normal lifetime as you would expect it from these
    // objects (aka. till the end of the scope in this case)/
    let a_ref_init : &init Node = arena.allocInit(Node { next: None, ... });
    let b_ref_init : &init Node = arena.allocInit(Node { next: None, ... });

    a_ref_init.next = b_ref_init;
    b_ref_init.next = a_ref_init;
}

// The following is a bit of language magic. It converts the `&init T` type to a
// `& T` type. This is only allowed if the `init-time` of the passed in `&init T`
// references has already finished.
let a_ref : &Node = arena.ref_from_init(a_ref_init);
```

## Semantics of the `&init T` pointers

The `&init T` point is similar to the `&mut T` pointer, as it allows mutation of the reference but instead of allowing only a single mutable reference, there can be multiple `&init T` reference at the same time. To still be confirm with the security features of rust, the following constraints must be enforced:

- The `&init T` references cannot be shared, meaning they cannot be passed to different threads.

- It is not possible to borrow a mutable or immutable reference from an `&init T` reference:

```rust
    let a_ref_init : &init Node = ...

    let a_ref : &Node = a_ref_init; // NOT allowed.
    let a_ref_mut : &mut Node = a_ref_init; // NOT allowed.
```

- Creating multiple `&int T` references from the same `&init T` reference is possible and doesn't change any of the `&init T` properties

``` rust
    let a_ref_init : &init Node = ...
    let another_a_ref_init : &init Node = a_ref_init; // This works.
```

- Assigning to an immutable field `& T` from an `&init T` reference is possible by either assigning an `&init T`, `&init? T`, a `& T` or a `&mut T` reference. As with other rust code, the assignment of the `&mut T` reference performs a `& T` borrowing, which makes the `&mut T` immutable for the time of the borrow. As the lifetime of the `&init T` refernece is defined as the lifetime of the arena it is created from, the `&mut T` reference becomes borrowed until the lifetime of the arena for the `&init T` ends.

``` rust
    let a_ref_init : &init Node = ...
    let leaf_ref : & Leaf = ...
    let leaf_ref_mut : & Leaf = ...
    let leaf_ref_init : &init Leaf = ...
    a_ref_init.leaf = leaf_ref; // This works.
    a_ref_init.leaf = leaf_ref_mut; // This works.
    a_ref_init.leaf = leaf_ref_init; // This works.
```

- Reading an `&mut T` field from an `&init T` reference results in a reference of type `&init T`. The result cannot be of type `&mut T`, as there can be multiple copies of the same `&init T` reference at the same time and if the `&mut T` field lookup would yield `&mut T` again, then there could be multiple `&mut T` references to the same object at the same time, which is not allowed by the rust type system.

- Reading an `& T` field from an `&init T` reference yields the very weak `&init? T` type. The `&init? T` type behaves like a `& T` type but prevents sharing to different threads (as the assigned value might be of type `&init T` which is not allowed to be shared). (QUESTION: Is it required to restrict the `&init? T` type further by e.g. disallow field reads?)

- Borrowing a `&mut T` reference of a `mut T` field of a `&init T` reference is not allowed as there can be multiple `&init T` reference as the same time.

- Borrowing a `&T` reference of a `T` or `mut T` field of an `&init T` reference is allowed.

# Drawbacks

Why should we *not* do this?

# Alternatives

What other designs have been considered? What is the impact of not doing this?

# Unresolved questions / Future Work

> What parts of the design are still TBD?

- memory management: are there problems when objects are linked in possible cyclic object structures? Can such cyclic structures cause double freeing?

- what should be the semantics for the `drop` trait? As the objects contain cycles a reference on one object might not be around anymore as it was dropped already and accessing the object could cause segfaults. As simple solution, implementing the `Drop` trait for objects stored to `&init` pointer could be disallowed. (Open question: is it possible to figure out, if there is a `Drop` implementation for a given strut in a modular fashion?)




---

Resources:
- rust/src/libcollections/dlist.rs - double linked list implementation

