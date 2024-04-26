use pretty::{Doc, RcDoc};

#[derive(Debug, Clone)]
pub enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

impl Sexp {
    /// Return a pretty printed format of self.
    pub fn to_doc(&self) -> RcDoc<()> {
        match *self {
            Self::Atom(ref x) => RcDoc::as_string(x),
            Self::List(ref xs) => RcDoc::text("(")
                .append(
                    RcDoc::intersperse(xs.into_iter().map(|x| x.to_doc()), Doc::line())
                        .nest(1)
                        .group(),
                )
                .append(RcDoc::text(")")),
        }
    }

    pub fn to_pretty(&self, width: usize) -> String {
        let mut w = Vec::new();
        self.to_doc().render(width, &mut w).unwrap();
        String::from_utf8(w).unwrap()
    }
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
