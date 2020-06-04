use std::fmt::Debug;

pub trait Command<'a> {
    type Context;

    fn execute(&mut self, context: &mut Self::Context);
    fn revert(&mut self, context: &mut Self::Context);
    fn finalize(&mut self, _: &mut Self::Context) {}
}

pub struct CommandStack<C> {
    commands: Vec<C>,
    top: Option<usize>,
}

impl<C> CommandStack<C> {
    pub fn new() -> Self {
        Self {
            commands: Default::default(),
            top: None,
        }
    }

    pub fn do_command<'a, Ctx>(&mut self, mut command: C, mut context: Ctx)
        where C: Command<'a, Context=Ctx> + Debug {
        if self.commands.is_empty() {
            self.top = Some(0);
        } else {
            // Advance top
            match self.top.as_mut() {
                None => self.top = Some(0),
                Some(top) => *top += 1,
            }
            // Drop everything after top.
            let top = self.top.unwrap_or(0);
            if top < self.commands.len() {
                for mut dropped_command in self.commands.drain(top..) {
                    println!("Finalizing command {:?}", dropped_command);
                    dropped_command.finalize(&mut context);
                }
            }
        }

        println!("Executing command {:?}", command);

        command.execute(&mut context);

        self.commands.push(command);
    }

    pub fn undo<'a, Ctx>(&mut self, mut context: Ctx)
        where C: Command<'a, Context=Ctx> + Debug {
        if !self.commands.is_empty() {
            if let Some(top) = self.top.as_mut() {
                if let Some(command) = self.commands.get_mut(*top) {
                    println!("Undo command {:?}", command);
                    command.revert(&mut context)
                }
                if *top == 0 {
                    self.top = None;
                } else {
                    *top -= 1;
                }
            }
        }
    }

    pub fn redo<'a, Ctx>(&mut self, mut context: Ctx)
        where C: Command<'a, Context=Ctx> + Debug {
        if !self.commands.is_empty() {
            let command = match self.top.as_mut() {
                None => {
                    self.top = Some(1);
                    self.commands.first_mut()
                }
                Some(top) => {
                    let last = self.commands.len();
                    if *top < last {
                        let command = self.commands.get_mut(*top);
                        *top += 1;
                        command
                    } else {
                        None
                    }
                }
            };

            if let Some(command) = command {
                println!("Redo command {:?}", command);
                command.execute(&mut context)
            }
        }
    }
}