use crate::sexp::{Sexp, ToSexp};

pub fn declare_const(name: impl ToSexp, ty: impl ToSexp) -> Sexp {
    Sexp::s3("declare-const", name, ty)
}

pub fn define_const(name: impl ToSexp, ty: impl ToSexp, value: impl ToSexp) -> Sexp {
    Sexp::s4("define-const", name, ty, value)
}

pub fn if_then_else(
    condition: impl ToSexp,
    true_value: impl ToSexp,
    false_value: impl ToSexp,
) -> Sexp {
    Sexp::s4("ite", condition, true_value, false_value)
}

pub fn memory_ty() -> Sexp {
    Sexp::s3("Array", bv_ty(64), bv_ty(8))
}

pub fn bv_ty(arg: usize) -> Sexp {
    Sexp::s3("_", "BitVec", &*arg.to_string())
}

pub fn bv_hex(arg: usize, size: usize) -> Sexp {
    let mut r = format!("{:x}", arg);
    let size = size * 2;
    format!("#x{:0>size$}", r).to_sexp()
}

pub fn bit_to_byte(bits: usize) -> usize {
    (bits + 7) / 8
}
