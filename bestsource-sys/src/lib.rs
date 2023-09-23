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
            let maybe_source = BestAudioSource_new(path.as_ptr(), -1, -1, 0, empty.as_ptr(), 0.0);

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

            let mut slice = vec![0u8; 256 * properties.BytesPerSample as usize];
            let error = BestAudioSource_GetPackedAudio(source, slice.as_mut_ptr(), 88200, 128);
            assert_eq!(
                error, 0,
                "there should be no error while getting packed audio"
            );
            assert_ne!(
                slice,
                vec![0; 256 * properties.BytesPerSample as usize],
                "there should be some audio data"
            );

            let error = BestAudioSource_delete(source);

            assert_eq!(
                error, 0,
                "there should be no error while destroying the BestAudioSource"
            );
        }
    }
}
