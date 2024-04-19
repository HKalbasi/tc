pub enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

impl Sexp {
    pub fn s1(i1: impl ToSexp) -> Self {
        Self::List(vec![i1.to_sexp()])
    }

    pub fn s2(i1: impl ToSexp, i2: impl ToSexp) -> Self {
        Self::List(vec![i1.to_sexp(), i2.to_sexp()])
    }

    pub fn s3(i1: impl ToSexp, i2: impl ToSexp, i3: impl ToSexp) -> Self {
        Self::List(vec![i1.to_sexp(), i2.to_sexp(), i3.to_sexp()])
    }

    pub fn s4(i1: impl ToSexp, i2: impl ToSexp, i3: impl ToSexp, i4: impl ToSexp) -> Self {
        Self::List(vec![i1.to_sexp(), i2.to_sexp(), i3.to_sexp(), i4.to_sexp()])
    }

    pub fn to_single_line(self) -> String {
        match self {
            Sexp::Atom(x) => x,
            Sexp::List(l) => {
                let mut r = "(".to_owned();
                for i in l {
                    r += &i.to_single_line();
                    r += " ";
                }
                r.pop();
                r += ")";
                r
            }
        }
    }
}

pub trait ToSexp {
    fn to_sexp(self) -> Sexp;
}

impl ToSexp for &str {
    fn to_sexp(self) -> Sexp {
        Sexp::Atom(self.to_owned())
    }
}

impl ToSexp for Sexp {
    fn to_sexp(self) -> Sexp {
        self
    }
}
