#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    /// Adapted from libp2p's own test suite:
    /// https://github.com/sekrit-twc/libp2p/blob/5e65679ae54d0f9fa412ab36289eb2255e341625/test/api_test.cpp#L248
    #[test]
    fn p2p_one_fill() {
        let planar: [u8; 9] = [0x11, 0x12, 0x13, 0x21, 0x22, 0x23, 0x31, 0x32, 0x33];
        let mut packed: [u8; 12] = [0x00; 12];

        let params = p2p_buffer_param {
            src: [
                planar.as_ptr() as *const std::ffi::c_void,
                unsafe { planar.as_ptr().add(3) } as *const std::ffi::c_void,
                unsafe { planar.as_ptr().add(6) } as *const std::ffi::c_void,
                std::ptr::null(),
            ],
            dst: [
                packed.as_mut_ptr() as *mut std::ffi::c_void,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ],
            src_stride: [3, 3, 3, 0],
            dst_stride: [12, 0, 0, 0],
            width: 3,
            height: 1,
            packing: p2p_packing_p2p_argb32_be,
        };

        packed.copy_from_slice(&[0xaau8; 12]);
        unsafe {
            p2p_pack_frame(&params, 0);
        }
        assert_eq!(0, packed[0]);
        assert_eq!(0, packed[4]);
        assert_eq!(0, packed[8]);

        packed.copy_from_slice(&[0xaa; 12]);
        unsafe {
            p2p_pack_frame(&params, P2P_ALPHA_SET_ONE as u64);
        }
        assert_eq!(0xFF, packed[0]);
        assert_eq!(0xFF, packed[4]);
        assert_eq!(0xFF, packed[8]);
    }
}
