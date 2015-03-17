- Start Date: 2015-03-02
- RFC PR:
- Rust Issue:

# Summary

# Motivation

Creating data structures with cyclic references is only possible by using either
dynamic runtime checks or fall back to a garbage collected memory management. The
core issue, that prevents rust to support cyclic references with the full static
power otherwise used by the language, is the way rust handles references and
especially the concept of borrowing of (mutable) references. At the point of
writing a object can have at most one effective mutable reference at the time. It is
possible to have multiple mutable references to the same time, but this requires
to mark all but at most one as borrowed, which then again brings the number of
effective useful mutable references at the same time down to one.

Having more than one effective mutable reference at the same time makes it harder
to reason about the program, which is why rust allows to have only one mutable
reference at the same time. (In the following read "only one mutable reference"
as "only one not-borrowed mutable reference".) Especially, reasoning about taking
an immutable borrow from a mutable reference, that can then be shared to different
tasks is a challenge here: How to ensure that there is not a mutable refernece
escaping and then later this mutable reference allows to mutate the underlying
object while the object is shared as immutable to other tasks, leading to possible
data races that rust tries to prevent at all cost.

In this RFC we propose a way to have mutliple mutable references at the same time
to the same object and set up a set of rules that ensure taking an immutable
reference of one of these multiple mutable references will mark all mutable
references of the object as mutable. This is achieved by grouping mutable references
together, were the groups are identified by having the same color. Actually, it
turns out to be more powerful (while at the same time not more restrictive) to
not only allow the grouping of multiple mutable references to the same object but
to allow the grouping of arbitrary multible mutable references to different objects
together. Instead of tracking if a reference is borrowed (is should be marked as
borrowed due to another (immutable) borrow) the entire group is borrowed. This
can lead to overapproximations, meaning, although only one object, that is not
interacting with any other object from the same group, is borrowed, the entire
group is borrowed. However, this overapproximation on the borrowing should not
be a problem in realtiy as developers can choose fine grained which (if any) group
a reference should be assigned to. Also, the overapproximations are required in
situations where rust does not currently provide enough annotations and therefore
it is required to mark more references as borrowed to ensure soundness for all
possible edge cases. In such a situation the type system can be adjsuted in the
future by providing more annotations, which will lead to less overapproximation.

Last but not least, this RFC manages to extend the rust type system. This means,
that the existing rules are not changed but only new rules are introduced. In
fact, by using a theory that works nicly with the existing type system and have
this extendable property enables us to reuse a lot of the power already developed
in the borrow checker and also makes is much easier to specify this feature.

The rest of the RFC is structured as follows: First the most simple cyclic
structure of a node referencing itself again is analysed and the problems with
the existing rust type system are highlighted. Based on the found limiations,
the concepts of grouping mutable references together using the concepts of colors
is introduced and the first simple new type rules (mostly extentions to the borrow
checker) are outlined. Next we explain what causes troubles when dealing with
multiple objects in the a cyclic graph (the lifetimes will cause troubles there)
and we will show how to work around this issue using arenas. Using arenas we
show of how to build complex cyclic references. Given that are now multiple
objects in the game we discuss the rules in the context of function invocation
when using the new referneces as arguments and return types. (In case this
RFC is not too long by then:) Last but not least
we discuss how to use to make structs polymorphic over the colors, which allows
then to build even more powerful data structures and allow to replace the unsafe
code of rusts current double linked list implementation from the standard library
with the ideas outlined in this RFC.

To make the following sort of a game-challenge, the authors request the following:
IF WE MANAGE TO EXPRESS DOUBLE LINKED LISTS WITHOUT UNSAFE CODE WE CLAIM VICTORY
AND WANT TO GET FREE RUST STICKERS!

# Details

## Problem with cycle of object to itself.

Consider the following rust program

``` rust
struct Node<'a> {
    next: Option<&'a mut Node<'a>>
}

fn main() {
    let mut a = Node { next: None };
    a.next = Some(&mut a /* (2) */); // (1)
}
```

The program is rejected by the rust compiler because the `a` is borrowed at
(2) on line (1) when a mutable refernece is taken from `a`. As `a` is borrowed
at this point, the value `a` becomes immutable and in particular the update
to `a.next` is no longer possible.

From a memory safety point of view, the above assignment should be possible.
However it is not possible at the moment in rust because there can only be one
mutable borrow to the same reference at the same time. Let's fix this by extending
references to a group of references. To distinquish these reference groups we
introduce a new annotation that we call "color" at the end of the `&mut` reference.
The precise definitions for "group references" and "color" will follow in just
a moment, but let's first take a look at the above example rewritten with the
new syntax:

``` rust
struct Node<'a> {
    id: isize,
    next: Option<&'a mut Node<'a>>
}

fn main() {
    let mut a = Node { id: 0, next: None };      // (3)

    {
        let a_ref  /*: &mut    Node */ = &mut a; // (4)
        let a_ref0   : &mut[c] Node    = a_ref;  // (5)
        let a_ref1 /*: &mut[c] Node */ = a_ref0; // (6)
        a_ref0.next = Some(a_ref1);
    }
}
```

Note that the type explict type annotation on (5) is necessary and the one on
(4), (6) is only for documentation purpose (and therefore commented out).

We introduce and name the synatx introduced in the last example and then explain
the semantics. As a new property of a mutable reference we introduce the concept
of a color set, which is annotated in square brackets behind the `&mut` definition.
If no color specified, then the color set of the reference is empty and is written
either by `&mut[]` or by the currently used `&mut` syntax. In the above example
the reference for `a_ref0` and `a_ref1` have the reference type `&mut[c]` and
therefore the color set is made up of the single color `c`. As most of the times
the color set will contain only one color entry, we will use the term color in
the following as a placeholder for color set and say "a reference has no color"
to indicate that the color set of a reference is empty. A color set with multiple
colors is denoted as `&mut[c,d]`, where where the set contains the two colors `c`
and `d`. As for the color names the only restriction is to not use the same labels
as used for lifetimes to avoid confusion.

All references with the same non-empty color belong to exactly one group of
references that we call "reference group". For symmetric purpose we can also
associate a reference group to every reference not having a color. For this,
we introduce a new unique property "id" for each created reference. Though these
ids are never written out in the program (as they are a latent concept) we make
use of the convention to use numbers for them and write them at the places where
the colors are expected. E.g. if we think the reference created on line (4) has
the id of `0` we can write the `&mut` more precisely as `&mut[0]`. With this
definitions, we can say, that a reference blongs to the "reference group" that
is either induced by the color set of the reference in case it is non-empty or
if the color set of the reference is empty, then the reference belongs to the
reference group induced by the reference id. From this definition it is clear,
that a colorless reference belongs is the only member of the reference group it
belongs to (given that the reference ids are unique).





Given these definitions, the above can also be rewritten in a shorter way as:

``` rust
struct Node<'a> {
    id: isize,
    next: Option<&'a mut Node<'a>>
}

fn main() {
    let mut a = Node { id: 0, next: None };

    {
        let a_ref : &mut[c] Node = &mut a;
        a_ref.next = Some(a_ref);
    }

    // Can update the `a` reference here again.
    a.id = 1;
    println!("a.id={}", a.id);
}
```



NEED TO SPECIFY:
- If a reference has a color, then all the references on the struct inherit
  this color. E.g. the type of the following is `(&mut[c] Node).next : Option<&mut[c] Node>`



## Outline for LinkedList

The current rust's LinkedList implementation is documented here:
http://doc.rust-lang.org/std/collections/struct.LinkedList.html

In the following a discussion about how to use the new color referneces to get
rid of the unsafe code in rust's LinkekdList implementation is outlined. While
the implementation with reference colors (hopefully) has the same feature set /
covers the full API of the current rust LinkedList implementation, there are two
main downsides:

1. Each LinkedList has an additional arena to spawn the internal node elements
   for the list from.
2. As the number of elements to spawn from an arena is set
   when the arena is constructed, the item size of the LinkedList must be known
   at the point of construction as well.
3. In comparison to the current LinkedList implementation, the values on each
   node must be stored in an `Option<T>` instead of a plain `T` element.

There is a fundamental implementation difference: While rust's LinkedList uses
boxes to store the nodes of the linked list the implementation here uses
references.

To make the naming between the current rust's LinkedList implementation and the
implementation proposed here easier we will call rust's LinkedList implementation
in the following `BoxLinkedList` and the proposed LinkedList using references
`RefLinkedList`.



Problem:
- While the color references makes it possible to setup the object relations
  beteween the `next` and `prev` field in the linked list the problematic part
  is about splitting the linked list into two parts
-


## STOPED working on this RFC

Working on this RFC idea has been posponsed. The main problem is the way a coloful
mutable reference can get back to a single mutable reference, which can then get
hold of mutable fields on a data struct individually, which point to the same
object and therefore allow the sharing of the recieved object while there is
another mutable reference around. E.g. consider the following dreamcode:

``` rust
struct Leaf {
  id: isize;
}

struct Node<'a> {
  lhs: Option<&'a mut Leaf>,
  rhs: Option<&'a mut Leaf>
}

fn main() {
  let mut l = Leaf { id: 0 }
  let mut n = Node { lhs: None, rhs: None };

  {
    // Create a colorful reference to the leaf object. Creating two references
    // of the same color is no problem as
    let mut l_ref_1 : &mut[c] Leaf = &mut[c] l;
    let mut l_ref_2 : &mut[c] Leaf = &mut[c] l_ref_1;

    // Create a reference to the node with the same colors as the leafs.
    let mut n_ref : &mut[c] Node = &mut[c] n;

    // At this point, because the `n_ref` has the color `c`, the fields also
    // inherit the color and therefore the type of `n_ref.lhs` is of
    //
    //   n_ref.n_ref : Option<&'a mut[c] Leaf>
    //
    // As the colors work, the following assignment is possible.
    n_ref.lhs = Some(l_ref_1);
    n_ref.rhs = Some(l_ref_2);
  }

  // At this point, the `n.lhs` and `n.rhs` point to the same object. This can
  // be exploited to share one of the field entries while the other one is still
  // mutable :/
  let mut ref lhs = n.lhs.as_mut();
  let mut ref rhs = n.rhs.as_mut();

  {
    // Create an immutable borrow of the n.lhs object.
    let lhs_ref = &lhs;
    share(lhs_ref);

    // Can stil update the rhs's id, but there is also an immutable borrow to the
    // same object, which should be impossible -> FAIL :(
    rhs.id = 2;
  }

}
```

The problem in the above example could be prevented in two ways:

1. If a reference has a color, then don't assume the field's references also to
  inherit the color by default. E.g. in the case above, the type of `n_ref.lhs`
  is then only: `n_ref.lhs : Option<&'a mut Leaf>` instead of
  `... : Option<&'a mut[c] Leaf>`.
2. (jviereck originally had another idea where colorful references could only be
  assigned to immutable fields of structs but that doesn't make any sense to me
  now that I think about it once more...)

The point 1. solves the problem, as the assignment to the `n_ref.lhs` requires
a mutable borrow without colors from `l_ref` the colorful references `l_ref_1` and
`l_ref_2` are marked as mutable borrowed during this operation. This then prevents
to store the same object on two fields of the same struct. HOWEVER, with this
definition creating an ego-cycle as outlined at the very top is no longer possible
either, as the assignment to the `node.next` field takes a mutable borrow of the
RHS of the assignemnt, which has the same color as the reciever `node` reference,
and as an borrow on a color makes all referneces of the same color as borrowed,
the reciever `node` reference is marked as borrowed and therefore immutable.




