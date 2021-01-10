use crate::command_args::CommandArgs;
use crate::documentation::get_help;
use crate::evaluate::block::run_block;
use crate::evaluation_context::EvaluationContext;
use crate::example::Example;
use async_trait::async_trait;
use nu_errors::ShellError;
use nu_parser::ParserScope;
use nu_protocol::hir::Block;
use nu_protocol::{ReturnSuccess, Signature, UntaggedValue};
use nu_source::{b, DebugDocBuilder, PrettyDebugWithSource, Tag};
use nu_stream::{OutputStream, ToOutputStream};
use std::sync::Arc;

#[async_trait]
pub trait WholeStreamCommand: Send + Sync {
    fn name(&self) -> &str;

    fn signature(&self) -> Signature {
        Signature::new(self.name()).desc(self.usage()).filter()
    }

    fn usage(&self) -> &str;

    async fn run(&self, args: CommandArgs) -> Result<OutputStream, ShellError>;

    fn is_binary(&self) -> bool {
        false
    }

    // Commands that are not meant to be run by users
    fn is_internal(&self) -> bool {
        false
    }

    fn examples(&self) -> Vec<Example> {
        Vec::new()
    }
}

// Custom commands are blocks, so we can use the information in the block to also
// implement a WholeStreamCommand
#[allow(clippy::suspicious_else_formatting)]
#[async_trait]
impl WholeStreamCommand for Block {
    fn name(&self) -> &str {
        &self.params.name
    }

    fn signature(&self) -> Signature {
        self.params.clone()
    }

    fn usage(&self) -> &str {
        &self.params.usage
    }

    async fn run(&self, args: CommandArgs) -> Result<OutputStream, ShellError> {
        let call_info = args.call_info.clone();

        let mut block = self.clone();
        block.set_redirect(call_info.args.external_redirection);

        let ctx = EvaluationContext::from_args(&args);
        let evaluated = call_info.evaluate(&ctx).await?;

        let input = args.input;
        ctx.scope.enter_scope();
        if let Some(args) = evaluated.args.positional {
            // FIXME: do not do this
            for arg in args.into_iter().zip(self.params.positional.iter()) {
                let name = arg.1 .0.name();

                if name.starts_with('$') {
                    ctx.scope.add_var(name, arg.0);
                } else {
                    ctx.scope.add_var(format!("${}", name), arg.0);
                }
            }
        }
        if let Some(args) = evaluated.args.named {
            for named in &block.params.named {
                let name = named.0;
                if let Some(value) = args.get(name) {
                    if name.starts_with('$') {
                        ctx.scope.add_var(name, value.clone());
                    } else {
                        ctx.scope.add_var(format!("${}", name), value.clone());
                    }
                } else if name.starts_with('$') {
                    ctx.scope
                        .add_var(name, UntaggedValue::nothing().into_untagged_value());
                } else {
                    ctx.scope.add_var(
                        format!("${}", name),
                        UntaggedValue::nothing().into_untagged_value(),
                    );
                }
            }
        } else {
            for named in &block.params.named {
                let name = named.0;
                if name.starts_with('$') {
                    ctx.scope
                        .add_var(name, UntaggedValue::nothing().into_untagged_value());
                } else {
                    ctx.scope.add_var(
                        format!("${}", name),
                        UntaggedValue::nothing().into_untagged_value(),
                    );
                }
            }
        }
        let result = run_block(&block, &ctx, input).await;
        ctx.scope.exit_scope();
        result.map(|x| x.to_output_stream())
    }

    fn is_binary(&self) -> bool {
        false
    }

    fn is_internal(&self) -> bool {
        false
    }

    fn examples(&self) -> Vec<Example> {
        vec![]
    }
}

#[derive(Clone)]
pub struct Command(Arc<dyn WholeStreamCommand>);

impl PrettyDebugWithSource for Command {
    fn pretty_debug(&self, source: &str) -> DebugDocBuilder {
        b::typed(
            "whole stream command",
            b::description(self.name())
                + b::space()
                + b::equals()
                + b::space()
                + self.signature().pretty_debug(source),
        )
    }
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Command({})", self.name())
    }
}

impl Command {
    pub fn name(&self) -> &str {
        self.0.name()
    }

    pub fn signature(&self) -> Signature {
        self.0.signature()
    }

    pub fn usage(&self) -> &str {
        self.0.usage()
    }

    pub fn examples(&self) -> Vec<Example> {
        self.0.examples()
    }

    pub async fn run(&self, args: CommandArgs) -> Result<OutputStream, ShellError> {
        if args.call_info.switch_present("help") {
            let cl = self.0.clone();
            Ok(OutputStream::one(Ok(ReturnSuccess::Value(
                UntaggedValue::string(get_help(&*cl, &args.scope)).into_value(Tag::unknown()),
            ))))
        } else {
            self.0.run(args).await
        }
    }

    pub fn is_binary(&self) -> bool {
        self.0.is_binary()
    }

    pub fn is_internal(&self) -> bool {
        self.0.is_internal()
    }

    pub fn stream_command(&self) -> &dyn WholeStreamCommand {
        &*self.0
    }
}

pub fn whole_stream_command(command: impl WholeStreamCommand + 'static) -> Command {
    Command(Arc::new(command))
}
