#![warn(missing_docs)]

//! Everything related to AI behavior and behavior trees.
//!
//! Behavior trees are simple but very powerful mechanism to implement artificial intelligence for
//! games. The main concept is in its name. Tree is a set of connected nodes, where each node could
//! have single parent and zero or more children nodes. Execution path of the tree is defined by the
//! actions of the nodes. Behavior tree has a set of hard coded nodes as well as leaf nodes with
//! user-defined logic. Hard coded nodes are: Sequence, Selector, Leaf. Leaf is special - it has
//! custom method `tick` that can contain any logic you want.
//!
//! For more info see:
//! - [Wikipedia article](https://en.wikipedia.org/wiki/Behavior_tree_(artificial_intelligence,_robotics_and_control)
//! - [Gamasutra](https://www.gamasutra.com/blogs/ChrisSimpson/20140717/221339/Behavior_trees_for_AI_How_they_work.php)

use crate::{
    core::{
        pool::{Handle, Pool},
        visitor::prelude::*,
    },
    utils::behavior::{
        composite::{CompositeNode, CompositeNodeKind},
        leaf::LeafNode,
    },
};
use std::fmt::Debug;
use std::ops::{Index, IndexMut};

pub mod composite;
pub mod leaf;

/// Status of execution of behavior tree node.
pub enum Status {
    /// Action was successful.
    Success,
    /// Failed to perform an action.
    Failure,
    /// Need another iteration to perform an action.
    Running,
}

/// A trait for user-defined actions for behavior tree.
pub trait Behavior<'a>: Visit + Default + PartialEq + Debug {
    /// A context in which the behavior will be performed.
    type Context;

    /// A function that will be called each frame depending on
    /// the current execution path of the behavior tree it belongs
    /// to.
    fn tick(&mut self, context: &mut Self::Context) -> Status;
}

/// Root node of the tree.
#[derive(Debug, PartialEq, Visit)]
pub struct RootNode<B> {
    child: Handle<BehaviorNode<B>>,
}

impl<B> Default for RootNode<B> {
    fn default() -> Self {
        Self {
            child: Default::default(),
        }
    }
}

/// Possible variations of behavior nodes.
#[derive(Debug, PartialEq, Visit)]
pub enum BehaviorNode<B> {
    #[doc(hidden)]
    Unknown,
    /// Root node of the tree.
    Root(RootNode<B>),
    /// Composite (sequence or selector) node of the tree.
    Composite(CompositeNode<B>),
    /// A node with custom logic.
    Leaf(LeafNode<B>),
}

impl<B> Default for BehaviorNode<B> {
    fn default() -> Self {
        Self::Unknown
    }
}

/// See module docs.
#[derive(Debug, PartialEq, Visit)]
pub struct BehaviorTree<B> {
    nodes: Pool<BehaviorNode<B>>,
    root: Handle<BehaviorNode<B>>,
}

impl<B> Default for BehaviorTree<B> {
    fn default() -> Self {
        Self {
            nodes: Default::default(),
            root: Default::default(),
        }
    }
}

impl<B> BehaviorTree<B> {
    /// Creates new behavior tree with single root node.
    pub fn new() -> Self {
        let mut nodes = Pool::new();
        let root = nodes.spawn(BehaviorNode::Root(RootNode {
            child: Default::default(),
        }));
        Self { nodes, root }
    }

    /// Adds a node to the tree, returns its handle.
    pub fn add_node(&mut self, node: BehaviorNode<B>) -> Handle<BehaviorNode<B>> {
        self.nodes.spawn(node)
    }

    /// Sets entry nodes of the tree. Execution will start from this node.
    pub fn set_entry_node(&mut self, entry: Handle<BehaviorNode<B>>) {
        if let BehaviorNode::Root(root) = &mut self.nodes[self.root] {
            root.child = entry;
        } else {
            unreachable!("must be root")
        }
    }

    fn tick_recursive<'a, Ctx>(&self, handle: Handle<BehaviorNode<B>>, context: &mut Ctx) -> Status
    where
        B: Behavior<'a, Context = Ctx>,
    {
        match self.nodes[handle] {
            BehaviorNode::Root(ref root) => {
                if root.child.is_some() {
                    self.tick_recursive(root.child, context)
                } else {
                    Status::Success
                }
            }
            BehaviorNode::Composite(ref composite) => match composite.kind {
                CompositeNodeKind::Sequence => {
                    let mut all_succeeded = true;
                    for child in composite.children.iter() {
                        match self.tick_recursive(*child, context) {
                            Status::Failure => {
                                all_succeeded = false;
                                break;
                            }
                            Status::Running => {
                                return Status::Running;
                            }
                            _ => (),
                        }
                    }
                    if all_succeeded {
                        Status::Success
                    } else {
                        Status::Failure
                    }
                }
                CompositeNodeKind::Selector => {
                    for child in composite.children.iter() {
                        match self.tick_recursive(*child, context) {
                            Status::Success => return Status::Success,
                            Status::Running => return Status::Running,
                            _ => (),
                        }
                    }
                    Status::Failure
                }
            },
            BehaviorNode::Leaf(ref leaf) => {
                leaf.behavior.as_ref().unwrap().borrow_mut().tick(context)
            }
            BehaviorNode::Unknown => {
                unreachable!()
            }
        }
    }

    /// Tries to get a shared reference to a node by given handle.
    pub fn node(&self, handle: Handle<BehaviorNode<B>>) -> Option<&BehaviorNode<B>> {
        self.nodes.try_borrow(handle)
    }

    /// Tries to get a mutable reference to a node by given handle.
    pub fn node_mut(&mut self, handle: Handle<BehaviorNode<B>>) -> Option<&mut BehaviorNode<B>> {
        self.nodes.try_borrow_mut(handle)
    }

    /// Performs a single update tick with given context.
    pub fn tick<'a, Ctx>(&self, context: &mut Ctx) -> Status
    where
        B: Behavior<'a, Context = Ctx>,
    {
        self.tick_recursive(self.root, context)
    }
}

impl<B> Index<Handle<BehaviorNode<B>>> for BehaviorTree<B> {
    type Output = BehaviorNode<B>;

    fn index(&self, index: Handle<BehaviorNode<B>>) -> &Self::Output {
        &self.nodes[index]
    }
}

impl<B> IndexMut<Handle<BehaviorNode<B>>> for BehaviorTree<B> {
    fn index_mut(&mut self, index: Handle<BehaviorNode<B>>) -> &mut Self::Output {
        &mut self.nodes[index]
    }
}

#[cfg(test)]
mod test {
    use crate::{
        core::{futures::executor::block_on, visitor::prelude::*},
        utils::behavior::{
            composite::{CompositeNode, CompositeNodeKind},
            leaf::LeafNode,
            Behavior, BehaviorTree, Status,
        },
    };
    use std::{env, fs::File, io::Write, path::PathBuf};

    #[derive(Debug, PartialEq, Default, Visit)]
    struct WalkAction;

    impl<'a> Behavior<'a> for WalkAction {
        type Context = Environment;

        fn tick(&mut self, context: &mut Self::Context) -> Status {
            if context.distance_to_door <= 0.0 {
                Status::Success
            } else {
                context.distance_to_door -= 0.1;
                println!(
                    "Approaching door, remaining distance: {}",
                    context.distance_to_door
                );
                Status::Running
            }
        }
    }

    #[derive(Debug, PartialEq, Default, Visit)]
    struct OpenDoorAction;

    impl<'a> Behavior<'a> for OpenDoorAction {
        type Context = Environment;

        fn tick(&mut self, context: &mut Self::Context) -> Status {
            if !context.door_opened {
                context.door_opened = true;
                println!("Door was opened!");
            }
            Status::Success
        }
    }

    #[derive(Debug, PartialEq, Default, Visit)]
    struct StepThroughAction;

    impl<'a> Behavior<'a> for StepThroughAction {
        type Context = Environment;

        fn tick(&mut self, context: &mut Self::Context) -> Status {
            if context.distance_to_door < -1.0 {
                Status::Success
            } else {
                context.distance_to_door -= 0.1;
                println!(
                    "Stepping through doorway, remaining distance: {}",
                    -1.0 - context.distance_to_door
                );
                Status::Running
            }
        }
    }

    #[derive(Debug, PartialEq, Default, Visit)]
    struct CloseDoorAction;

    impl<'a> Behavior<'a> for CloseDoorAction {
        type Context = Environment;

        fn tick(&mut self, context: &mut Self::Context) -> Status {
            if context.door_opened {
                context.door_opened = false;
                context.done = true;
                println!("Door was closed");
            }
            Status::Success
        }
    }

    #[derive(Debug, PartialEq, Visit)]
    enum BotBehavior {
        None,
        Walk(WalkAction),
        OpenDoor(OpenDoorAction),
        StepThrough(StepThroughAction),
        CloseDoor(CloseDoorAction),
    }

    impl Default for BotBehavior {
        fn default() -> Self {
            Self::None
        }
    }

    #[derive(Default, Visit)]
    struct Environment {
        // > 0 - door in front of
        // < 0 - door is behind
        distance_to_door: f32,
        door_opened: bool,
        done: bool,
    }

    impl<'a> Behavior<'a> for BotBehavior {
        type Context = Environment;

        fn tick(&mut self, context: &mut Self::Context) -> Status {
            match self {
                BotBehavior::None => unreachable!(),
                BotBehavior::Walk(v) => v.tick(context),
                BotBehavior::OpenDoor(v) => v.tick(context),
                BotBehavior::StepThrough(v) => v.tick(context),
                BotBehavior::CloseDoor(v) => v.tick(context),
            }
        }
    }

    fn create_tree() -> BehaviorTree<BotBehavior> {
        let mut tree = BehaviorTree::new();

        let entry = CompositeNode::new(
            CompositeNodeKind::Sequence,
            vec![
                LeafNode::new(BotBehavior::Walk(WalkAction)).add(&mut tree),
                LeafNode::new(BotBehavior::OpenDoor(OpenDoorAction)).add(&mut tree),
                LeafNode::new(BotBehavior::StepThrough(StepThroughAction)).add(&mut tree),
                LeafNode::new(BotBehavior::CloseDoor(CloseDoorAction)).add(&mut tree),
            ],
        )
        .add(&mut tree);

        tree.set_entry_node(entry);

        tree
    }

    #[test]
    fn test_behavior() {
        let tree = create_tree();

        let mut ctx = Environment {
            distance_to_door: 3.0,
            door_opened: false,
            done: false,
        };

        while !ctx.done {
            tree.tick(&mut ctx);
        }
    }

    #[test]
    fn test_behavior_save_load() {
        let (bin, txt) = {
            let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
            let root = PathBuf::from(manifest_dir).join("test_output");
            if !root.exists() {
                std::fs::create_dir(&root).unwrap();
            }
            (
                root.join(format!("{}.bin", "behavior_save_load")),
                root.join(format!("{}.txt", "behavior_save_load")),
            )
        };

        // Save
        let mut saved_tree = create_tree();
        let mut visitor = Visitor::new();
        saved_tree.visit("Tree", &mut visitor).unwrap();
        visitor.save_binary(bin.clone()).unwrap();
        let mut file = File::create(&txt).unwrap();
        file.write(visitor.save_text().as_bytes()).unwrap();

        // Load
        let mut visitor = block_on(Visitor::load_binary(bin)).unwrap();
        let mut loaded_tree = BehaviorTree::<BotBehavior>::default();
        loaded_tree.visit("Tree", &mut visitor).unwrap();

        assert_eq!(saved_tree, loaded_tree);
    }
}