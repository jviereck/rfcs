- Start Date: 2014-11-21
- RFC PR:
- Rust Issue:

# Summary

> One para explanation of the feature.

This RFC is about proposing a new set of pointer reference types `&init T` and `&init? T`, which allow

- to create object structures with multiple references to the same object (e.g. for linked lists, trees with back pointers to their parents)
- to have multiple mutable references to an object during a so call "initialisation phase"
- to fall back to the existing type system after the "initialisation phase", at which point the references become immutable

# Motivation

> Why are we doing this? What use cases does it support? What is the expected outcome?

At the point of writing, Rust has two basic reference pointer types `&mut T` as well as `&T`. The first one allows to mutate the target object and the invariant holds, that there can be only one mutable reference to the same object at a time. In the case of `&T` there can be multiple references to the same object, but these references do not allow mutation of the target object. These invariants on the refernce pointers ensure the desired security features in Rust of memory safety and prevention of data races in parallel settings. However, these invariants are also preventing (*V1*) the construction of object structures that contain a reference cycle, that is, there exists a path following the fields of the objects and the fields target objects again, that lead back to the original object. Examples for such object strucutres are linked lists data structures and tress with back pointers to their parents.

The Rust standard library provides an implementation for a linked list, which uses `unsafe` blocks internally. To support a tree structure with back pointers to the parent node, reference counted objects `Rc` and `RefCell`s can be used (for an example on this, see the example in the [std:rc](http://doc.rust-lang.org/std/rc/index.html)), which requires dynamic runtime checks. Both, the use of `unsafe` blocks and dynamic runtime checks, are indications of limitations of the existing type system in Rust. This RFC attempts to work around these limitations by making the type system more flexible during the initialisation / construction of object structures.

Does not provide the full ability of the curernt linked list or `Rc` and `RefCell` combo. While the creation of the object structure is possible with the ideas outlined in this RFC, the resulting object structure has either to become immutable eventually to allow sharing of the involved objects or allows mutation but then not sharing of the involved objects to other tasks.

*V1*: ((the construction of object structrues containing multiple references to the same object.) << this is not true. It is not a problem to build a object graph with e.g. A → B and C → B. Here, the object B is pointed to twice. The problem is more that with the two pointers to the object B, B cannot be modified anymore).

# Detailed design


This is the bulk of the RFC. Explain the design in enough detail for somebody familiar
with the language to understand, and for somebody familiar with the compiler to implement.
This should get into specifics and corner-cases, and include examples of how the feature is used.

# Drawbacks

Why should we *not* do this?

# Alternatives

What other designs have been considered? What is the impact of not doing this?

# Unresolved questions

What parts of the design are still TBD?
