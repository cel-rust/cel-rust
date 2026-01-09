// Generated protobuf code
pub mod cel {
    pub mod expr {
        include!(concat!(env!("OUT_DIR"), "/cel.expr.rs"));
        pub mod conformance {
            pub mod test {
                include!(concat!(env!("OUT_DIR"), "/cel.expr.conformance.test.rs"));
            }
            pub mod proto2 {
                include!(concat!(env!("OUT_DIR"), "/cel.expr.conformance.proto2.rs"));
            }
            pub mod proto3 {
                include!(concat!(env!("OUT_DIR"), "/cel.expr.conformance.proto3.rs"));
            }
        }
    }
}
