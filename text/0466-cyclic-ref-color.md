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



