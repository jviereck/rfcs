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

> This is the bulk of the RFC. Explain the design in enough detail for somebody familiar
> with the language to understand, and for somebody familiar with the compiler to implement.
> This should get into specifics and corner-cases, and include examples of how the feature is used.

At the core the idea behind `&init` pointers is to allow multiple mutable references to the same object during an initialization phase of a object structure. Multiple mutable references are [disallowed in Rust](https://mail.mozilla.org/pipermail/rust-dev/2014-September/011140.html) to avoid data race problems when the object is shared to other tasks and to allow the compiler to perform more aggressive optimizations. To still allow multiple mutable references, it is therefore necessary to prevent such objects from being shared to different tasks, which in return guarantees a data race free program. Restricting the objects to not be shared does not enable the otherwise aggressive optimizations, but as the initialization phase of object structures is usual small, making the trade off in terms of a slower execution speed for a small part of the code might be worth it.

## Subtyping relation of `&init T` with existing pointer types `&mut T` and `& T`

Preventing sharing of `&init ` references has direct consequences on the subtyping relations between the `&init T` pointer type and the existing `&mut T` and `& T` ones.

A `&init T` pointer cannot be shared to other tasks, which is doable for `& T` pointers. Therefore, a `&init T` pointer cannot be used at places where `& T` is expected to prevent sharing of the object. This implies that `&init T` cannot be a subtype of `& T`. Writing this down as a subtyping relation and denoting by `A </: B` "A is not a subtype of B", we get:

```
  &init T </: & T
```

Can a `& T` be used when a `&init T` is expected? Clearly not, as `&init T` are mutable but `& T` are immutable and then they cannot be a subtype of each other:


```
  & T </: &init T
```

In a similar analysis as done for the `& T` pointer, let's have a look at the relation between the `&mut T` and `&init T` pointer. A pointer of type `&mut T` can be used in places where a `& T` pointer is expected. Therefore, `&mut T` is a subtype of the `& T` pointer:

```
  &mut T <: &T
```

If we assume that `&init T` is a subtype of `&mut T`, then it would be possible to get hold of an `&T` pointer, which can be shared to different tasks. Given this argumentation, it sounds like `&init T` should not be a subtype of `&mut T` to prevent sharing via `& T` references. However, when a `& T` reference is borrowed from a `&mut T` reference, the mutable reference is marked as borrowed and therefore cannot be modified as long all borrowed pointers are returned. Isn't it therefore enough to add a similar "is borrowed" semantic on the `&init T` pointer, preventing modifications to the pointer while it is borrowed and therefore can use the `&init T` pointer in places where `&mut T` is expected? The answer to this is no. The problem is, that there can be multiple `&init T` references to the same underlying object. Making one of the `&init T` references as "borrowed" prevents this one reference from being used to perform a modification, but mutation can still happen through the other references. To make all the existing `&init T` references, that point to the same object, as "borrowed", either a non-modular analysis or additional type annotations (e.g. name the `&init T` pointers) would be necessary. Both seems very complex and in the case of the non-modular analysis undesired (given that Rust's type checker uses only modular analysis). Therefore, for the sake of simplicity, the subtyping of `&init T` to `&mut T` is disallowed:

```
  &init T </: &mut T
```

Does the inverse hold? Can a `&mut T` reference be used in places where a `&init T` is expected. Such a usage should be prevented as well, as `&mut T` implies a unique reference to the object, while `&init T` allows to create multiple ones. Therefore `&mut T` cannot be a subtype of the `&init T` pointer:

```
  &mut </: &init T
```

(Not sure - is the following argumentation also valid? Let's assume `&mut <: &init T` would be a subtype. Also we have `&mut T <: &T` and `& T </: &init T` as well as `& T </: &init T`. Assuming `<:` is transitive, then `&mut T <: &init T </: &T` violates `&mut T <: &T` and similar `&mut T <: &T </: &init T` concludes as well `&mut T </: &init T`. So `&mut T` is not a subtype of `&init T`.)

NOTE: The above statement about the subtyping relation for `&mut </: &init T` is per see correct, however, in the final specifiction the assignment of an `&mut T` pointer to an `&init T` pointer will be permitted as the assignment implies a change of the `&mut T` pointer to a `&mut T #multiplied` pointer (similar to the `#borrowed` annotation).

## Introduction of `&init T` pointers in the system

SEE: Notes from 24. Nov 2014

$ 10 Feb 2015

Basic idea:
- The `&init T` pointers are created by an assignment from an `&mut T` pointer
- Similar to how a `&mut T` pointer is annoated as `#borrowed` after the assignment to an `& T` pointer, the assignment of an `&mut T` pointer to an `&init T` pointer annotates the `&mut T` pointer as `#multiplied`
- The main point is, that a `#multiplied` annotation is still on the `&mut T` pointer after the lifetime of the `&init T` pointer.

Q: What prevents the `#multiplied` pointer from being returned from a function as a 'normal' `&mut T` poitner?
-> You cannot assign a `&mut T #multiplied` pointer to a different `&mut T` pointer. This also means, that a `&mut T #multiplied` pointer cannot be used as return type for a `&mut T` function as the types are incompatible.
-> You can assign a `&mut T #multiplied` pointer to a `& T` pointer and take a borrowed refernce this way

-> The above might work out, but it makes it hard to ensure the passed in values are of the right type! Need to make sure the lifetime matches or come up with a way to require the passing in of `&init T` pointers directly to the function

PROBLEM: Cannot use references to point to each other because of same lifetime :(

## TDS - "Test Driven Specs"

``` rust
struct Node<'a> {
	next: Option<&'a mut Node>
}

// This example should be prevented, as the passed in nodes are multiplied after
// the method call.
fn setup_refs<'a>(a: &'a mut Node<'a>, b: &'a mut Node<'a>) {
	let a_init : &init Node = a;
	let b_init : &init Node = b;

	// Assign the `next` field of the nodes to create a cyclic referenced
	//
	//  a  ---> a.next
	//  ^           |
  //  |           V
  //  b.next <--- b
  //
	a_init.next = b_init;
	b_init.next = a_init;

	// At this point, there should be an error. If the function just returns,
	// then there are two mutable references to the same objects via
	//   a.next == b
	//   b.next == a
	//
}

fn main() {
	let mut a = Node { leaf: None }
	let mut b = Node { leaf: None }

	// This call should be possible.
	// BUT: See 20150210/mut_assign.rs - rustc prevents this method call as the
	//      lifetimes of a and b are not the same.
	setup_refs(&mut a, &mut b);
}

```

WANTED:
- after the assignment of an `&mut T` to an `&init T` pointer, the original pointer should become `&mut T # borrowed`

## Viewpoint adaption for field read/write of `&init T` pointers

TODO: Talk here about the need for `&init? T` pointers.

## Defining the timespan for the initialisation phase

TODO: Talk here about the concept of init-time.

## Story for memory management with Arenas

TODO: Outline the idea of using arenas here.

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

