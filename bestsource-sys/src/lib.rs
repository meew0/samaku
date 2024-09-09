#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use std::env;
    use std::ffi::CString;
    use std::path::Path;

    use super::*;

    #[test]
    fn audio_properties_and_decoding() {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let music_path = Path::new(&manifest_dir).join("../test_files/music.mp3");
        if !music_path.exists() {
            panic!("Could not find test data (test_files/music.mp3)! Perhaps some relative-path problem?");
        }

        unsafe {
            let path: CString = CString::new(format!("{}", music_path.display())).unwrap();
            let empty: CString = CString::new("").unwrap();

            unsafe extern "C" fn progress(_track: i32, _current: i64, _total: i64) -> i32 {
                1
            }

            let maybe_source = BestAudioSource_new(
                path.as_ptr(),
                -1,
                -1,
                0,
                0,
                0,
                empty.as_ptr(),
                0.0,
                Some(progress),
            );

            assert_eq!(
                maybe_source.error, 0,
                "there should be no error while constructing the BestAudioSource"
            );
            assert!(!maybe_source.value.is_null());

            let source = maybe_source.value;
            let properties = BestAudioSource_GetAudioProperties(source);

            assert_eq!(
                properties.error, 0,
                "there should be no error while getting the AudioProperties"
            );

            assert_eq!(properties.SampleRate, 44100);
            assert_eq!(properties.Channels, 2);

            let mut slice = vec![0u8; 256 * properties.AF.BytesPerSample as usize];
            let error = BestAudioSource_GetPackedAudio(source, slice.as_mut_ptr(), 88200, 128);
            assert_eq!(
                error, 0,
                "there should be no error while getting packed audio"
            );
            assert_ne!(
                slice,
                vec![0; 256 * properties.AF.BytesPerSample as usize],
                "there should be some audio data"
            );

            let error = BestAudioSource_delete(source);

            assert_eq!(
                error, 0,
                "there should be no error while destroying the BestAudioSource"
            );
        }
    }

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
