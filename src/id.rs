macro_rules! indexable_id {
    ($vis:vis $name:ident) => {
        #[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Debug)]
        $vis struct $name(usize);

        impl $name {
            pub fn new(index: usize) -> Self {
                Self(index)
            }

            pub fn index(&self) -> usize {
                self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl ::std::ops::Deref for $name {
            type Target = usize;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

macro_rules! impl_next_id {
    ($struct:ident $(<$($lt:lifetime),+>)?.$field:ident -> $name:ident) => {
        impl$(<$($lt),+>)? $struct$(<$($lt),+>)? {
            fn next_id(&mut self) -> $name {
                let id = self.$field;
                self.$field += 1;
                $name::new(id)
            }
        }
    };
    ($struct:ident $(<$($lt:lifetime),+>)?.$field:ident -> $name:ident, $method_name:ident) => {
        impl$(<$($lt),+>)? $struct$(<$($lt),+>)? {
            fn $method_name(&mut self) -> $name {
                let id = self.$field;
                self.$field += 1;
                $name::new(id)
            }
        }
    };
}
