/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use dom::bindings::callback::RethrowExceptions;
use dom::bindings::codegen::Bindings::NodeIteratorBinding;
use dom::bindings::codegen::Bindings::NodeIteratorBinding::NodeIteratorMethods;
use dom::bindings::codegen::Bindings::NodeBinding::NodeMethods;
use dom::bindings::codegen::Bindings::NodeFilterBinding::NodeFilter;
// FIXME: Uncomment when codegen fix allows NodeFilterConstants
// to move to the NodeFilter binding file (#3149).
// For now, it is defined in treewalker.rs.
use dom::treewalker::NodeFilterConstants;
use dom::treewalker::{Filter,FilterNone,FilterJS,FilterNative};
use dom::bindings::error::Fallible;
use dom::bindings::global::Window;
use dom::bindings::js::{JS, JSRef, OptionalRootable, Temporary};
use dom::bindings::utils::{Reflectable, Reflector, reflect_dom_object};
use dom::document::Document;
use dom::node::{Node, NodeHelpers};

use std::cell::Cell;

// XXX
// "Each NodeIterator object has an associated iterator collection,
//  which is a collection rooted at root, whose filter matches any node."

// XXX implement the "removing steps"

// http://dom.spec.whatwg.org/#nodeiterator
#[jstraceable]
#[must_root]
pub struct NodeIterator {
    pub reflector_: Reflector,
    pub root_node: JS<Node>,
    pub reference_node: Cell<JS<Node>>,
    pub pointer_before_reference_node: Cell<bool>,
    pub what_to_show: u32,
    pub filter: Filter
}

impl NodeIterator {
    pub fn new_inherited(root_node: JSRef<Node>,
                         what_to_show: u32,
                         filter: Filter) -> NodeIterator {
        NodeIterator {
            reflector_: Reflector::new(),
            root_node: JS::from_rooted(root_node),
            reference_node: Cell::new(JS::from_rooted(root_node)),
            pointer_before_reference_node: Cell::new(true),
            what_to_show: what_to_show,
            filter: filter
        }
    }

    pub fn new_with_filter(document: JSRef<Document>,
                           root_node: JSRef<Node>,
                           what_to_show: u32,
                           filter: Filter) -> Temporary<NodeIterator> {
        let window = document.window.root();
        reflect_dom_object(box NodeIterator::new_inherited(root_node, what_to_show, filter),
                           &Window(*window),
                           NodeIteratorBinding::Wrap)
    }

    pub fn new(document: JSRef<Document>,
               root_node: JSRef<Node>,
               what_to_show: u32,
               node_filter: Option<NodeFilter>) -> Temporary<NodeIterator> {
        let filter = match node_filter {
            None => FilterNone,
            Some(jsfilter) => FilterJS(jsfilter)
        };
        NodeIterator::new_with_filter(document, root_node, what_to_show, filter)
    }
}

impl<'a> NodeIteratorMethods for JSRef<'a, NodeIterator> {
    fn Root(self) -> Temporary<Node> {
        Temporary::new(self.root_node)
    }

    fn WhatToShow(self) -> u32 {
        self.what_to_show
    }

    fn GetFilter(self) -> Option<NodeFilter> {
        match self.filter {
            FilterNone => None,
            FilterJS(nf) => Some(nf),
            FilterNative(_) => fail!("Cannot convert native node filter to DOM NodeFilter")
        }
    }

    fn GetReferenceNode(self) -> Option<Temporary<Node>> {
        Some(Temporary::new(self.reference_node.get()))
    }

    fn PointerBeforeReferenceNode(self) -> bool {
        self.pointer_before_reference_node.get()
    }

    fn PreviousNode(self) -> Fallible<Option<Temporary<Node>>> {
        self.prev_node()
    }

    fn NextNode(self) -> Fallible<Option<Temporary<Node>>> {
        self.next_node()
    }

    fn Detach(self) {
        // "The detach() method must do nothing."
    }
}

impl Reflectable for NodeIterator {
    fn reflector<'a>(&'a self) -> &'a Reflector {
        &self.reflector_
    }
}

trait PrivateNodeIteratorHelpers<'a> {
    fn following(self, node: JSRef<Node>) -> Option<Temporary<Node>>;
    fn preceding(self, node: JSRef<Node>) -> Option<Temporary<Node>>;
    fn traverse(self, direction: Direction) -> Fallible<Option<Temporary<Node>>>;
    fn is_root_node(self, node: JSRef<'a, Node>) -> bool;
    fn accept_node(self, node: JSRef<'a, Node>) -> Fallible<u16>;
}

enum Direction {
    Next,
    Previous
}

impl<'a> PrivateNodeIteratorHelpers<'a> for JSRef<'a, NodeIterator> {
    fn following(self, node: JSRef<Node>) -> Option<Temporary<Node>> {
        match node.first_child() {
            None => match node.next_sibling() {
                None => {
                    let mut candidate = node;
                    while !self.is_root_node(candidate) && candidate.next_sibling().is_none() {
                        match candidate.parent_node() {
                            None =>
                                // XXX can this happen in NodeIterator? Can dom modifications cause this?
                                return None,
                            Some(n) => candidate = n.root().clone()
                        }
                    }
                    if self.is_root_node(candidate) {
                        None
                    } else {
                        candidate.next_sibling()
                    }
                },
                it => it
            },
            it => it
        }
    }

    fn preceding(self, node: JSRef<Node>) -> Option<Temporary<Node>> {
        if self.is_root_node(node) {
            None
        } else {
            match node.prev_sibling() {
                Some(sibling) => {
                    let mut node = sibling.root().clone();
                    while node.first_child().is_some() {
                        node = node.last_child().unwrap().root().clone()
                    }
                    Some(Temporary::from_rooted(node))
                },
                None => node.parent_node()
            }
        }
    }

    // http://dom.spec.whatwg.org/#concept-nodeiterator-traverse
    fn traverse(self, direction: Direction) -> Fallible<Option<Temporary<Node>>> {
        // To traverse in direction direction run these steps:
        // Let node be the value of the referenceNode attribute.
        let mut node = self.reference_node.get().root().clone();
        // Let before node be the value of the pointerBeforeReferenceNode attribute.
        let mut before_node = self.pointer_before_reference_node.get();
        // Run these substeps:
        loop {
            match direction {
                // If direction is next
                Next => match before_node {
                    // If before node is false,
                    false => match self.following(node) {
                        // let node be the first node following node in the iterator collection.
                        Some(n) => node = n.root().clone(),
                        // If there is no such node return null.
                        None => return Ok(None)
                    },
                    // If before node is true, set it to false.
                    true => before_node = false
                },
                // If direction is previous
                Previous => match before_node {
                    // If before node is true,
                    true => match self.preceding(node) {
                        // let node be the first node preceding node in the iterator collection.
                        Some(n) => node = n.root().clone(),
                        // If there is no such node return null.
                        None => return Ok(None)
                    },
                    // If before node is false, set it to true.
                    false => before_node = true
                }
            }
            // Filter node and let result be the return value.
            match self.accept_node(node) {
                Err(e) => return Err(e),
                // If result is FILTER_ACCEPT, go to the next step in the overall set of steps.
                Ok(NodeFilterConstants::FILTER_ACCEPT) => break,
                // Otherwise, run these substeps again.
                _ => {}
            }
        }
        // Set the referenceNode attribute to node,
        self.reference_node.set(JS::from_rooted(node));
        // set the pointerBeforeReferenceNode attribute to before node,
        self.pointer_before_reference_node.set(before_node);
        // and return node.
        Ok(Some(Temporary::from_rooted(node)))
    }

    // http://dom.spec.whatwg.org/#concept-node-filter
    fn accept_node(self, node: JSRef<'a, Node>) -> Fallible<u16> {
        // "To filter node run these steps:"
        // "1. Let n be node's nodeType attribute value minus 1."
        let n: uint = node.NodeType() as uint - 1;
        // "2. If the nth bit (where 0 is the least significant bit) of whatToShow is not set,
        //     return FILTER_SKIP."
        if (self.what_to_show & (1 << n)) == 0 {
            return Ok(NodeFilterConstants::FILTER_SKIP)
        }
        // "3. If filter is null, return FILTER_ACCEPT."
        // "4. Let result be the return value of invoking filter."
        // "5. If an exception was thrown, re-throw the exception."
        // "6. Return result."
        match self.filter {
            FilterNone => Ok(NodeFilterConstants::FILTER_ACCEPT),
            FilterNative(f) => Ok((*f)(node)),
            FilterJS(callback) => callback.AcceptNode_(self, node, RethrowExceptions)
        }
    }

    fn is_root_node(self, node: JSRef<'a, Node>) -> bool {
        JS::from_rooted(node) == self.root_node
    }
}

pub trait NodeIteratorHelpers<'a> {
    fn next_node(self) -> Fallible<Option<Temporary<Node>>>;
    fn prev_node(self) -> Fallible<Option<Temporary<Node>>>;
}

impl<'a> NodeIteratorHelpers<'a> for JSRef<'a, NodeIterator> {
    // http://dom.spec.whatwg.org/#dom-nodeiterator-nextnode
    fn next_node(self) -> Fallible<Option<Temporary<Node>>> {
        // "The nextNode() method must traverse in direction next."
        self.traverse(Next)
    }

    // http://dom.spec.whatwg.org/#dom-nodeiterator-previousnode
    fn prev_node(self) -> Fallible<Option<Temporary<Node>>> {
        // "The previousNode() method must traverse in direction previous."
        self.traverse(Previous)
    }
}

impl<'a> Iterator<JSRef<'a, Node>> for JSRef<'a, NodeIterator> {
   fn next(&mut self) -> Option<JSRef<'a, Node>> {
       match self.next_node() {
           Ok(node) => node.map(|n| n.root().clone()),
           Err(_) =>
               // The Err path happens only when a JavaScript
               // NodeFilter throws an exception. This iterator
               // is meant for internal use from Rust code, which
               // will probably be using a native Rust filter,
               // which cannot produce an Err result.
               unreachable!()
       }
   }
}
