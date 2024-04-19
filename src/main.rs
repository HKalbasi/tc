use std::{
    borrow::BorrowMut, cell::RefCell, collections::HashMap, fmt::Display, io::Write,
    os::unix::process::CommandExt,
};

use llvm_ir::{instruction::BinaryOp, Function, Module, Terminator};
use sexp::{Sexp, ToSexp};
use z3_decl::{bv_hex, bv_ty, declare_const, define_const, memory_ty};

mod sexp;
mod z3_decl;

struct VerifierState {
    local_addresses: RefCell<HashMap<llvm_ir::Name, usize>>,
    left: Function,
    right: Function,
    z3_state: String,
    memory_generator_counter: usize,
    intersting_consts: Vec<String>,
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
        self.z3_state += &arg.to_single_line();
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
        &mut self,
        left_bb: usize,
        right_bb: usize,
        left_memory: MemorySnapshot,
        right_memory: MemorySnapshot,
    ) -> bool {
        let left_memory = self.do_bb_statements(self.left.clone(), left_bb, left_memory);
        let right_memory = self.do_bb_statements(self.right.clone(), right_bb, right_memory);
        self.compare_bb_end(left_bb, right_bb, left_memory, right_memory)
    }

    fn compare_bb_end(
        &mut self,
        left_bb: usize,
        right_bb: usize,
        left_memory: MemorySnapshot,
        right_memory: MemorySnapshot,
    ) -> bool {
        let left_term = &self.left.basic_blocks[left_bb].term;
        let right_term = &self.right.basic_blocks[right_bb].term;
        match (left_term, right_term) {
            (Terminator::Ret(left_r), Terminator::Ret(right_r)) => {
                let z3_snapshot = self.z3_state.clone();
                let Some(left_op) = &left_r.return_operand else {
                    return true;
                };
                let Some(right_op) = &right_r.return_operand else {
                    return true;
                };
                let left_value = self.operand_to_sexp(left_op, left_memory);
                let right_value = self.operand_to_sexp(right_op, right_memory);
                self.add_z3_line(define_const("return_left", bv_ty(32), left_value));
                self.add_z3_line(define_const("return_right", bv_ty(32), right_value));
                self.add_z3_line(Sexp::s2(
                    "assert",
                    Sexp::s2("not", Sexp::s3("=", "return_left", "return_right")),
                ));
                self.intersting_consts.push("return_left".to_owned());
                self.intersting_consts.push("return_right".to_owned());
                self.check_sat();
                self.intersting_consts.pop();
                self.intersting_consts.pop();
                self.z3_state = z3_snapshot;
                true
            }
            _ => todo!(),
        }
    }

    fn do_bb_statements(
        &mut self,
        f: Function,
        bb: usize,
        mut memory: MemorySnapshot,
    ) -> MemorySnapshot {
        let bb = &f.basic_blocks[bb];
        for stmt in &bb.instrs {
            match stmt {
                llvm_ir::Instruction::Add(add) => {
                    let o0 = self.operand_to_sexp(&add.operand0, memory);
                    let o1 = self.operand_to_sexp(&add.operand1, memory);
                    let o = Sexp::s3("bvadd", o0, o1);
                    let addr = self.address_of_name(&add.dest);
                    let next_memory = self.store_in_addr(addr, 4, o, memory);
                    memory = next_memory;
                }
                llvm_ir::Instruction::Sub(sub) => {
                    let o0 = self.operand_to_sexp(&sub.operand0, memory);
                    let o1 = self.operand_to_sexp(&sub.operand1, memory);
                    let o = Sexp::s3("bvsub", o0, o1);
                    let addr = self.address_of_name(&sub.dest);
                    let next_memory = self.store_in_addr(addr, 4, o, memory);
                    memory = next_memory;
                }
                _ => todo!(),
            }
        }
        memory
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
                    bv_hex(value as usize, bits as usize / 8)
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
            llvm_ir::Type::IntegerType { bits } => (*bits / 8) as usize,
            _ => todo!(),
        }
    }

    fn load_from_addr(&self, addr: usize, size: usize, memory: MemorySnapshot) -> Sexp {
        let mut r = vec!["concat".to_sexp()];
        for i in (0..size).rev() {
            r.push(Sexp::s3("select", memory, bv_hex(addr + i, 8)));
        }
        Sexp::List(r)
    }

    fn check_sat(&mut self) {
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
                        dbg!(1);
                        let mut f = std::fs::File::create("z3-model").unwrap();
                        f.write_all(r.as_bytes()).unwrap();
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
        const ALLOCA_SIZE: usize = 1 << 30;
        let new_addr = (la_map.len() + 5) * ALLOCA_SIZE;
        la_map.insert(name.clone(), new_addr);
        new_addr
    }
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
