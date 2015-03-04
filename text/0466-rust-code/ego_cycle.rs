struct Node<'a> {
    next: Option<&'a mut Node<'a>>
}

fn main() {
    let mut a = Node { next: None };
    a.next = Some(&mut a);
}
