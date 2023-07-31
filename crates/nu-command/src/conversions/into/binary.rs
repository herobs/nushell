use nu_cmd_base::input_handler::{operate, CmdArgument};
use nu_engine::CallExt;
use nu_protocol::{
    ast::{Call, CellPath},
    engine::{Command, EngineState, Stack},
    Category, Example, IntoPipelineData, PipelineData, ShellError, Signature, Span, SyntaxShape,
    Type, Value,
};

pub struct Arguments {
    cell_paths: Option<Vec<CellPath>>,
    compact: bool,
}

impl CmdArgument for Arguments {
    fn take_cell_paths(&mut self) -> Option<Vec<CellPath>> {
        self.cell_paths.take()
    }
}

#[derive(Clone)]
pub struct SubCommand;

impl Command for SubCommand {
    fn name(&self) -> &str {
        "into binary"
    }

    fn signature(&self) -> Signature {
        Signature::build("into binary")
            .input_output_types(vec![
                (Type::Binary, Type::Binary),
                (Type::Int, Type::Binary),
                (Type::Number, Type::Binary),
                (Type::String, Type::Binary),
                (Type::Bool, Type::Binary),
                (Type::Filesize, Type::Binary),
                (Type::Date, Type::Binary),
                (Type::Table(vec![]), Type::Table(vec![])),
                (Type::Record(vec![]), Type::Record(vec![])),
            ])
            .allow_variants_without_examples(true) // TODO: supply exhaustive examples
            .switch("compact", "output without padding zeros", None)
            .rest(
                "rest",
                SyntaxShape::CellPath,
                "for a data structure input, convert data at the given cell paths",
            )
            .category(Category::Conversions)
    }

    fn usage(&self) -> &str {
        "Convert value to a binary primitive."
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["convert", "bytes"]
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        into_binary(engine_state, stack, call, input)
    }

    fn examples(&self) -> Vec<Example> {
        vec![
            Example {
                description: "convert string to a nushell binary primitive",
                example: "'This is a string that is exactly 52 characters long.' | into binary",
                result: Some(Value::Binary {
                    val: "This is a string that is exactly 52 characters long."
                        .to_string()
                        .as_bytes()
                        .to_vec(),
                    span: Span::test_data(),
                }),
            },
            Example {
                description: "convert a number to a nushell binary primitive",
                example: "1 | into binary",
                result: Some(Value::Binary {
                    val: i64::from(1).to_ne_bytes().to_vec(),
                    span: Span::test_data(),
                }),
            },
            Example {
                description: "convert a boolean to a nushell binary primitive",
                example: "true | into binary",
                result: Some(Value::Binary {
                    val: i64::from(1).to_ne_bytes().to_vec(),
                    span: Span::test_data(),
                }),
            },
            Example {
                description: "convert a filesize to a nushell binary primitive",
                example: "ls | where name == LICENSE | get size | into binary",
                result: None,
            },
            Example {
                description: "convert a filepath to a nushell binary primitive",
                example: "ls | where name == LICENSE | get name | path expand | into binary",
                result: None,
            },
            Example {
                description: "convert a decimal to a nushell binary primitive",
                example: "1.234 | into binary",
                result: Some(Value::Binary {
                    val: 1.234f64.to_ne_bytes().to_vec(),
                    span: Span::test_data(),
                }),
            },
            Example {
                description:
                    "convert an integer to a nushell binary primitive with compact enabled",
                example: "10 | into binary --compact",
                result: Some(Value::Binary {
                    val: vec![10],
                    span: Span::test_data(),
                }),
            },
        ]
    }
}

fn into_binary(
    engine_state: &EngineState,
    stack: &mut Stack,
    call: &Call,
    input: PipelineData,
) -> Result<PipelineData, ShellError> {
    let head = call.head;
    let cell_paths = call.rest(engine_state, stack, 0)?;
    let cell_paths = (!cell_paths.is_empty()).then_some(cell_paths);

    match input {
        PipelineData::ExternalStream { stdout: None, .. } => Ok(Value::Binary {
            val: vec![],
            span: head,
        }
        .into_pipeline_data()),
        PipelineData::ExternalStream {
            stdout: Some(stream),
            ..
        } => {
            // TODO: in the future, we may want this to stream out, converting each to bytes
            let output = stream.into_bytes()?;
            Ok(Value::Binary {
                val: output.item,
                span: head,
            }
            .into_pipeline_data())
        }
        _ => {
            let args = Arguments {
                cell_paths,
                compact: call.has_flag("compact"),
            };
            operate(action, args, input, call.head, engine_state.ctrlc.clone())
        }
    }
}

pub fn action(input: &Value, _args: &Arguments, span: Span) -> Value {
    let value = match input {
        Value::Binary { .. } => input.clone(),
        Value::Int { val, .. } => Value::Binary {
            val: val.to_ne_bytes().to_vec(),
            span,
        },
        Value::Float { val, .. } => Value::Binary {
            val: val.to_ne_bytes().to_vec(),
            span,
        },
        Value::Filesize { val, .. } => Value::Binary {
            val: val.to_ne_bytes().to_vec(),
            span,
        },
        Value::String { val, .. } => Value::Binary {
            val: val.as_bytes().to_vec(),
            span,
        },
        Value::Bool { val, .. } => Value::Binary {
            val: i64::from(*val).to_ne_bytes().to_vec(),
            span,
        },
        Value::Duration { val, .. } => Value::Binary {
            val: val.to_ne_bytes().to_vec(),
            span,
        },
        Value::Date { val, .. } => Value::Binary {
            val: val.format("%c").to_string().as_bytes().to_vec(),
            span,
        },
        // Propagate errors by explicitly matching them before the final case.
        Value::Error { .. } => input.clone(),
        other => Value::Error {
            error: Box::new(ShellError::OnlySupportsThisInputType {
                exp_input_type: "integer, float, filesize, string, date, duration, binary or bool"
                    .into(),
                wrong_type: other.get_type().to_string(),
                dst_span: span,
                src_span: other.expect_span(),
            }),
        },
    };

    if _args.compact {
        if let Value::Binary { val, span } = value {
            let val = if cfg!(target_endian = "little") {
                match val.iter().rposition(|&x| x != 0) {
                    Some(idx) => &val[..idx + 1],
                    None => &val,
                }
            } else {
                match val.iter().position(|&x| x != 0) {
                    Some(idx) => &val[idx..],
                    None => &val,
                }
            };

            Value::Binary {
                val: val.to_vec(),
                span,
            }
        } else {
            value
        }
    } else {
        value
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::*;

    #[test]
    fn test_examples() {
        use crate::test_examples;

        test_examples(SubCommand {})
    }

    #[rstest]
    #[case(vec![10], vec![10])]
    #[case(vec![10, 0, 0], vec![10])]
    #[case(vec![0, 10, 0, 0], vec![0, 10])]
    fn test_compact(
        #[case] input: Vec<u8>,
        #[case] output: Vec<u8>,
    ) {
        let s = Value::test_binary(input);
        let actual = action(
            &s,
            &Arguments {
                cell_paths: None,
                compact: true,
            },
            Span::test_data(),
        );
        assert_eq!(actual, Value::test_binary(output));
    }
}
