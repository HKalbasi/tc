use std::{
    borrow::BorrowMut,
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fmt::Display,
    io::Write,
    os::unix::process::CommandExt,
};

use interpret::{Effect, Position};
use llvm_ir::{
    instruction::{BinaryOp, Call},
    types::Typed,
    Function, Module, Operand, Terminator,
};
use sexp::{Sexp, ToSexp};
use z3_decl::{bit_to_byte, bv_hex, bv_ty, declare_const, define_const, if_then_else, memory_ty};

mod interpret;
mod sexp;
mod z3_decl;

#[derive(Debug, Clone)]
struct VerifierState {
    local_addresses: RefCell<HashMap<llvm_ir::Name, usize>>,
    left: Function,
    right: Function,
    z3_state: String,
    memory_generator_counter: usize,
    intersting_consts: Vec<String>,
    goal: Vec<Sexp>,
}

#[derive(Debug, Clone, Copy)]
struct MemorySnapshot {
    index: usize,
}

impl Display for MemorySnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("memory_")?;
        self.index.fmt(f)
    }
}

impl ToSexp for MemorySnapshot {
    fn to_sexp(self) -> Sexp {
        self.to_string().to_sexp()
    }
}

impl VerifierState {
    fn new(left: Function, right: Function) -> Self {
        Self {
            local_addresses: RefCell::new(HashMap::new()),
            left,
            right,
            z3_state: "".to_owned(),
            memory_generator_counter: 0,
            intersting_consts: vec![],
            goal: vec![],
        }
    }

    fn compare_functions(mut self) -> bool {
        let memory = self.new_memory();
        self.add_z3_line(declare_const(memory, memory_ty()));
        for p in self.left.parameters.clone() {
            let name = format!("param_{}", p.name);
            let size = self.size_of_ty(&p.ty);
            let addr = self.address_of_name(&p.name);
            let value = self.load_from_addr(addr, size, memory);
            self.add_z3_line(define_const(&*name, bv_ty(size * 8), value));
            self.intersting_consts.push(name);
        }
        self.compare_bb_start(0, 0, memory, memory)
    }

    fn add_z3_line(&mut self, arg: Sexp) {
        self.z3_state += &arg.to_pretty(100);
        self.z3_state.push('\n');
    }

    fn new_memory(&mut self) -> MemorySnapshot {
        let r = MemorySnapshot {
            index: self.memory_generator_counter,
        };
        self.memory_generator_counter += 1;
        r
    }

    fn compare_bb_start(
        &self,
        left_bb: usize,
        right_bb: usize,
        left_memory: MemorySnapshot,
        right_memory: MemorySnapshot,
    ) -> bool {
        let mut queue = VecDeque::new();
        queue.push_back((
            self.clone(),
            left_memory,
            right_memory,
            Position { bb: 0, instr: 0 },
            Position { bb: 0, instr: 0 },
        ));
        while let Some((mut this, left_memory, right_memory, left_pos, right_pos)) =
            queue.pop_front()
        {
            let (left_memory, left_effect) =
                this.run_until_effect(self.left.clone(), left_pos, left_memory);
            let (right_memory, right_effect) =
                this.run_until_effect(self.right.clone(), right_pos, right_memory);
            match (left_effect.clone(), right_effect.clone()) {
                (Effect::Return(left_op), Effect::Return(right_op)) => {
                    this.compare_returns(left_op, right_op, left_memory, right_memory);
                    return true;
                }
                (
                    Effect::Call {
                        call: left_call,
                        return_pos: left_pos,
                    },
                    Effect::Call {
                        call: right_call,
                        return_pos: right_pos,
                    },
                ) => {
                    this.clone()
                        .compare_calls(left_call, right_call, left_memory, right_memory);
                    queue.push_back((this, left_memory, right_memory, left_pos, right_pos));
                }
                (Effect::CondBr(left_br), Effect::CondBr(right_br)) => {
                    let left_cond_false = Sexp::s3(
                        "=",
                        self.operand_to_sexp(&left_br.condition, left_memory),
                        "#x00",
                    );
                    let left_cond_true = Sexp::s2("not", left_cond_false.clone());
                    let right_cond_false = Sexp::s3(
                        "=",
                        self.operand_to_sexp(&right_br.condition, right_memory),
                        "#x00",
                    );
                    let right_cond_true = Sexp::s2("not", right_cond_false.clone());
                    let left_true_pos = pos_of_bb_name(&left_br.true_dest, &self.left);
                    let left_false_pos = pos_of_bb_name(&left_br.false_dest, &self.left);
                    let right_true_pos = pos_of_bb_name(&right_br.true_dest, &self.right);
                    let right_false_pos = pos_of_bb_name(&right_br.false_dest, &self.right);
                    let this_true_true = {
                        let mut t = this.clone();
                        t.add_z3_line(Sexp::s2("assert", left_cond_true.clone()));
                        t.add_z3_line(Sexp::s2("assert", right_cond_true.clone()));
                        t
                    };
                    let this_true_false = {
                        let mut t = this.clone();
                        t.add_z3_line(Sexp::s2("assert", left_cond_true.clone()));
                        t.add_z3_line(Sexp::s2("assert", right_cond_false.clone()));
                        t
                    };
                    let this_false_true = {
                        let mut t = this.clone();
                        t.add_z3_line(Sexp::s2("assert", left_cond_false.clone()));
                        t.add_z3_line(Sexp::s2("assert", right_cond_true.clone()));
                        t
                    };
                    let this_false_false = {
                        let mut t = this.clone();
                        t.add_z3_line(Sexp::s2("assert", left_cond_false.clone()));
                        t.add_z3_line(Sexp::s2("assert", right_cond_false.clone()));
                        t
                    };
                    queue.push_back((
                        this_true_true,
                        left_memory,
                        right_memory,
                        left_true_pos,
                        right_true_pos,
                    ));
                    queue.push_back((
                        this_true_false,
                        left_memory,
                        right_memory,
                        left_true_pos,
                        right_false_pos,
                    ));
                    queue.push_back((
                        this_false_true,
                        left_memory,
                        right_memory,
                        left_false_pos,
                        right_true_pos,
                    ));
                    queue.push_back((
                        this_false_false,
                        left_memory,
                        right_memory,
                        left_false_pos,
                        right_false_pos,
                    ));
                }
                _ => {
                    let reason = match (left_effect, right_effect) {
                        (Effect::Call { .. }, Effect::Return(_)) => "Call missed in new",
                        (Effect::Return(_), Effect::Call { .. }) => "Call happened in new",
                        (Effect::CondBr(_), _)
                        | (_, Effect::CondBr(_))
                        | (Effect::Call { .. }, Effect::Call { .. })
                        | (Effect::Return(_), Effect::Return(_)) => unreachable!(),
                    };
                    this.check_sat(reason);
                }
            }
        }
        true
    }

    fn add_interesting_compare(
        &mut self,
        name: &str,
        ty: Sexp,
        left_value: Sexp,
        right_value: Sexp,
    ) {
        let name_left = format!("{name}_left");
        let name_right = format!("{name}_right");
        self.add_z3_line(define_const(name_left.as_str(), ty.clone(), left_value));
        self.add_z3_line(define_const(name_right.as_str(), ty, right_value));
        self.goal
            .push(Sexp::s3("=", name_left.as_str(), name_right.as_str()));
        self.intersting_consts.push(name_left);
        self.intersting_consts.push(name_right);
    }

    fn compare_returns(
        mut self,
        left_op: Option<Operand>,
        right_op: Option<Operand>,
        left_memory: MemorySnapshot,
        right_memory: MemorySnapshot,
    ) {
        let Some(left_op) = &left_op else {
            return;
        };
        let Some(right_op) = &right_op else {
            return;
        };
        let left_value = self.operand_to_sexp(left_op, left_memory);
        let right_value = self.operand_to_sexp(right_op, right_memory);
        let size = self.size_of_operand(left_op);
        self.add_interesting_compare("return", bv_ty(size * 8), left_value, right_value);
        self.check_sat("Return with different values");
    }

    fn operand_to_sexp(&self, operand: &llvm_ir::Operand, memory: MemorySnapshot) -> Sexp {
        match operand {
            llvm_ir::Operand::LocalOperand { name, ty } => {
                let size = self.size_of_ty(ty);
                let addr = self.address_of_name(name);
                self.load_from_addr(addr, size, memory)
            }
            llvm_ir::Operand::ConstantOperand(c) => match &**c {
                &llvm_ir::Constant::Int { bits, value } => {
                    bv_hex(value as usize, bit_to_byte(bits as usize))
                }
                llvm_ir::Constant::GlobalReference { name, ty } => {
                    let size = self.size_of_ty(ty);
                    let addr = self.address_of_name(name);
                    self.load_from_addr(addr, size, memory)
                }
                _ => todo!(),
            },
            llvm_ir::Operand::MetadataOperand => todo!(),
        }
    }

    fn store_in_addr(
        &mut self,
        addr: usize,
        size: usize,
        o: Sexp,
        memory: MemorySnapshot,
    ) -> MemorySnapshot {
        let nm = self.new_memory();
        let mut stored = memory.to_sexp();
        for i in 0..size {
            stored = Sexp::s4(
                "store",
                stored,
                bv_hex(addr + i, 8),
                Sexp::s2(
                    Sexp::s4(
                        "_",
                        "extract",
                        &*(i * 8 + 7).to_string(),
                        &*(i * 8).to_string(),
                    ),
                    "val",
                ),
            );
        }
        self.add_z3_line(define_const(
            nm,
            memory_ty(),
            Sexp::s3("let", Sexp::s1(Sexp::s2("val", o)), stored),
        ));
        nm
    }

    fn size_of_ty(&self, ty: &llvm_ir::TypeRef) -> usize {
        match &**ty {
            llvm_ir::Type::VoidType => 0,
            llvm_ir::Type::IntegerType { bits } => bit_to_byte(*bits as usize),
            llvm_ir::Type::FuncType { .. } => 8,
            _ => todo!(),
        }
    }

    fn size_of_operand(&self, operand: &llvm_ir::Operand) -> usize {
        match operand {
            llvm_ir::Operand::LocalOperand { name, ty } => self.size_of_ty(ty),
            llvm_ir::Operand::ConstantOperand(c) => match &**c {
                &llvm_ir::Constant::Int { bits, value } => bit_to_byte(bits as usize),
                _ => todo!(),
            },
            llvm_ir::Operand::MetadataOperand => todo!(),
        }
    }

    fn load_from_addr(&self, addr: usize, size: usize, memory: MemorySnapshot) -> Sexp {
        if size == 1 {
            return Sexp::s3("select", memory, bv_hex(addr, 8));
        }
        let mut r = vec!["concat".to_sexp()];
        for i in (0..size).rev() {
            r.push(Sexp::s3("select", memory, bv_hex(addr + i, 8)));
        }
        Sexp::List(r)
    }

    fn check_sat(mut self, sat_message: &str) {
        match &*self.goal {
            [] => {}
            [g] => self.add_z3_line(Sexp::s2("assert", Sexp::s2("not", g.clone()))),
            _ => todo!(),
        }
        self.add_z3_line(Sexp::s1("check-sat"));
        self.add_z3_line(Sexp::s1("get-model"));
        let mut f = std::fs::File::create("z3-query").unwrap();
        f.write_all(self.z3_state.as_bytes()).unwrap();
        let mut child = std::process::Command::new("bash")
            .arg("-c")
            .arg("z3 z3-query > z3-result")
            .spawn()
            .unwrap();
        child.wait().unwrap();
        let r = std::fs::read_to_string("z3-result").unwrap();
        if !r.starts_with("unsat") {
            if let Some(r) = r.strip_prefix("sat") {
                let r = r.trim();
                if let Some(r) = r.strip_prefix("(") {
                    if let Some(r) = r.strip_suffix(")") {
                        let mut f = std::fs::File::create("z3-model").unwrap();
                        f.write_all(r.as_bytes()).unwrap();
                        writeln!(f, r#"(echo "{sat_message}")"#).unwrap();
                        for x in &self.intersting_consts {
                            writeln!(f, r#"(echo "{x} is:") (simplify {x})"#).unwrap();
                        }
                        let mut child = std::process::Command::new("bash")
                            .arg("-c")
                            .arg("z3 z3-model > z3-model-simplified")
                            .spawn()
                            .unwrap();
                        child.wait().unwrap();
                        let r = std::fs::read_to_string("z3-model-simplified").unwrap();
                        panic!("{r}");
                    }
                }
            }
            panic!("{r}");
        }
    }

    fn address_of_name(&self, name: &llvm_ir::Name) -> usize {
        let mut la_map = self.local_addresses.borrow_mut();
        if let Some(x) = la_map.get(name) {
            return *x;
        }
        const ALLOCA_SIZE: usize = 1 << 32;
        let new_addr = (la_map.len() + 5) * ALLOCA_SIZE;
        la_map.insert(name.clone(), new_addr);
        new_addr
    }

    fn compare_calls(
        mut self,
        left_call: Call,
        right_call: Call,
        left_memory: MemorySnapshot,
        right_memory: MemorySnapshot,
    ) {
        if left_call.function_ty != right_call.function_ty {
            let left_ty = left_call.function_ty;
            let right_ty = right_call.function_ty;
            self.check_sat(&format!("Mismatched function call.\nLeft called function with signature {left_ty:?}\nRight called function with signature {right_ty}"));
            return;
        }
        self.add_interesting_compare(
            "function",
            bv_ty(64),
            self.operand_to_sexp(&left_call.function.right().unwrap(), left_memory),
            self.operand_to_sexp(&right_call.function.right().unwrap(), right_memory),
        );
        self.check_sat("Mismatched function or arguments");
    }
}

fn pos_of_bb_name(name: &llvm_ir::Name, left: &Function) -> Position {
    let bb = left
        .basic_blocks
        .iter()
        .position(|x| x.name == *name)
        .unwrap();
    Position { bb, instr: 0 }
}

fn main() {
    let m = Module::from_bc_path("./playground/playground.bc").unwrap();
    let mut left = None;
    let mut right = None;
    for function in m.functions {
        if function.name == "left" {
            left = Some(function.clone());
        }
        if function.name == "right" {
            right = Some(function);
        }
    }
    let verifier = VerifierState::new(left.unwrap(), right.unwrap());
    dbg!(verifier.compare_functions());
}
