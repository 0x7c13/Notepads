// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses and attribution below remain applicable.

//! A DOM-like tree data structure based on `&Node` references.
//!
//! Included from <https://github.com/SimonSapin/rust-forest/blob/5783c8be8680b84c0438638bdee07d4e4aca40ac/arena-tree/lib.rs>.
//! MIT license (per Cargo.toml).
//!
//! Any non-trivial tree involves reference cycles
//! (e.g. if a node has a first child, the parent of the child is that node).
//! To enable this, nodes need to live in an arena allocator
//! such as `arena::TypedArena` distributed with rustc (which is `#[unstable]` as of this writing)
//! or [`typed_arena::Arena`](https://crates.io/crates/typed-arena).
//!
//! If you need mutability in the node’s `data`,
//! make it a cell (`Cell` or `RefCell`) or use cells inside of it.

use std::cell::{BorrowError, Cell, Ref, RefCell, RefMut};
use std::fmt;

/// A node inside a DOM-like tree.
pub struct Node<'a, T: 'a> {
    parent: Cell<Option<&'a Node<'a, T>>>,
    previous_sibling: Cell<Option<&'a Node<'a, T>>>,
    next_sibling: Cell<Option<&'a Node<'a, T>>>,
    first_child: Cell<Option<&'a Node<'a, T>>>,
    last_child: Cell<Option<&'a Node<'a, T>>>,

    /// The data held by the node.
    pub data: T,
}

/// A simple Debug implementation that prints the children as a tree, without
/// looping through the various interior pointer cycles.
impl<'a, T: 'a> fmt::Debug for Node<'a, RefCell<T>>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        struct Children<'a, T>(Option<&'a Node<'a, RefCell<T>>>);
        impl<T: fmt::Debug> fmt::Debug for Children<'_, T> {
            fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
                f.debug_list()
                    .entries(std::iter::successors(self.0, |child| {
                        child.next_sibling.get()
                    }))
                    .finish()
            }
        }

        match self.data.try_borrow() {
            Ok(data) => {
                write!(f, "{data:?}")?;
            }
            _ => {
                write!(f, "!!mutably borrowed!!")?;
            }
        }
        if let Some(first_child) = self.first_child.get() {
            write!(f, " {:?}", Children(Some(first_child)))?;
        }
        Ok(())
    }
}

impl<'a, T> Node<'a, T> {
    /// Create a new node from its associated data.
    ///
    /// Typically, this node needs to be moved into an arena allocator
    /// before it can be used in a tree.
    pub fn new(data: T) -> Node<'a, T> {
        Node {
            parent: Cell::new(None),
            first_child: Cell::new(None),
            last_child: Cell::new(None),
            previous_sibling: Cell::new(None),
            next_sibling: Cell::new(None),
            data,
        }
    }

    /// Return a reference to the parent node, unless this node is the root of the tree.
    pub fn parent(&self) -> Option<&'a Node<'a, T>> {
        self.parent.get()
    }

    /// Return a reference to the first child of this node, unless it has no child.
    pub fn first_child(&self) -> Option<&'a Node<'a, T>> {
        self.first_child.get()
    }

    /// Return a reference to the last child of this node, unless it has no child.
    pub fn last_child(&self) -> Option<&'a Node<'a, T>> {
        self.last_child.get()
    }

    /// Return a reference to the previous sibling of this node, unless it is a first child.
    pub fn previous_sibling(&self) -> Option<&'a Node<'a, T>> {
        self.previous_sibling.get()
    }

    /// Return a reference to the next sibling of this node, unless it is a last child.
    pub fn next_sibling(&self) -> Option<&'a Node<'a, T>> {
        self.next_sibling.get()
    }

    /// Returns whether two references point to the same node.
    pub fn same_node(&self, other: &Node<'a, T>) -> bool {
        std::ptr::eq(self, other)
    }

    /// Return an iterator of references to this node and its ancestors.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn ancestors(&'a self) -> Ancestors<'a, T> {
        Ancestors(Some(self))
    }

    /// Return an iterator of references to this node and the siblings before it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn preceding_siblings(&'a self) -> PrecedingSiblings<'a, T> {
        PrecedingSiblings(Some(self))
    }

    /// Return an iterator of references to this node and the siblings after it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn following_siblings(&'a self) -> FollowingSiblings<'a, T> {
        FollowingSiblings(Some(self))
    }

    /// Return an iterator of references to this node’s children.
    pub fn children(&'a self) -> Children<'a, T> {
        Children(self.first_child.get())
    }

    /// Return an iterator of references to this node’s children, in reverse order.
    pub fn reverse_children(&'a self) -> ReverseChildren<'a, T> {
        ReverseChildren(self.last_child.get())
    }

    /// Return an iterator of references to this `Node` and its descendants, in tree order.
    ///
    /// Parent nodes appear before the descendants.
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    ///
    /// *Similar Functions:* Use `traverse()` or `reverse_traverse` if you need
    /// references to the `NodeEdge` structs associated with each `Node`
    pub fn descendants(&'a self) -> Descendants<'a, T> {
        Descendants(self.traverse())
    }

    /// Return an iterator of references to `NodeEdge` enums for each `Node` and its descendants,
    /// in tree order.
    ///
    /// `NodeEdge` enums represent the `Start` or `End` of each node.
    ///
    /// *Similar Functions:* Use `descendants()` if you don't need `Start` and `End`.
    pub fn traverse(&'a self) -> Traverse<'a, T> {
        Traverse {
            root: self,
            next: Some(NodeEdge::Start(self)),
        }
    }

    /// Return an iterator of references to `NodeEdge` enums for each `Node` and its descendants,
    /// in *reverse* order.
    ///
    /// `NodeEdge` enums represent the `Start` or `End` of each node.
    ///
    /// *Similar Functions:* Use `descendants()` if you don't need `Start` and `End`.
    pub fn reverse_traverse(&'a self) -> ReverseTraverse<'a, T> {
        ReverseTraverse {
            root: self,
            next: Some(NodeEdge::End(self)),
        }
    }

    /// Detach a node from its parent and siblings. Children are not affected.
    pub fn detach(&self) {
        let parent = self.parent.take();
        let previous_sibling = self.previous_sibling.take();
        let next_sibling = self.next_sibling.take();

        if let Some(next_sibling) = next_sibling {
            next_sibling.previous_sibling.set(previous_sibling);
        } else if let Some(parent) = parent {
            parent.last_child.set(previous_sibling);
        }

        if let Some(previous_sibling) = previous_sibling {
            previous_sibling.next_sibling.set(next_sibling);
        } else if let Some(parent) = parent {
            parent.first_child.set(next_sibling);
        }
    }

    /// Append a new child to this node, after existing children.
    pub fn append(&'a self, new_child: &'a Node<'a, T>) {
        new_child.detach();
        new_child.parent.set(Some(self));
        if let Some(last_child) = self.last_child.take() {
            new_child.previous_sibling.set(Some(last_child));
            debug_assert!(last_child.next_sibling.get().is_none());
            last_child.next_sibling.set(Some(new_child));
        } else {
            debug_assert!(self.first_child.get().is_none());
            self.first_child.set(Some(new_child));
        }
        self.last_child.set(Some(new_child));
    }

    /// Append multiple new children to this node, after existing children.
    pub fn extend(&'a self, new_children: impl IntoIterator<Item = &'a Node<'a, T>>) {
        for child in new_children.into_iter() {
            self.append(child);
        }
    }

    /// Prepend a new child to this node, before existing children.
    pub fn prepend(&'a self, new_child: &'a Node<'a, T>) {
        new_child.detach();
        new_child.parent.set(Some(self));
        if let Some(first_child) = self.first_child.take() {
            debug_assert!(first_child.previous_sibling.get().is_none());
            first_child.previous_sibling.set(Some(new_child));
            new_child.next_sibling.set(Some(first_child));
        } else {
            debug_assert!(self.first_child.get().is_none());
            self.last_child.set(Some(new_child));
        }
        self.first_child.set(Some(new_child));
    }

    /// Insert a new sibling after this node.
    pub fn insert_after(&'a self, new_sibling: &'a Node<'a, T>) {
        new_sibling.detach();
        new_sibling.parent.set(self.parent.get());
        new_sibling.previous_sibling.set(Some(self));
        if let Some(next_sibling) = self.next_sibling.take() {
            debug_assert!(std::ptr::eq(
                next_sibling.previous_sibling.get().unwrap(),
                self
            ));
            next_sibling.previous_sibling.set(Some(new_sibling));
            new_sibling.next_sibling.set(Some(next_sibling));
        } else if let Some(parent) = self.parent.get() {
            debug_assert!(std::ptr::eq(parent.last_child.get().unwrap(), self));
            parent.last_child.set(Some(new_sibling));
        }
        self.next_sibling.set(Some(new_sibling));
    }

    /// Insert a new sibling before this node.
    pub fn insert_before(&'a self, new_sibling: &'a Node<'a, T>) {
        new_sibling.detach();
        new_sibling.parent.set(self.parent.get());
        new_sibling.next_sibling.set(Some(self));
        if let Some(previous_sibling) = self.previous_sibling.take() {
            new_sibling.previous_sibling.set(Some(previous_sibling));
            debug_assert!(std::ptr::eq(
                previous_sibling.next_sibling.get().unwrap(),
                self
            ));
            previous_sibling.next_sibling.set(Some(new_sibling));
        } else if let Some(parent) = self.parent.get() {
            debug_assert!(std::ptr::eq(parent.first_child.get().unwrap(), self));
            parent.first_child.set(Some(new_sibling));
        }
        self.previous_sibling.set(Some(new_sibling));
    }
}

macro_rules! axis_iterator {
    (#[$attr:meta] $name:ident : $next:ident) => {
        #[$attr]
        pub struct $name<'a, T: 'a>(Option<&'a Node<'a, T>>);

        impl<'a, T: 'a> fmt::Debug for $name<'a, RefCell<T>>
        where
            T: fmt::Debug,
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }

        impl<'a, T> Iterator for $name<'a, T> {
            type Item = &'a Node<'a, T>;

            fn next(&mut self) -> Option<&'a Node<'a, T>> {
                match self.0.take() {
                    Some(node) => {
                        self.0 = node.$next.get();
                        Some(node)
                    }
                    None => None,
                }
            }
        }
    };
}

axis_iterator! {
    #[doc = "An iterator of references to the ancestors a given node."]
    Ancestors: parent
}

axis_iterator! {
    #[doc = "An iterator of references to the siblings before a given node."]
    PrecedingSiblings: previous_sibling
}

axis_iterator! {
    #[doc = "An iterator of references to the siblings after a given node."]
    FollowingSiblings: next_sibling
}

axis_iterator! {
    #[doc = "An iterator of references to the children of a given node."]
    Children: next_sibling
}

axis_iterator! {
    #[doc = "An iterator of references to the children of a given node, in reverse order."]
    ReverseChildren: previous_sibling
}

/// An iterator of references to a given node and its descendants, in tree order.
pub struct Descendants<'a, T: 'a>(Traverse<'a, T>);

impl<'a, T: 'a> fmt::Debug for Descendants<'a, RefCell<T>>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Descendants").field(&self.0).finish()
    }
}

impl<'a, T> Iterator for Descendants<'a, T> {
    type Item = &'a Node<'a, T>;

    fn next(&mut self) -> Option<&'a Node<'a, T>> {
        loop {
            match self.0.next() {
                Some(NodeEdge::Start(node)) => return Some(node),
                Some(NodeEdge::End(_)) => {}
                None => return None,
            }
        }
    }
}

/// An edge of the node graph returned by a traversal iterator.
#[derive(Debug, Clone)]
pub enum NodeEdge<T> {
    /// Indicates that start of a node that has children.
    /// Yielded by `Traverse::next` before the node’s descendants.
    /// In HTML or XML, this corresponds to an opening tag like `<div>`
    Start(T),

    /// Indicates that end of a node that has children.
    /// Yielded by `Traverse::next` after the node’s descendants.
    /// In HTML or XML, this corresponds to a closing tag like `</div>`
    End(T),
}

macro_rules! traverse_iterator {
    (#[$attr:meta] $name:ident : $first_child:ident, $next_sibling:ident) => {
        #[$attr]
        pub struct $name<'a, T: 'a> {
            root: &'a Node<'a, T>,
            next: Option<NodeEdge<&'a Node<'a, T>>>,
        }

        impl<'a, T: 'a> fmt::Debug for $name<'a, RefCell<T>>
        where
            T: fmt::Debug,
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("root", &self.root)
                    .field("next", &self.next)
                    .finish()
            }
        }

        impl<'a, T> Iterator for $name<'a, T> {
            type Item = NodeEdge<&'a Node<'a, T>>;

            fn next(&mut self) -> Option<NodeEdge<&'a Node<'a, T>>> {
                match self.next.take() {
                    Some(item) => {
                        self.next = match item {
                            NodeEdge::Start(node) => match node.$first_child.get() {
                                Some(child) => Some(NodeEdge::Start(child)),
                                None => Some(NodeEdge::End(node)),
                            },
                            NodeEdge::End(node) => {
                                if node.same_node(self.root) {
                                    None
                                } else {
                                    match node.$next_sibling.get() {
                                        Some(sibling) => Some(NodeEdge::Start(sibling)),
                                        None => match node.parent.get() {
                                            Some(parent) => Some(NodeEdge::End(parent)),
                                            None => panic!("tree modified during iteration"),
                                        },
                                    }
                                }
                            }
                        };
                        Some(item)
                    }
                    None => None,
                }
            }
        }
    };
}

traverse_iterator! {
    #[doc = "An iterator of the start and end edges of a given
    node and its descendants, in tree order."]
    Traverse: first_child, next_sibling
}

traverse_iterator! {
    #[doc = "An iterator of the start and end edges of a given
    node and its descendants, in reverse tree order."]
    ReverseTraverse: last_child, previous_sibling
}

#[test]
fn it_works() {
    struct DropTracker<'a>(&'a Cell<u32>);
    impl Drop for DropTracker<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let drop_counter = Cell::new(0);
    {
        let mut new_counter = 0;
        let arena = typed_arena::Arena::new();
        let mut new = || {
            new_counter += 1;
            arena.alloc(Node::new((new_counter, DropTracker(&drop_counter))))
        };

        let a = new(); // 1
        a.append(new()); // 2
        a.append(new()); // 3
        a.prepend(new()); // 4
        let b = new(); // 5
        b.append(a);
        a.insert_before(new()); // 6
        a.insert_before(new()); // 7
        a.insert_after(new()); // 8
        a.insert_after(new()); // 9
        let c = new(); // 10
        b.append(c);

        assert_eq!(drop_counter.get(), 0);
        c.previous_sibling.get().unwrap().detach();
        assert_eq!(drop_counter.get(), 0);

        assert_eq!(
            b.descendants().map(|node| node.data.0).collect::<Vec<_>>(),
            [5, 6, 7, 1, 4, 2, 3, 9, 10]
        );
    }

    assert_eq!(drop_counter.get(), 10);
}

impl<T> Node<'_, RefCell<T>> {
    /// Shorthand for `node.data.borrow()`.
    pub fn data(&self) -> Ref<'_, T> {
        self.data.borrow()
    }

    /// Shorthand for `node.data.try_borrow()`.
    pub fn try_data(&self) -> Result<Ref<'_, T>, BorrowError> {
        self.data.try_borrow()
    }

    /// Shorthand for `node.data.borrow_mut()`.
    pub fn data_mut(&self) -> RefMut<'_, T> {
        self.data.borrow_mut()
    }
}
