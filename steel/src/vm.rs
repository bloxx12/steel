mod arity;
mod constants;
mod expand;
mod map;

pub use arity::Arity;
pub use arity::ArityMap;
pub use constants::ConstantMap;
pub use constants::ConstantTable;
pub use expand::expand;
pub use expand::extract_macro_definitions;
pub use map::SymbolMap;

// use expand::is_definition;

// pub enum ByteCode {}
// use std::cell::RefCell;
// use std::convert::TryFrom;
use std::iter::Iterator;
// use std::rc::Rc;
use std::result;

// use crate::env::{Env, FALSE, TRUE, VOID};
// use crate::parser::lexer::Tokenizer;
// use crate::parser::lexer::TokenStream;
// use crate::parser::tokens::Token;
// use crate::parser::tokens::TokenError;
use crate::parser::tokens::TokenType;
use crate::parser::SyntaxObject;
use crate::parser::{Expr, ParseError, Parser};
// use crate::primitives::ListOperations;
use crate::rerrs::SteelErr;
use crate::rvals::{ByteCodeLambda, Result, SteelVal};
// use crate::stop;
// use crate::structs::SteelStruct;
// use crate::throw;
use std::collections::HashMap;
// use std::ops::Deref;

// use crate::interpreter::evaluator::Evaluator;

use std::ops::Deref;

use crate::env::Env;
use crate::env::FALSE;
use crate::env::TRUE;
use crate::env::VOID;
use std::cell::RefCell;
use std::rc::Rc;

use std::convert::TryFrom;

use crate::parser::span::Span;

use std::time::Instant;

// use std::collections::HashSet;

// use crate::expander::SteelMacro;
// use crate::structs::SteelStruct;

// fn recursive_expand(expr: Expr, )

// use crate::interpreter::evaluator::emit_instructions;

// pass define statement
// identify the handle for the function call
// traverse idenitifying function calls
// if the function call is in the tail position of any of the body, then transform that to be an explicit jump -> __JUMP__
// only need to check the last thing in the body
// pub fn identify_tail_call(expr: &Expr) {}

const STACK_LIMIT: usize = 1024;

pub fn transform_tail_call(instructions: &mut Vec<Instruction>, defining_context: &str) -> bool {
    println!(
        "Calling transform tail call with function: {}",
        defining_context
    );

    let last_idx = instructions.len() - 1;

    // could panic
    let mut indices = vec![last_idx];

    let mut transformed = false;

    for (idx, instruction) in instructions.iter().enumerate() {
        if instruction.op_code == OpCode::JMP && instruction.payload_size == last_idx {
            indices.push(idx);
        }
    }

    for index in &indices {
        if *index < 2 {
            continue;
        }
        let prev_instruction = instructions.get(index - 1);
        let prev_func_push = instructions.get(index - 2);

        match (prev_instruction, prev_func_push) {
            (
                Some(Instruction {
                    op_code: OpCode::FUNC,
                    ..
                }),
                Some(Instruction {
                    op_code: OpCode::PUSH,
                    contents:
                        Some(SyntaxObject {
                            ty: TokenType::Identifier(s),
                            ..
                        }),
                    ..
                }),
            ) => {
                if s == defining_context {
                    println!("Making optimization!");

                    let new_jmp = Instruction::new_jmp(0);
                    // inject tail call jump
                    instructions[index - 2] = new_jmp;
                    instructions[index - 1] = Instruction::new_pass();
                    transformed = true;
                } else {
                    println!("Found function call in tail position")
                }
            }
            _ => {}
        }
    }

    return transformed;
}

// Hopefully this doesn't break anything...
fn count_and_collect_global_defines(exprs: &[Expr], symbol_map: &mut SymbolMap) -> usize {
    let mut count = 0;
    for expr in exprs {
        match expr {
            Expr::Atom(_) => {}
            Expr::VectorVal(list_of_tokens) => {
                match (list_of_tokens.get(0), list_of_tokens.get(1)) {
                    (
                        Some(Expr::Atom(SyntaxObject {
                            ty: TokenType::Identifier(def),
                            ..
                        })),
                        Some(Expr::Atom(SyntaxObject {
                            ty: TokenType::Identifier(name),
                            ..
                        })),
                    ) => {
                        if def == "define" || def == "defn" {
                            // println!("Found definition: {}", name);
                            symbol_map.add(name.as_str());
                            count += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    count
}

// insert fast path for built in functions
// rather than look up function in env, be able to call it directly?
pub fn collect_defines_from_current_scope(
    instructions: &[Instruction],
    symbol_map: &mut SymbolMap,
) -> usize {
    let mut def_stack: usize = 0;
    let mut count = 0;

    for i in 0..instructions.len() {
        match &instructions[i] {
            Instruction {
                op_code: OpCode::SDEF,
                contents:
                    Some(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }),
                ..
            } => {
                if def_stack == 0 {
                    let _ = symbol_map.get_or_add(s);
                    count += 1;
                    // println!("####### FOUND DEFINE #########");
                    // println!("Renaming: {} to index: {}", s, idx);
                    // if let Some(x) = instructions.get_mut(i) {
                    //     x.contents = None;
                    // }
                }

                // def_stack += 1;
            }
            Instruction {
                op_code: OpCode::SCLOSURE,
                ..
            } => {
                def_stack += 1;
            }
            Instruction {
                op_code: OpCode::ECLOSURE,
                ..
            } => {
                if def_stack > 0 {
                    def_stack -= 1;
                }
            }
            _ => {}
        }
    }

    count
}

pub fn collect_binds_from_current_scope(
    instructions: &mut [Instruction],
    symbol_map: &mut SymbolMap,
    start: usize,
    end: usize,
) {
    let mut def_stack: usize = 0;
    for i in start..end {
        match &instructions[i] {
            Instruction {
                op_code: OpCode::BIND,
                contents:
                    Some(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }),
                ..
            } => {
                if def_stack == 1 {
                    let idx = symbol_map.add(s);

                    // println!("Renaming: {} to index: {}", s, idx);
                    if let Some(x) = instructions.get_mut(i) {
                        x.payload_size = idx;
                        x.constant = false;
                    }
                }
            }
            Instruction {
                op_code: OpCode::SCLOSURE,
                ..
            } => {
                def_stack += 1;
            }
            Instruction {
                op_code: OpCode::ECLOSURE,
                ..
            } => {
                if def_stack > 0 {
                    def_stack -= 1;
                }
            }
            _ => {}
        }
    }
}

pub fn insert_debruijn_indices(instructions: &mut [Instruction], symbol_map: &mut SymbolMap) {
    let mut stack: Vec<usize> = Vec::new();
    // let mut def_stack: Vec<usize> = Vec::new();

    // Snag the defines that are going to be available from the global scope
    let _ = collect_defines_from_current_scope(instructions, symbol_map);

    // Snag the binds before the defines
    // collect_binds_from_current_scope(instructions, symbol_map);

    // name mangle
    // Replace all identifiers with indices
    for i in 0..instructions.len() {
        match &instructions[i] {
            Instruction {
                op_code: OpCode::PUSH,
                contents:
                    Some(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }),
                ..
            } => {
                let idx = symbol_map.get(s);
                // println!("Renaming: {} to index: {}", s, idx);
                if let Some(x) = instructions.get_mut(i) {
                    x.payload_size = idx;
                    x.constant = false;
                }
            }
            // Is this even necessary?
            Instruction {
                op_code: OpCode::BIND,
                contents:
                    Some(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }),
                ..
            } => {
                let idx = symbol_map.get_or_add(s);

                // println!("Renaming: {} to index: {}", s, idx);
                if let Some(x) = instructions.get_mut(i) {
                    x.payload_size = idx;
                    // x.contents = None;
                }
            }
            Instruction {
                op_code: OpCode::SCLOSURE,
                ..
            } => {
                stack.push(symbol_map.len());
                // More stuff goes here
                let payload = *(&instructions[i].payload_size);

                // Go through the current scope and collect binds from the lambds
                collect_binds_from_current_scope(instructions, symbol_map, i, i + payload - 1);

                // Go through the current scope and find defines and the count
                let def_count = collect_defines_from_current_scope(
                    &instructions[i + 1..(i + payload - 1)],
                    symbol_map,
                );
                // Set the def count of the NDEFS instruction after the closure
                if let Some(x) = instructions.get_mut(i + 1) {
                    x.payload_size = def_count;
                }
            }
            Instruction {
                op_code: OpCode::ECLOSURE,
                ..
            } => symbol_map.roll_back(stack.pop().unwrap()),
            Instruction {
                op_code: OpCode::SDEF,
                // contents:
                //     Some(SyntaxObject {
                //         ty: TokenType::Identifier(s),
                //         ..
                //     }),
                ..
            } => {
                // let idx = symbol_map.add(s);
                // println!("Renaming: {} to index: {}", s, idx);

                if let Some(x) = instructions.get_mut(i) {
                    x.constant = false;
                }
            }
            _ => {}
        }
    }
}

pub fn emit_instructions<CT: ConstantTable>(
    expr_str: &str,
    symbol_map: &mut SymbolMap,
    constants: &mut CT,
    global_env: &Rc<RefCell<Env>>,
    arity_map: &mut ArityMap,
) -> Result<Vec<Vec<DenseInstruction>>> {
    let mut intern = HashMap::new();
    let mut results = Vec::new();

    let parsed: result::Result<Vec<Expr>, ParseError> =
        Parser::new(expr_str, &mut intern).collect();
    let parsed = parsed?;

    let macro_env = Rc::new(RefCell::new(Env::root()));
    // let real_env = Rc::new(RefCell::new(Env::default_env()));

    let extracted_statements =
        extract_macro_definitions(&parsed, &macro_env, global_env, symbol_map)?;

    for expr in extracted_statements {
        let mut instructions: Vec<Instruction> = Vec::new();
        emit_loop(&expr, &mut instructions, None, arity_map, constants)?;
        // instructions.push(Instruction::new_clear());
        instructions.push(Instruction::new_pop());

        pretty_print_instructions(&instructions);

        insert_debruijn_indices(&mut instructions, symbol_map);

        println!("------ DeBruijn Indices succeeded! ------");

        // println!("Got after the debruijn indices");

        extract_constants(&mut instructions, constants)?;

        let dense_instructions = densify(instructions);

        results.push(dense_instructions);
    }

    Ok(results)
}

pub fn densify(instructions: Vec<Instruction>) -> Vec<DenseInstruction> {
    instructions.into_iter().map(|x| x.into()).collect()
}

pub fn pretty_print_instructions(instrs: &[Instruction]) {
    // for (i, item) in foo.iter().enumerate()
    for (i, instruction) in instrs.iter().enumerate() {
        if instruction.contents.is_some() {
            println!(
                "{}    {:?} : {}     {}",
                i,
                instruction.op_code,
                instruction.payload_size,
                instruction.contents.as_ref().unwrap().ty
            );
        } else {
            println!(
                "{}    {:?} : {}",
                i, instruction.op_code, instruction.payload_size
            );
        }
    }
}

pub fn pretty_print_dense_instructions(instrs: &[DenseInstruction]) {
    // for (i, item) in foo.iter().enumerate()
    for (i, instruction) in instrs.iter().enumerate() {
        println!(
            "{}    {:?} : {}",
            i, instruction.op_code, instruction.payload_size
        );
    }
}

fn coalesce_clears(instructions: &mut Vec<Instruction>) {
    for i in 0..instructions.len() - 2 {
        match (
            instructions.get(i),
            instructions.get(i + 1),
            instructions.get(i + 2),
        ) {
            (
                Some(Instruction {
                    op_code: OpCode::FUNC,
                    ..
                }),
                Some(Instruction {
                    op_code: OpCode::CLEAR,
                    ..
                }),
                Some(Instruction {
                    op_code: OpCode::FUNC,
                    ..
                }),
            ) => {
                if let Some(x) = instructions.get_mut(i + 1) {
                    x.op_code = OpCode::PASS;
                }
            }
            (
                Some(Instruction {
                    op_code: OpCode::FUNC,
                    ..
                }),
                Some(Instruction {
                    op_code: OpCode::CLEAR,
                    ..
                }),
                _,
            ) => {}
            _ => {}
        }
    }
}

fn emit_loop<CT: ConstantTable>(
    expr: &Expr,
    instructions: &mut Vec<Instruction>,
    defining_context: Option<&str>,
    arity_map: &mut ArityMap,
    constant_map: &mut CT,
) -> Result<()> {
    match expr {
        Expr::Atom(s) => {
            instructions.push(Instruction::new(OpCode::PUSH, 0, s.clone(), true));
        }
        Expr::VectorVal(list_of_tokens) => {
            if let Some(f) = list_of_tokens.first() {
                match f.deref() {
                    Expr::Atom(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }) if s == "quote" => {
                        check_length("quote", &list_of_tokens, 2)?;
                        let converted = SteelVal::try_from(list_of_tokens[1].clone())?;
                        let idx = constant_map.add_or_get(Rc::new(converted));
                        instructions.push(Instruction::new_push_const(idx));
                        // instructions.push(Instruction::new_quote());
                        return Ok(());
                    }

                    Expr::Atom(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }) if s == "eval" => {
                        check_length("eval", &list_of_tokens, 2)?;
                        instructions.push(Instruction::new_eval());
                        return Ok(());
                    }

                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "declare" => {
                    //     // check_length("eval", &list_of_tokens, 2)?;
                    //     instructions.push(Instruction::new_eval());
                    //     return Ok(());
                    // }
                    Expr::Atom(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        span: sp,
                    }) if s == "if" => {
                        if let [test_expr, then_expr, else_expr] = &list_of_tokens[1..] {
                            // load in the test condition
                            emit_loop(test_expr, instructions, None, arity_map, constant_map)?;
                            // push in If
                            instructions.push(Instruction::new_if(instructions.len() + 2));
                            // save spot of jump instruction, fill in after
                            let idx = instructions.len();
                            instructions.push(Instruction::new_jmp(0)); // dummy

                            // emit instructions for then expression
                            emit_loop(then_expr, instructions, None, arity_map, constant_map)?;
                            instructions.push(Instruction::new_jmp(0)); // dummy
                            let false_start = instructions.len();

                            // emit instructions for else expression
                            emit_loop(else_expr, instructions, None, arity_map, constant_map)?;
                            let j3 = instructions.len(); // first instruction after else

                            // set index of jump instruction to be
                            if let Some(elem) = instructions.get_mut(idx) {
                                (*elem).payload_size = false_start;
                            } else {
                                stop!(Generic => "out of bounds jump");
                            }

                            if let Some(elem) = instructions.get_mut(false_start - 1) {
                                (*elem).payload_size = j3
                            } else {
                                stop!(Generic => "out of bounds jump");
                            }
                        } else {
                            stop!(BadSyntax => "malformed if statement"; *sp);
                        }
                        return Ok(());
                    }

                    Expr::Atom(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }) if s == "define" || s == "defn" => {
                        let sidx = instructions.len();
                        instructions.push(Instruction::new_sdef());

                        let identifier = &list_of_tokens[1];

                        match identifier {
                            Expr::Atom(syn) => {
                                let defining_context = if let TokenType::Identifier(name) = &syn.ty
                                {
                                    // Get the defining context for the debruijn indices
                                    if let Some(x) = instructions.get_mut(sidx) {
                                        x.contents = Some(syn.clone());
                                    }
                                    Some(name.as_str())
                                } else {
                                    None
                                };

                                if list_of_tokens.len() != 3 {
                                    let e = format!(
                                        "{}: multiple expressions after the identifier, expected {} args got {}",
                                        "Define",
                                        2,
                                        list_of_tokens.len()
                                    );
                                    stop!(ArityMismatch => e; syn.span)
                                }

                                emit_loop(
                                    &list_of_tokens[2],
                                    instructions,
                                    defining_context,
                                    arity_map,
                                    constant_map,
                                )?;

                                instructions.push(Instruction::new_pop());
                                let defn_body_size = instructions.len() - sidx;
                                instructions.push(Instruction::new_edef());

                                if let Some(elem) = instructions.get_mut(sidx) {
                                    (*elem).payload_size = defn_body_size;
                                } else {
                                    stop!(Generic => "out of bounds closure len");
                                }

                                instructions.push(Instruction::new_bind(syn.clone()));
                                instructions.push(Instruction::new_void());

                                // Roll back scope to default if the depth > 0?
                            }

                            // _ => {}
                            Expr::VectorVal(_) => {
                                panic!("Complex defines not yet supported");
                            }
                        }
                    }
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "define-syntax" => {
                    //     instructions.push("define-syntax".to_string());
                    //     return Ok(());
                    // }
                    // (lambda (vars*) (body))
                    Expr::Atom(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }) if s == "lambda" || s == "λ" || s == "fn" => {
                        let idx = instructions.len();
                        instructions.push(Instruction::new_sclosure());

                        instructions.push(Instruction::new_ndef(0)); // Default with 0 for now

                        let list_of_symbols = &list_of_tokens[1];

                        // make recursive call with "fresh" vector so that offsets are correct
                        let mut body_instructions = Vec::new();

                        // let mut arity = 0;
                        let arity;

                        match list_of_symbols {
                            Expr::VectorVal(l) => {
                                arity = l.len();
                                let rev_iter = l.into_iter().rev();
                                for symbol in rev_iter {
                                    if let Expr::Atom(syn) = symbol {
                                        // println!("{:?}", syn);
                                        match &syn {
                                            SyntaxObject {
                                                ty: TokenType::Identifier(_),
                                                ..
                                            } => body_instructions
                                                .push(Instruction::new_bind(syn.clone())),
                                            SyntaxObject { ty: _, span: sp } => {
                                                stop!(Generic => "lambda function requires list of identifiers"; *sp);
                                            }
                                        }
                                    } else {
                                        stop!(Generic => "lambda function requires list of identifiers"; symbol.span());
                                    }
                                }
                            }
                            _ => {
                                stop!(TypeMismatch => "List of Identifiers"; list_of_symbols.span())
                            }
                        }

                        // let mut body_instructions = Vec::new();

                        for expr in &list_of_tokens[2..] {
                            emit_loop(expr, &mut body_instructions, None, arity_map, constant_map)?;
                        }

                        // TODO look out here for the
                        body_instructions.push(Instruction::new_pop());
                        // body_instructions.push(Instruction::new_clear());

                        if let Some(ctx) = defining_context {
                            transform_tail_call(&mut body_instructions, ctx);
                            arity_map.insert_exact(ctx, arity);
                        }

                        instructions.append(&mut body_instructions);

                        let closure_body_size = instructions.len() - idx;
                        instructions.push(Instruction::new_eclosure(arity));

                        if let Some(elem) = instructions.get_mut(idx) {
                            (*elem).payload_size = closure_body_size;
                        } else {
                            stop!(Generic => "out of bounds closure len");
                        }

                        return Ok(());
                    }
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "eval" => {
                    //     instructions.push("eval".to_string());
                    //     for expr in &list_of_tokens[1..] {
                    //         emit_loop(Rc::clone(expr), instructions)?;
                    //     }
                    //     return Ok(());
                    // }
                    // set! expression
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "set!" => {
                    //     instructions.push("set!".to_string());
                    //     return Ok(());
                    // }
                    // (let (var binding)* (body))
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "let" => {
                    //     instructions.push("let".to_string());
                    //     return Ok(());
                    // }
                    Expr::Atom(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }) if s == "begin" => {
                        // instructions.push("begin".to_string());
                        for expr in &list_of_tokens[1..] {
                            emit_loop(expr, instructions, None, arity_map, constant_map)?;
                        }
                        return Ok(());
                    }
                    Expr::Atom(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }) if s == "return" => {
                        check_length("return", &list_of_tokens, 2)?;
                        emit_loop(
                            &list_of_tokens[1],
                            instructions,
                            None,
                            arity_map,
                            constant_map,
                        )?;
                        // pop is equivalent to the last instruction in the function
                        instructions.push(Instruction::new_pop());
                        return Ok(());
                    }
                    Expr::Atom(SyntaxObject {
                        ty: TokenType::Identifier(s),
                        ..
                    }) if s == "panic!" => {
                        check_length("panic!", &list_of_tokens, 2)?;
                        emit_loop(
                            &list_of_tokens[1],
                            instructions,
                            None,
                            arity_map,
                            constant_map,
                        )?;

                        // pop is equivalent to the last instruction in the function
                        instructions.push(Instruction::new_panic(
                            if let Expr::Atom(s) = &list_of_tokens[0] {
                                s.clone()
                            } else {
                                SyntaxObject::default(TokenType::Identifier("panic!".to_string()))
                            },
                        ));
                        return Ok(());
                    }
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "apply" => {
                    //     instructions.push("apply".to_string());
                    //     for expr in &list_of_tokens[1..] {
                    //         emit_loop(Rc::clone(expr), instructions)?;
                    //     }
                    //     return Ok(());
                    // }
                    // Catches errors and captures an Error result from the execution
                    // resumes execution at the other branch of the execution
                    // try! should match the following form:

                    /*
                    (try! [expression1] [except expression2])
                    */
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "try!" => {
                    //     instructions.push("try!".to_string());
                    //     return Ok(());
                    // }
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "export" => {
                    //     instructions.push("export".to_string());
                    //     return Ok(());
                    // }

                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "require" => {
                    //     instructions.push("require".to_string());
                    //     return Ok(());
                    // }
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "mapR" => {
                    //     instructions.push("mapR".to_string());
                    //     return Ok(());
                    // }
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "filterR" => {
                    //     instructions.push("filterR".to_string());
                    //     return Ok(());
                    // }
                    // Expr::Atom(SyntaxObject {
                    //     ty: TokenType::Identifier(s),
                    //     ..
                    // }) if s == "struct" => {
                    //     instructions.push("struct".to_string());
                    //     return Ok(());
                    // }
                    // Expr::Atom(s) => {
                    //     let pop_len = &list_of_tokens[1..].len();
                    //     for expr in &list_of_tokens[1..] {
                    //         emit_loop(Rc::clone(expr), instructions)?;
                    //     }
                    //     // instructions.push(format!("PUSH: Function Call: {}, {}", s, pop_len));
                    //     instructions.push(Instruction::new(OpCode::FUNC, *pop_len, s.clone()));
                    //     return Ok(());

                    //     // instructions.push("function call!".to_string());
                    // }
                    // (sym args*), sym must be a procedure
                    _sym => {
                        let pop_len = list_of_tokens[1..].len();

                        // TODO come back to this
                        // Update arity stuff correctly
                        if let Expr::Atom(SyntaxObject {
                            ty: TokenType::Identifier(_function_name),
                            ..
                        }) = &list_of_tokens[0]
                        {
                            // if !arity_map.check(function_name.as_str(), pop_len) {
                            //     stop!(ArityMismatch => "arity mismatch in function call with function {}", function_name);
                            // }
                        }

                        for expr in &list_of_tokens[1..] {
                            emit_loop(expr, instructions, None, arity_map, constant_map)?;
                        }

                        emit_loop(f, instructions, None, arity_map, constant_map)?;

                        if let Expr::Atom(s) = &list_of_tokens[0] {
                            instructions.push(Instruction::new_func(pop_len, s.clone()));
                        } else {
                            instructions.push(Instruction::new_func(
                                pop_len,
                                SyntaxObject::default(TokenType::Identifier("lambda".to_string())),
                            ));
                            instructions.push(Instruction::new_clear());
                        }

                        // TODO fix this noise
                        // instructions.push(Instruction::new_func(
                        //     pop_len,
                        //     if let Expr::Atom(s) = &list_of_tokens[0] {
                        //         s.clone()
                        //     } else {
                        //         SyntaxObject::default(TokenType::Identifier("lambda".to_string()))
                        //     },
                        // ));

                        // instructions.push(Instruction::new_clear());

                        return Ok(());
                    }
                }
            } else {
                stop!(TypeMismatch => "Given empty list"; expr.span())
            }
        }
    }

    Ok(())
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Hash, PartialEq)]
pub enum OpCode {
    VOID = 0,
    PUSH = 1,
    LOOKUP = 2,
    IF = 3,
    JMP = 4,
    FUNC = 5,
    SCLOSURE = 6,
    ECLOSURE = 7,
    STRUCT = 8,
    POP = 9,
    BIND = 10,
    SDEF = 11,
    EDEF = 12,
    PASS = 13,
    PUSHCONST = 14,
    NDEFS = 15,
    EVAL = 16,
    PANIC = 17,
    CLEAR = 18,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct DenseInstruction {
    op_code: OpCode,
    payload_size: usize,
    span: Span,
}

impl DenseInstruction {
    pub fn new(op_code: OpCode, payload_size: usize, span: Span) -> DenseInstruction {
        DenseInstruction {
            op_code,
            payload_size,
            span,
        }
    }
}

impl From<Instruction> for DenseInstruction {
    fn from(val: Instruction) -> DenseInstruction {
        DenseInstruction::new(
            val.op_code,
            val.payload_size,
            if let Some(syn) = val.contents {
                syn.span
            } else {
                Span::new(0, 0)
            },
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Instruction {
    op_code: OpCode,
    payload_size: usize,
    contents: Option<SyntaxObject>,
    constant: bool,
}

impl Instruction {
    pub fn new(
        op_code: OpCode,
        payload_size: usize,
        contents: SyntaxObject,
        constant: bool,
    ) -> Instruction {
        Instruction {
            op_code,
            payload_size,
            contents: Some(contents),
            constant,
        }
    }

    pub fn new_panic(span: SyntaxObject) -> Instruction {
        Instruction {
            op_code: OpCode::PANIC,
            payload_size: 0,
            contents: Some(span),
            constant: false,
        }
    }

    pub fn new_clear() -> Instruction {
        Instruction {
            op_code: OpCode::CLEAR,
            payload_size: 0,
            contents: None,
            constant: false,
        }
    }

    pub fn new_push_const(idx: usize) -> Instruction {
        Instruction {
            op_code: OpCode::PUSHCONST,
            payload_size: idx,
            contents: None,
            constant: true,
        }
    }

    pub fn new_eval() -> Instruction {
        Instruction {
            op_code: OpCode::EVAL,
            payload_size: 0,
            contents: None,
            constant: false,
        }
    }

    pub fn new_ndef(payload_size: usize) -> Instruction {
        Instruction {
            op_code: OpCode::NDEFS,
            payload_size,
            contents: None,
            constant: false,
        }
    }

    pub fn new_func(arity: usize, contents: SyntaxObject) -> Instruction {
        Instruction {
            op_code: OpCode::FUNC,
            payload_size: arity,
            contents: Some(contents),
            constant: false,
        }
    }

    pub fn new_pop() -> Instruction {
        Instruction {
            op_code: OpCode::POP,
            payload_size: 0,
            contents: None,
            constant: false,
        }
    }

    pub fn new_if(true_jump: usize) -> Instruction {
        Instruction {
            op_code: OpCode::IF,
            payload_size: true_jump,
            contents: None,
            constant: false,
        }
    }

    pub fn new_jmp(jump: usize) -> Instruction {
        Instruction {
            op_code: OpCode::JMP,
            payload_size: jump,
            contents: None,
            constant: false,
        }
    }

    pub fn new_sclosure() -> Instruction {
        Instruction {
            op_code: OpCode::SCLOSURE,
            payload_size: 0,
            contents: None,
            constant: false,
        }
    }

    pub fn new_eclosure(arity: usize) -> Instruction {
        Instruction {
            op_code: OpCode::ECLOSURE,
            payload_size: arity,
            contents: None,
            constant: false,
        }
    }

    pub fn new_bind(contents: SyntaxObject) -> Instruction {
        Instruction {
            op_code: OpCode::BIND,
            payload_size: 0,
            contents: Some(contents),
            constant: false,
        }
    }

    pub fn new_sdef() -> Instruction {
        Instruction {
            op_code: OpCode::SDEF,
            payload_size: 0,
            contents: None,
            constant: false,
        }
    }

    pub fn new_edef() -> Instruction {
        Instruction {
            op_code: OpCode::EDEF,
            payload_size: 0,
            contents: None,
            constant: false,
        }
    }

    pub fn new_void() -> Instruction {
        Instruction {
            op_code: OpCode::VOID,
            payload_size: 0,
            contents: None,
            constant: false,
        }
    }

    pub fn new_pass() -> Instruction {
        Instruction {
            op_code: OpCode::PASS,
            payload_size: 0,
            contents: None,
            constant: false,
        }
    }
}

pub struct Ctx<CT: ConstantTable> {
    pub(crate) symbol_map: SymbolMap,
    pub(crate) constant_map: CT,
    pub(crate) arity_map: ArityMap,
}

impl<CT: ConstantTable> Ctx<CT> {
    pub fn new(symbol_map: SymbolMap, constant_map: CT, arity_map: ArityMap) -> Ctx<CT> {
        Ctx {
            symbol_map,
            constant_map,
            arity_map,
        }
    }
}

pub struct VirtualMachine {
    global_env: Rc<RefCell<Env>>,
    global_heap: Vec<Rc<RefCell<Env>>>,
    macro_env: Rc<RefCell<Env>>,
}

impl VirtualMachine {
    pub fn new() -> VirtualMachine {
        VirtualMachine {
            global_env: Rc::new(RefCell::new(Env::default_env())),
            global_heap: Vec::new(),
            macro_env: Rc::new(RefCell::new(Env::root())),
        }
    }

    pub fn parse_and_execute<CT: ConstantTable>(
        &mut self,
        expr_str: &str,
        ctx: &mut Ctx<CT>,
    ) -> Result<Vec<Rc<SteelVal>>> {
        let now = Instant::now();
        let gen_bytecode = self.emit_instructions(expr_str, ctx)?;
        println!("Bytecode generated in: {:?}", now.elapsed());
        gen_bytecode
            .into_iter()
            .map(|x| {
                let now = Instant::now();
                let res = self.execute(Rc::new(x.into_boxed_slice()), &ctx.constant_map);
                println!("Time taken: {:?}", now.elapsed());
                res
            })
            .collect::<Result<Vec<Rc<SteelVal>>>>()
    }

    pub fn emit_instructions<CT: ConstantTable>(
        &mut self,
        expr_str: &str,
        ctx: &mut Ctx<CT>,
    ) -> Result<Vec<Vec<DenseInstruction>>> {
        let mut intern = HashMap::new();
        let mut results = Vec::new();

        // Parse the input
        let parsed: result::Result<Vec<Expr>, ParseError> =
            Parser::new(expr_str, &mut intern).collect();
        let parsed = parsed?;

        // Yoink the macro definitions
        // Add them to our macro env
        // TODO change this to be a unique macro env struct
        // Just a thin wrapper around a hashmap
        let extracted_statements = extract_macro_definitions(
            &parsed,
            &self.macro_env,
            &self.global_env,
            &mut ctx.symbol_map,
        )?;

        // Walk through and expand all macros, lets, and defines
        let expanded_statements: Vec<Expr> = extracted_statements
            .into_iter()
            .map(|x| expand(x, &self.global_env, &self.macro_env))
            .collect::<Result<Vec<Expr>>>()?;

        // Collect global defines here first
        let ndefs = count_and_collect_global_defines(&expanded_statements, &mut ctx.symbol_map);

        // Reserve the definitions in the global environment
        self.global_env
            .borrow_mut()
            .reserve_defs(if ndefs > 0 { ndefs - 1 } else { 0 });

        let mut instruction_buffer = Vec::new();
        let mut index_buffer = Vec::new();
        for expr in expanded_statements {
            let mut instructions: Vec<Instruction> = Vec::new();
            emit_loop(
                &expr,
                &mut instructions,
                None,
                &mut ctx.arity_map,
                &mut ctx.constant_map,
            )?;
            // if !script {
            // instructions.push(Instruction::new_clear());
            instructions.push(Instruction::new_pop());
            // }
            index_buffer.push(instructions.len());
            instruction_buffer.append(&mut instructions);
        }

        insert_debruijn_indices(&mut instruction_buffer, &mut ctx.symbol_map);
        extract_constants(&mut instruction_buffer, &mut ctx.constant_map)?;
        coalesce_clears(&mut instruction_buffer);

        for idx in index_buffer {
            let extracted: Vec<Instruction> = instruction_buffer.drain(0..idx).collect();
            pretty_print_instructions(extracted.as_slice());
            results.push(densify(extracted));
        }

        // for expr in expanded_statements {
        //     println!("{}", expr.to_string());

        //     let mut instructions: Vec<Instruction> = Vec::new();
        //     emit_loop(&expr, &mut instructions, None, arity_map, constant_map)?;
        //     instructions.push(Instruction::new_pop());

        //     pretty_print_instructions(&instructions);

        //     // let mut stack: Vec<usize> = Vec::new();

        //     insert_debruijn_indices(&mut instructions, symbol_map);

        //     println!("------- Debruijn Indices Succeeded -------");
        //     println!("Symbol Map: {:?}", symbol_map);

        //     extract_constants(&mut instructions, constant_map)?;

        //     let dense_instructions = densify(instructions);

        //     results.push(dense_instructions);
        // }

        Ok(results)
    }

    pub fn execute<CT: ConstantTable>(
        &mut self,
        instructions: Rc<Box<[DenseInstruction]>>,
        constants: &CT,
    ) -> Result<Rc<SteelVal>> {
        // execute_vm(instructions)
        // let mut stack: Vec<Rc<SteelVal>> = Vec::new();
        let mut heap: Vec<Rc<RefCell<Env>>> = Vec::new();
        // let mut constants: Vec<Rc<RefCell<Env>>

        // let global_env = Rc::new(RefCell::new(Env::default_env()));
        let result = vm(
            instructions,
            // &mut stack,
            &mut heap,
            Rc::clone(&self.global_env),
            constants,
        );

        if self.global_env.borrow().is_binding_context() {
            self.global_heap.append(&mut heap);
            self.global_env.borrow_mut().set_binding_context(false);
        }

        println!("Global heap length after: {}", self.global_heap.len());

        result
    }
}

pub fn execute_vm(
    instructions: Rc<Box<[DenseInstruction]>>,
    constants: &ConstantMap,
) -> Result<Rc<SteelVal>> {
    // let mut stack: Vec<Rc<SteelVal>> = Vec::new();
    let mut heap: Vec<Rc<RefCell<Env>>> = Vec::new();
    // let mut constants: Vec<Rc<SteelVal>> = Vec::new();
    let global_env = Rc::new(RefCell::new(Env::default_env()));
    vm(instructions, &mut heap, global_env, constants)
}

// TODO make this not so garbage but its kind of okay
pub fn extract_constants<CT: ConstantTable>(
    instructions: &mut [Instruction],
    constants: &mut CT,
) -> Result<()> {
    for i in 0..instructions.len() {
        let inst = &instructions[i];
        if let OpCode::PUSH = inst.op_code {
            // let idx = constants.len();
            if inst.constant {
                let value = eval_atom(&inst.contents.as_ref().unwrap())?;
                let idx = constants.add_or_get(value);
                // constants.push(eval_atom(&inst.contents.as_ref().unwrap())?);
                if let Some(x) = instructions.get_mut(i) {
                    x.op_code = OpCode::PUSHCONST;
                    x.payload_size = idx;
                    x.contents = None;
                }
            }
        }

        // else if let OpCode::PUSHQUOTE = inst.op_code {
        //     let idx = constants.len();

        //     if let Some(syntax) = inst.contents {
        //         //     check_length("Quote", &list_of_tokens, 2)?;
        //         let converted = SteelVal::try_from(syntax.ty)?;
        //         // return Ok(Rc::new(converted));

        //         // let converted = SteelVal::try_from(syntax);
        //         unimplemented!()
        //     }
        // }
    }

    Ok(())
}

pub fn inspect_heap(heap: &Vec<Rc<RefCell<Env>>>) {
    let hp: Vec<String> = heap
        .into_iter()
        .map(|x| x.borrow().string_bindings_vec())
        .collect();
    println!("{:?}", hp);
}

pub fn vm<CT: ConstantTable>(
    instructions: Rc<Box<[DenseInstruction]>>,
    // stack: &mut Vec<Rc<SteelVal>>,
    heap: &mut Vec<Rc<RefCell<Env>>>,
    global_env: Rc<RefCell<Env>>,
    constants: &CT,
) -> Result<Rc<SteelVal>> {
    let mut ip = 0;
    let mut global_env = global_env;

    if instructions.is_empty() {
        stop!(Generic => "empty stack!");
    }

    // instruction stack for function calls
    let mut instruction_stack: Vec<(usize, Rc<Box<[DenseInstruction]>>)> = Vec::new();
    // parallel instruction stack
    // let mut instruction_ptr_stack: Vec<usize> = Vec::new();
    // stacks on stacks baby
    let mut stacks: Vec<Vec<Rc<SteelVal>>> = Vec::new();
    // initialize the instruction pointer
    let mut cur_inst;

    let mut instructions = instructions;
    let mut stack: Vec<Rc<SteelVal>> = Vec::new();
    // Will only contain Closure values
    // let mut closure_stack: Vec<Rc<SteelVal>> = Vec::new();

    let mut env_stack: Vec<Rc<RefCell<Env>>> = Vec::new();

    let mut pop_count = 1;

    let mut heap_stack: Vec<usize> = Vec::new();

    // let mut heap_count = 0;

    // Depth of the nested definition
    // let mut def_stack = 0;

    while ip < instructions.len() {
        cur_inst = &instructions[ip];

        match cur_inst.op_code {
            OpCode::PANIC => {
                let error_message = stack.pop().unwrap();
                stop!(Generic => error_message.to_string(); cur_inst.span);
            }
            OpCode::EVAL => {
                panic!("eval not yet supported");
            }
            OpCode::PASS => {
                ip += 1;
            }
            OpCode::VOID => {
                stack.push(VOID.with(|f| Rc::clone(f)));
                ip += 1;
            }
            OpCode::PUSHCONST => {
                let val = constants.get(cur_inst.payload_size);
                stack.push(val);
                ip += 1;
            }
            OpCode::PUSH => {
                let value = global_env.borrow().lookup_idx(cur_inst.payload_size)?;
                stack.push(value);
                ip += 1;
            }
            OpCode::CLEAR => {
                println!("%%%%%%%%%%% Hitting clear! %%%%%%%%%%%");
                println!("length of heap at clear: {}", heap.len());
                // println!("Heap count: {}", heap_count);
                println!("Heap at clear:");
                inspect_heap(&heap);
                heap.clear();
                // heap.remove(0);
                // heap_count += 1;
                ip += 1;
            }
            OpCode::FUNC => {
                let stack_func = stack.pop().unwrap();
                // closure_stack.push(stack_func);

                // let stack_func = stack.last().unwrap();

                // let match_stack = Rc::clone(&stack_func);

                match stack_func.as_ref() {
                    // SteelVal::SymbolV(s) => {
                    //     let func = global_env.borrow().lookup(s.as_str())?;
                    //     let args = stack.split_off(stack.len() - cur_inst.payload_size);
                    //     if let SteelVal::FuncV(f) = func.as_ref() {
                    //         stack.push(f(args)?);
                    //         ip += 1;
                    //         cur_inst = &instructions[ip];
                    //     } else {
                    //         unimplemented!();
                    //     }
                    // }
                    SteelVal::StructClosureV(factory, func) => {
                        let args = stack.split_off(stack.len() - cur_inst.payload_size);
                        let result = func(args, factory).map_err(|x| x.set_span(cur_inst.span))?;
                        stack.push(result);
                        ip += 1;
                    }
                    SteelVal::FuncV(f) => {
                        let args = stack.split_off(stack.len() - cur_inst.payload_size);
                        // println!("Calling function with args: {:?}", args);
                        // println!("Span found at this instruction: {:?}", cur_inst.span);
                        stack.push(f(args).map_err(|x| x.set_span(cur_inst.span))?);
                        // println!("{:?}", stack);
                        ip += 1;
                        // cur_inst = &instructions[ip];
                    }
                    SteelVal::Closure(closure) => {
                        // println!("Calling function");

                        if stacks.len() == STACK_LIMIT {
                            stop!(Generic => "stack overflowed!"; cur_inst.span);
                        }

                        // let cloned_stack_func = Rc::clone(&stack_func);
                        // let closure = cloned_stack_func.bytecode_lambda_or_panic();
                        // closure_stack.push(cloned_stack_func);

                        // let closure = cloned_stack_func.bytecode_lambda_or_panic();
                        // closure_stack.push(Rc::clone(&cloned_stack_func));

                        // println!("Stack inside closure case: {:?}", stack);

                        // let mut args = stack.split_off(stack.len() - cur_inst.payload_size);

                        let args = stack.split_off(stack.len() - cur_inst.payload_size);

                        if let Some(parent_env) = closure.parent_env() {
                            let offset = closure.offset() + parent_env.borrow().local_offset();

                            // let parent_env = lambda.parent_env();
                            let inner_env = Rc::new(RefCell::new(Env::new(&parent_env, offset)));

                            inner_env
                                .borrow_mut()
                                .reserve_defs(if closure.ndef_body() > 0 {
                                    closure.ndef_body() - 1
                                } else {
                                    0
                                });

                            // let params_exp = lambda.params_exp();
                            // let result =
                            // vm(closure.body_exp(), &mut args, heap, inner_env, constants)?;

                            // println!("Found a closure");
                            // instead of the recursive call, update the values and go back through the loop...
                            // closure_stack.push(Rc::clone(&stack_func));
                            env_stack.push(global_env);
                            // println!("Env stack size after pushing up top: {}", env_stack.len());
                            // println!("Env stack:");
                            // inspect_heap(&env_stack);
                            global_env = inner_env;
                            instruction_stack.push((ip + 1, instructions));
                            pop_count += 1;
                            stacks.push(stack);
                            instructions = closure.body_exp();
                            stack = args;
                            ip = 0;

                        // stack.push(result);

                        // evaluate(&lambda.body_exp(), &inner_env)
                        } else if let Some(parent_env) = closure.sub_expression_env() {
                            // TODO remove this unwrap
                            let offset = closure.offset()
                                + parent_env.upgrade().unwrap().borrow().local_offset();

                            let inner_env = Rc::new(RefCell::new(Env::new_subexpression(
                                parent_env.clone(),
                                offset,
                            )));

                            inner_env
                                .borrow_mut()
                                .reserve_defs(if closure.ndef_body() > 0 {
                                    closure.ndef_body() - 1
                                } else {
                                    0
                                });

                            // let result =
                            // vm(closure.body_exp(), &mut args, heap, inner_env, constants)?;
                            // closure_stack.push(Rc::clone(&stack_func));
                            // TODO this is where the memory leak is
                            env_stack.push(global_env);
                            // println!("Env stack size after pushing below: {}", env_stack.len());
                            // println!("Env stack:");
                            // inspect_heap(&env_stack);
                            global_env = inner_env;
                            instruction_stack.push((ip + 1, instructions));
                            pop_count += 1;
                            stacks.push(stack);
                            instructions = closure.body_exp();
                            stack = args;
                            ip = 0;

                        // heap.pop();

                        // stack.push(result);
                        } else {
                            stop!(Generic => "Root env is missing!")
                        }

                        // Do lookahead to see if we have exited scope
                        // match &instructions.get(ip + 1) {
                        //     Some(DenseInstruction {
                        //         op_code: OpCode::FUNC,
                        //         ..
                        //     }) => {}
                        //     Some(_) => {
                        //         println!("CLEARING THE HEAP");
                        //         println!("Instructions: {}", ip);
                        //         pretty_print_dense_instructions(&instructions);
                        //         heap.clear();
                        //     }
                        //     _ => {}
                        // }

                        // ip += 1;
                        // cur_inst = &instructions[ip];
                    }
                    _ => {
                        stop!(BadSyntax => "Application not a procedure or function type not supported"; cur_inst.span);
                    }
                }
            }
            OpCode::IF => {
                // println!("stack at if: {:?}", stack);
                // stack.pop()
                // if let Some(SteelVal::BoolV(true)) = stack.pop().as_ref()
                if let SteelVal::BoolV(true) = stack.pop().unwrap().as_ref() {
                    ip = cur_inst.payload_size; // Jump to payload
                                                // ip += 2; // Jump to payload
                                                // cur_inst = &instructions[ip];
                } else {
                    ip += 1;
                    // cur_inst = &instructions[ip];
                }
            }
            OpCode::JMP => {
                ip = cur_inst.payload_size;
                // cur_inst = &instructions[ip];
            }
            OpCode::POP => {
                pop_count -= 1;
                if pop_count == 0 {
                    println!("Stack of stack at pop: {:?}", stacks);
                    println!("Stack at pop: {:?}", stack);
                    println!("Env stack length: {}", env_stack.len());
                    println!("instruction stack at pop: {:?}", instruction_stack);
                    println!("Heap at exit: {}", heap.len());

                    env_stack.clear();
                    heap.clear();

                    return stack.pop().ok_or_else(|| {
                        SteelErr::Generic("stack empty at pop".to_string(), Some(cur_inst.span))
                    });
                // if instruction_stack.is_empty() && def_stack > 0
                } else {
                    // println!("Getting here with def stack: {}", def_stack);

                    // println!("$$$$$$$$$$$$$$$$$$$$$$$$$$$$$");
                    // println!("depth: {}", instruction_stack.len());
                    // println!("env stack: ");
                    // inspect_heap(&env_stack);

                    // let ret_val = stack.pop().unwrap();
                    // def_stack -= 1;
                    // stack = stacks.pop().unwrap();
                    // stack.push(ret_val);
                    // ip += 1;

                    let ret_val = stack.pop().unwrap();
                    let prev_state = instruction_stack.pop().unwrap();

                    if prev_state.1.len() != 0 {
                        global_env = env_stack.pop().unwrap();
                        // println!("Env stack size after popping: {}", env_stack.len());
                        // println!("Env stack:");
                        // inspect_heap(&env_stack);
                        // println!("Instructions -> At instr # : {}", ip);
                        // pretty_print_dense_instructions(&instructions);
                        ip = prev_state.0;
                        instructions = prev_state.1;
                    // inspect_heap(&heap);
                    // heap.remove(0);

                    // heap.pop();
                    // heap.truncate(heap_stack.pop();

                    // heap_stack.pop().and_then(|x| {
                    //     println!("Popped off this value {}", x);
                    //     heap.truncate(x);
                    //     Some(x)
                    // });

                    // println!("Heap:");
                    // inspect_heap(&heap);
                    // if heap.len() > 2 {
                    //     heap.remove(0);
                    // }
                    // if heap_count > 2 {
                    //     // heap.pop();
                    //     heap_count -= 1;
                    // }
                    // let _l = heap.pop();
                    // heap.truncate(heap_stack.pop().unwrap());
                    // heap.pop();
                    } else {
                        println!("%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%");
                        ip += 1;
                    }

                    stack = stacks.pop().unwrap();
                    stack.push(ret_val);

                    // unimplemented!();
                }
                // else {

                // }

                // instruction_stack.push((ip + 1, instructions));
                // stacks.push(stack);
                // instructions = closure.body_exp();
                // stack = args;
                // ip = 0;

                // unimplemented!();

                // return stack.pop().ok_or_else(|| {
                //     SteelErr::Generic("stack empty at pop".to_string(), Some(cur_inst.span))
                // });
            }
            OpCode::BIND => {
                let offset = global_env.borrow().local_offset();

                // println!(
                //     "Defining with payload: {} and offset: {}",
                //     cur_inst.payload_size, offset
                // );

                global_env
                    .borrow_mut()
                    .define_idx(cur_inst.payload_size - offset, stack.pop().unwrap());
                ip += 1;
            }
            OpCode::SCLOSURE => {
                ip += 1;
                // pop_count += 1;

                let forward_jump = cur_inst.payload_size - 1;
                // Snag the number of definitions here
                let ndefs = instructions[ip].payload_size;
                ip += 1;
                // Construct the closure body using the offsets from the payload
                // used to be - 1, now - 2
                let closure_body = instructions[ip..(ip + forward_jump - 1)].to_vec();

                // snag the arity from the eclosure instruction
                let arity = instructions[ip + forward_jump - 1].payload_size;

                let capture_env = Rc::clone(&global_env);

                let mut closure_offset = global_env.borrow().len();

                if global_env.borrow().is_binding_context()
                    && !global_env.borrow().is_binding_offset()
                {
                    global_env.borrow_mut().set_binding_offset(true);
                    closure_offset += 1
                };

                // set the number of definitions for the environment
                capture_env.borrow_mut().set_ndefs(ndefs);

                // TODO look at this heap thing
                // Need to clear it pop when the environment exits
                // GC...
                println!("Pushing onto the heap!");

                heap.push(Rc::clone(&capture_env));
                // Set fallback point
                heap_stack.push(heap.len());
                inspect_heap(&heap);
                let constructed_lambda = ByteCodeLambda::new(
                    closure_body,
                    None,
                    Some(Rc::downgrade(&capture_env)),
                    closure_offset,
                    arity,
                    ndefs,
                );

                // Determine the kind of bytecode lambda to construct
                // let constructed_lambda = if capture_env.borrow().is_root() {
                //     ByteCodeLambda::new(
                //         closure_body,
                //         Some(capture_env),
                //         None,
                //         closure_offset,
                //         arity,
                //         ndefs,
                //     )
                // } else {
                //     // set the number of definitions for the environment
                //     capture_env.borrow_mut().set_ndefs(ndefs);

                //     // TODO look at this heap thing
                //     // Need to clear it pop when the environment exits
                //     // GC...
                //     heap.push(Rc::clone(&capture_env));
                //     ByteCodeLambda::new(
                //         closure_body,
                //         None,
                //         Some(Rc::downgrade(&capture_env)),
                //         closure_offset,
                //         arity,
                //         ndefs,
                //     )
                // };

                stack.push(Rc::new(SteelVal::Closure(constructed_lambda)));

                ip += forward_jump;
                println!("Performed forward jump to instruction: {}", ip);
            }
            OpCode::ECLOSURE => {
                // println!("Hitting ECLOSURE");
                // println!("Heap at end of closure: ");
                // inspect_heap(&heap);
                // heap.pop();
                ip += 1;
            }
            OpCode::SDEF => {
                ip += 1;

                global_env.borrow_mut().set_binding_context(true);
                global_env.borrow_mut().set_binding_offset(false);

                // let defn_body = &instructions[ip..(ip + cur_inst.payload_size - 1)];

                // println!("Instructions for def body: {:?}", defn_body);

                // let temp_stack: Vec<Rc<SteelVal>> = Vec::new();

                stacks.push(stack);
                stack = Vec::new();
                // def_stack += 1;

                // placeholder on the instruction_stack
                instruction_stack.push((0, Rc::new(Box::new([]))));
                pop_count += 1;

                // println!("Incrementing the def stack with instructions: ");
                // pretty_print_dense_instructions(&instructions);

                // unimplemented!();

                // let result = vm(
                //     defn_body,
                //     &mut temp_stack,
                //     heap,
                //     Rc::clone(&global_env),
                //     constants,
                // )?;

                // stack.push(result);
                // ip += cur_inst.payload_size;
                // cur_inst = &instructions[ip];
            }
            OpCode::EDEF => {
                println!("Found end definition");
                // def_stack -= 1;
                ip += 1;
                // unimplemented!();
            }
            _ => {
                unimplemented!();
            }
        }
    }

    // unimplemented!()
    println!("###### Out of bounds instruction ######");
    println!(
        "Instruction pointer: {}, instructions length: {}",
        ip,
        instructions.len()
    );
    println!("Instructions at time:");
    pretty_print_dense_instructions(&instructions);
    panic!("Out of bounds instruction")
}

/// evaluates an atom expression in given environment
fn eval_atom(t: &SyntaxObject) -> Result<Rc<SteelVal>> {
    match &t.ty {
        TokenType::BooleanLiteral(b) => {
            if *b {
                Ok(TRUE.with(|f| Rc::clone(f)))
            } else {
                Ok(FALSE.with(|f| Rc::clone(f)))
            }
        }
        // TokenType::Identifier(s) => env.borrow().lookup(&s),
        TokenType::NumberLiteral(n) => Ok(Rc::new(SteelVal::NumV(*n))),
        TokenType::StringLiteral(s) => Ok(Rc::new(SteelVal::StringV(s.clone()))),
        TokenType::CharacterLiteral(c) => Ok(Rc::new(SteelVal::CharV(*c))),
        TokenType::IntegerLiteral(n) => Ok(Rc::new(SteelVal::IntV(*n))),
        what => {
            // println!("getting here");
            stop!(UnexpectedToken => what; t.span)
        }
    }
}

/// returns error if tokens.len() != expected
fn check_length(what: &str, tokens: &[Expr], expected: usize) -> Result<()> {
    if tokens.len() == expected {
        Ok(())
    } else {
        if let Some((first, rest)) = tokens.split_first() {
            let span = rest
                .into_iter()
                .map(|x| x.span())
                .fold(first.span(), |x, y| Span::merge(x, y));

            Err(SteelErr::ArityMismatch(
                format!("{}: expected {} args got {}", what, expected, tokens.len()),
                Some(span),
            ))
        } else {
            Err(SteelErr::ArityMismatch(
                format!("{}: expected {} args got {}", what, expected, tokens.len()),
                None,
            ))
        }
    }
}

/*
(+ 1 2 (+ 3 4 (+ 5 6)))

push 1
push 2
push 3
push 4
push 5
push 6
push (BUILT_IN_FUNCTION + (pop 2))
push (BUILT_IN_FUNCTION + (pop 3))
push BUILT_IN_FUNCTION + (pop 3)
END -> pop last result


(if (= 1 2) (+ 1 2 3) (+ 4 5 6))


push 1
push 2
push (BUILT_IN_FUNCTION) (pop 2)
IF - pop last
JMP payload: 1
JMP payload: 2

JMP - 1
push 1
push 2
push 3
push (BUILT_IN_FUNCTION) (pop 3)
END

JMP - 2
push 4
push 5
push 6
push (BUILT_IN_FUNCTION) (pop 3)
END
*/