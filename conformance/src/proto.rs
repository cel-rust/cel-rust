use prost_reflect::DescriptorPool;
use std::sync::OnceLock;

const FILE_DESCRIPTOR_SET_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/gen/file_descriptor_set.bin"
));

pub(crate) fn descriptor_pool() -> &'static DescriptorPool {
    static DESCRIPTOR_POOL: OnceLock<DescriptorPool> = OnceLock::new();
    DESCRIPTOR_POOL.get_or_init(|| {
        DescriptorPool::decode(FILE_DESCRIPTOR_SET_BYTES).expect("Failed to decode descriptor pool")
    })
}

// Generated protobuf code
#[allow(dead_code)]
#[allow(clippy::all)]
pub mod cel {
    pub mod expr {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/gen/cel.expr.rs"));
        pub mod conformance {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/gen/cel.expr.conformance.rs"
            ));
            pub mod test {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/gen/cel.expr.conformance.test.rs"
                ));
            }
            pub mod proto2 {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/gen/cel.expr.conformance.proto2.rs"
                ));
            }
            pub mod proto3 {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/gen/cel.expr.conformance.proto3.rs"
                ));
            }
        }
    }
}
