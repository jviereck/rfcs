- Start Date: 2014-11-21
- RFC PR:
- Rust Issue:

# Summary

> One para explanation of the feature.

Add two new pointer reference types `&init T` and `&init? T`, which allow

- to create object structures with multiple references to the same object (e.g. for linked lists, trees with back pointers to their parents)
- to have multiple mutable references to an object during a so call "initialization phase"
- to fall back to the existing type system after the "initialization phase", at which point the references become immutable (`&init T` becomes then `&T`)

# Motivation

> Why are we doing this? What use cases does it support? What is the expected outcome?

Currently, Rust has two basic reference pointer types `&mut T` as well as `&T`. The first one allows to mutate the target object and the invariant holds, that there can be only one mutable reference to the same object at a time. In the case of `&T` there can be multiple references to the same object, but these references do not allow mutation of the target object. These invariants on the reference pointers ensure the desired security features in Rust of memory safety and prevent data races in parallel settings. However, these invariants are also preventing the construction of object structures that contain a reference cycle. Examples for such object structures are linked lists data structures and tress with back pointers to their parents.

The Rust standard library provides an implementation for a linked list, which uses `unsafe` blocks internally. To support a tree structure with back pointers to the parent node, reference counted objects `Rc` and `RefCell`s can be used (for an example on this, see the example in the [std:rc](http://doc.rust-lang.org/std/rc/index.html)), which requires dynamic runtime checks. Both, the use of `unsafe` blocks and dynamic runtime checks are indications of limitations of the existing type system in Rust. This RFC attempts to work around these limitations by making the type system more flexible during the initialization / construction of object structures.

Note: This RFC does not enable the full ability of the current linked list or `Rc` and `RefCell` combo available in Rust. While the creation of the object structure is (hopefully) possible with the ideas outlined in this RFC, the resulting object structure has either to become immutable to allow sharing of the involved objects eventually or allows mutation but then not sharing of the involved objects to other tasks.


# Detailed design

## Analysing what prevents cyclic references in rust

For demonstration, let's look at the following code example:

``` rust
struct Node<'a> {
    next: Option<&'a Node<'a>>
}

{
    let a_ref = &mut Node({ next: None })
    let b_ref = &mut Node({ next: None })

    a_ref.next = Some(b_ref); // (1)
    b_ref.next = Some(a_ref); // (2)
}
```

Initialising the above example in rust is not possible because of two reasons: first a borrowed reference makes the original reference immutable and second because of the conflicting object's lifetime. On line (1) a borrowed pointer is taken from the `b_ref` pointer, which makes the `b_ref` pointer immutable as long as the `a_ref` reference is alive. As the lifetime of `a_ref` extends to the end of the block, `b_ref` is still borrowed and as it is therefore also immutable the assignment in (2) is prevented.

Let's assume for now, the two references `a_ref` and `b_ref` are from a special pointer type that we call `&init T` (similar to `& T` and `&mut T`), which allows to take multiple borrows from an reference and still permits mutation. Even then the above example is rejected by the rust type checker, as the used objects lifetime's conflict. The reference `a_ref` has a larger lifetime as the `b_ref` one. As rust requires the lifetime of field references to be larger than the ones of the object itself, the assignment at (1) is not possible. In fact, the lifetime analysis in rust requires references, that are part of a reference cycle, to either have the same lifetime or rely on weak references (which in turn require runtime checks again).

## Using TypedArenas to create |&init T| references

Creating objects with the same lifetime is already possible in rust by using (Typed)Arenas. E.g. the references `a_ref` and `b_ref` have the same lifetime in the following example:

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

To get around the problem with borrowed reference being immutable and therefore preventing the construction of reference cycles, we propose a new method on the arenas `.alloc_init` that returns a new type of reference pointer that we call `&init T`. The above example changes then to:

``` rust
// Reuse same struct definition as before.

fn main() {
    let mut arena: TypedArena<Node> = TypedArena::with_capacity(16us);

    {
        let a_ref : &init Node = arena.alloc_init(Node { next: None }); // (5)
        let b_ref : &init Node = arena.alloc_init(Node { next: None });

        a_ref.next = Some(b_ref); // (6)
        b_ref.next = Some(a_ref);
    } // (7)

    let a_normal_ref : &Node = a_ref; // (8)
}
```

In short, the `&init T` pointers are mutable, there can be multiple mutable borrows to the same referenced object, sharing a `&init T` reference to a different thread is not possible and there are further restrictions on the pointer when it comes to object field read and writes. The detailed semantics of the new `&init T` reference will be discussed in the next section.

## Definition of "initialisation phase" and lifetime of "&init T" references

To make these new pointers work with the existing type system, the idea is to build up the data structures with cyclic references during what we call "initialisation phase" (therefore we call the pointers `&init T` pointers) and after the initialization of the data structures it is possible to get hold of an immutable reference pointer `&T` of the data structure. This can be done by assigning an `&init T` reference to an `&T` reference after the initialisation phase as shown on line (8). This assignment is only permitted when the initialisation phase of the `&init T` pointer has already ended.

The "initialisation phase" of an `&init T` reference equals the normal lifetime of a reference in rust. As example, the "initialisation phase" of `a_ref` starts at line (5) and expands to line (7).

To make the above example work, the lifetime of the `&init T` and the `&Node` reference on line (8) are different than normally defined in rust. The lifetime of an `&init T` reference is defined to equal the one from its allocated arena. In the example above, the lifetime of `a_ref` does not end at line (7) but extends to the end of the `fn main()` body. This adjustment is necessary as a `&init T` reference can be assigned to an `&T` reference after the initialisation phase has finished. Defining the lifetime in this way is possible as objects allocated from an arena are deallocated together with the arena object itself.

The deallocation of objects created from `arena::alloc_init` follow the normal deallocation strategy of the arena: When the lifetime of the arena ends, all allocated objects (including the ones from the `alloc_init(...)` call) are deallocated before the arena object itself gets deallocated.

QUESTION: What should the lifetime of the `a_normal_ref` on line (8) be? In principle, it is possible to define the lifetime to equal the one of the assigned `&init T` reference or should it follow more the normal rules when borrowing a reference in rust, such that the lifetime of the reference should start from the assignment and end at the end of the current block?

QUESTION: Is it possible to use `&init T` references inside of structs for fields? I (aka. jviereck) arguess this should be disallowed as the `&init T` references only make sense during an initialisation phase.

## Semantics of the `&init T` pointers

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
    mut_leaf: &'a mut Leaf
}
```


The `&init T` point is similar to the `&mut T` pointer, as it allows mutation of the reference. However, instead of allowing only a single mutable reference, there can be multiple `&init T` reference during the initialisation phase of the reference. To still be confirm with the security features of rust, the following constraints must be enforced:

### Sharing properties:

- The `&init T` references cannot be shared, meaning they cannot be passed to different threads.


### Rules for borrowing:

- Creating multiple `&int T` references from the same `&init T` reference is possible and doesn't change any of the `&init T` properties

``` rust
    let a_ref_init : &init Node = ...
    let another_a_ref_init : &init Node = a_ref_init; // This works.
```

- It is not possible to borrow a mutable or immutable reference from an `&init T` reference:

```rust
    let a_ref_init : &init Node = ...

    let a_ref : &Node = a_ref_init; // NOT allowed.
    let a_ref_mut : &mut Node = a_ref_init; // NOT allowed.
```

- Borrowing a `&mut T` reference of a `mut T` field of a `&init T` reference is not allowed as there can be multiple `&init T` reference as the same time.

- Borrowing a `&T` reference of a `T` or `mut T` field of an `&init T` reference is allowed.


### Reading a field from an `&init T` reference:

- Reading any reference field `& T` or `&mut T` from an `&init T` reference yields a very weak `&init? T` type. The `&init? T` type behaves like a `& T` type but prevents sharing to different threads (as the assigned value might be of type `&init T` which is not allowed to be shared). (QUESTION: Is it required to restrict the `&init? T` type further by e.g. disallow field reads?)


### Updating a field from an `&init T` reference:

- Assigning to an immutable field `&mut T` from an `&init T` reference is possible by assigning an `&mut T` reference.

- Assigning to an immutable field `& T` from an `&init T` reference is possible by either assigning an `&init T`, `&init? T`, a `& T` or a `&mut T` reference. As with other rust code, the assignment of the `&mut T` reference performs a `& T` borrowing, which makes the `&mut T` immutable for the time of the borrow. As the lifetime of the `&init T` reference is defined as the lifetime of the arena it is created from, the `&mut T` reference becomes borrowed until the lifetime of the arena for the `&init T` ends.

``` rust
    let a_ref_init : &init Node = ...
    let leaf_ref : & Leaf = ...
    let leaf_ref_mut : & Leaf = ...
    let leaf_ref_init : &init Leaf = ...
    a_ref_init.leaf = leaf_ref; // This works.
    a_ref_init.leaf = leaf_ref_mut; // This works.
    a_ref_init.leaf = leaf_ref_init; // This works.
```

QUESTION: Is it a problem, that reading an `&mut T` reference yields an `&init? T` reference, which can be assigned to an `& T` field reference?

### Using `&init T` references for function argument types

In short, passing an `&init T` reference to a function as argument is not allowed.

- As the semantics of `&init T` is incompatible with `& T` or `&mut T` it is not possible to pass  a `&init T` reference to an argument expecting either an `& T` or `&mut T` reference

- Defining the type of a function argument as `&init T` causes trouble with the existing type system and is therefore not allowed: At this point rust has annotations for the lifetime of the passed in argument reference, however, recall that the `&init T` have not only a lifetime but also an initialisation phase. One could come up with a new notation for the initialisation phase (e.g. similar to the lifetime annotation `'a' along the lines of `''a`) but to keep this RFC simple, adding such an initialisation phase annotation is part of future work.

# Drawbacks

- The `&init T` references become immutable when assigning to `& T` references later on. In contrast the already available `RefCells` are more powerful in rust, as they allow the mutation of objects participating in an reference cycle at the cost of dynamic runtime checks.

- The `&init T` references can only be created from arenas which is different to the normal allocation strategy in rust.

- The `&init T` reference is incompatible with the existing `& T` and `&mut T` pointers. This causes troubles, e.g. when passing an `&init T` reference to existing functions that expect an `& T` or `&mut T` reference. In fact, the current RFC disallow passing an `&init T` pointer as an argument to any function at all.

- While this RFC allows to build a double linked list, this list must become immutable before elements of the list can be shared to other threads. This is in contrast to the double linked list implementation available in the rust standard library, which allows the mutation of the double linked list even though it contained elements are shared to other threads.

# Alternatives

> What other designs have been considered? What is the impact of not doing this?

- Stay with dynamic runtime checks when building cyclic data structures and/or add a garbage collector to the language.

# Unresolved questions / Future Work

> What parts of the design are still TBD?

- Beside the `&T` and `&mut T` references, rust also has references to arrays/vectors. This proposal must be extended to cover these cases as well.

- A future goal might be to extend / restrict this RFC to enable the implementation of the double linked list from rust's standard library, that allows the mutation of the linked list structure later on.


