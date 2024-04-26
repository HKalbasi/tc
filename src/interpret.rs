use llvm_ir::{instruction::Call, terminator::CondBr, Function, Operand};

use crate::{sexp::Sexp, z3_decl::if_then_else, MemorySnapshot, VerifierState};

#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub bb: usize,
    pub instr: usize,
}

#[derive(Debug, Clone)]
pub enum Effect {
    Call { return_pos: Position, call: Call },
    Return(Option<Operand>),
    CondBr(CondBr),
}

impl VerifierState {
    pub fn run_until_effect(
        &mut self,
        f: Function,
        p: Position,
        mut memory: MemorySnapshot,
    ) -> (MemorySnapshot, Effect) {
        let bb = &f.basic_blocks[p.bb];
        if p.instr == bb.instrs.len() {
            match &bb.term {
                llvm_ir::Terminator::Ret(ret) => {
                    return (memory, Effect::Return(ret.return_operand.clone()))
                }
                llvm_ir::Terminator::CondBr(cond_br) => {
                    return (memory, Effect::CondBr(cond_br.clone()));
                }
                _ => unimplemented!("{:?}", bb.term),
            }
        }
        for (instr_id, instr) in bb.instrs.iter().enumerate().skip(p.instr) {
            macro_rules! binop_instr {
                ($x:ident, $z3fn:expr) => {{
                    let o0 = self.operand_to_sexp(&$x.operand0, memory);
                    let o1 = self.operand_to_sexp(&$x.operand1, memory);
                    let o = Sexp::s3($z3fn, o0, o1);
                    let size = self.size_of_operand(&$x.operand0);
                    let addr = self.address_of_name(&$x.dest);
                    let next_memory = self.store_in_addr(addr, size, o, memory);
                    memory = next_memory;
                }};
            }
            match instr {
                llvm_ir::Instruction::Add(add) => binop_instr!(add, "bvadd"),
                llvm_ir::Instruction::And(and) => binop_instr!(and, "bvand"),
                llvm_ir::Instruction::Sub(sub) => binop_instr!(sub, "bvsub"),
                llvm_ir::Instruction::ICmp(icmp) => {
                    let operation = match icmp.predicate {
                        llvm_ir::IntPredicate::EQ => "=",
                        llvm_ir::IntPredicate::NE => todo!(),
                        llvm_ir::IntPredicate::UGT => "bvugt",
                        llvm_ir::IntPredicate::UGE => "bvuge",
                        llvm_ir::IntPredicate::ULT => "bvult",
                        llvm_ir::IntPredicate::ULE => "bvule",
                        llvm_ir::IntPredicate::SGT => "bvsgt",
                        llvm_ir::IntPredicate::SGE => "bvsge",
                        llvm_ir::IntPredicate::SLT => "bvslt",
                        llvm_ir::IntPredicate::SLE => "bvsle",
                    };
                    let o0 = self.operand_to_sexp(&icmp.operand0, memory);
                    let o1 = self.operand_to_sexp(&icmp.operand1, memory);
                    let r = if_then_else(Sexp::s3(operation, o0, o1), "#x01", "#x00");
                    let addr = self.address_of_name(&icmp.dest);
                    memory = self.store_in_addr(addr, 1, r, memory);
                }
                llvm_ir::Instruction::Select(select) => {
                    let condition = self.operand_to_sexp(&select.condition, memory);
                    let otrue = self.operand_to_sexp(&select.true_value, memory);
                    let ofalse = self.operand_to_sexp(&select.false_value, memory);
                    let r = if_then_else(Sexp::s3("=", condition, "#x00"), ofalse, otrue);
                    let addr = self.address_of_name(&select.dest);
                    let size = self.size_of_operand(&select.true_value);
                    memory = self.store_in_addr(addr, size, r, memory);
                }
                llvm_ir::Instruction::Call(call) => {
                    return (
                        memory,
                        Effect::Call {
                            call: call.clone(),
                            return_pos: Position {
                                bb: p.bb,
                                instr: instr_id + 1,
                            },
                        },
                    )
                }
                _ => unimplemented!("{instr:?}"),
            }
        }
        let next_pos = Position {
            bb: p.bb,
            instr: bb.instrs.len(),
        };
        self.run_until_effect(f, next_pos, memory)
    }
}
