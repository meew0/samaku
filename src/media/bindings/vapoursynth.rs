#![allow(
    dead_code,
    reason = "implements more of what VS does for now than is currently used in samaku"
)]

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::marker::PhantomData;
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use rustsynth_sys as vs;

use crate::model;

use super::c_string;

fn vs_assert(ret: i32, message: &str) {
    assert!(ret <= 0, "{}", message);
}

static SCRIPTAPI: AtomicPtr<vs::VSSCRIPTAPI> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<vs::VSAPI> = AtomicPtr::new(ptr::null_mut());

fn get_script_api() -> *const vs::VSSCRIPTAPI {
    let ptr = SCRIPTAPI.load(Ordering::Relaxed);

    if ptr.is_null() {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "constant defined by VS, does not matter if it wraps"
        )]
        let vs_api_version = vs::VSSCRIPT_API_VERSION as i32;

        let new_ptr = unsafe { vs::getVSScriptAPI(vs_api_version).cast_mut() };
        assert!(!new_ptr.is_null(), "Failed to initialise VSScriptAPI");

        SCRIPTAPI.store(new_ptr, Ordering::Relaxed);
        new_ptr
    } else {
        ptr
    }
}

fn get_api() -> *const vs::VSAPI {
    let ptr = API.load(Ordering::Relaxed);

    if ptr.is_null() {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "constant defined by VS, does not matter if it wraps"
        )]
        let vs_api_version = vs::VAPOURSYNTH_API_VERSION as i32;

        let script_api = get_script_api();
        let new_ptr = unsafe { (*script_api).getVSAPI.unwrap()(vs_api_version).cast_mut() };
        assert!(!new_ptr.is_null(), "Failed to initialise VSAPI");

        API.store(new_ptr, Ordering::Relaxed);
        new_ptr
    } else {
        ptr
    }
}

pub(crate) type LogHandler = dyn Fn(i32, &str);

unsafe extern "C" fn log_handler(msg_type: c_int, msg: *const c_char, user_data: *mut c_void) {
    let log_handler: *mut Box<LogHandler> = user_data.cast::<Box<LogHandler>>();
    let rust_str: &str = unsafe {
        CStr::from_ptr(msg)
            .to_str()
            .expect("the log message received from VapourSynth should be valid UTF-8")
    };
    unsafe {
        (*log_handler)(msg_type, rust_str);
    }
}

unsafe extern "C" fn log_handler_free(user_data: *mut c_void) {
    let log_handler: *mut Box<LogHandler> = user_data.cast::<Box<LogHandler>>();
    let data: Box<Box<LogHandler>> = unsafe { Box::from_raw(log_handler) };
    drop(data);
}

pub(crate) struct Core {
    core: *mut vs::VSCore,
}

impl Core {
    pub(crate) fn create_core(flags: i32) -> Option<Core> {
        let api = get_api();
        let core = unsafe { (*api).createCore.unwrap()(flags) };
        if core.is_null() {
            None
        } else {
            Some(Core { core })
        }
    }

    pub(crate) fn create_script(&self) -> Option<Script> {
        self.check_null();
        let script_api = get_script_api();
        let script = unsafe { (*script_api).createScript.unwrap()(self.core) };
        if script.is_null() {
            None
        } else {
            Some(Script { script })
        }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "argument only contains a pointer"
    )]
    fn get_plugin_by_id(&self, identifier: CString) -> Option<Plugin> {
        self.check_null();
        let api = get_api();
        let plugin = unsafe { (*api).getPluginByID.unwrap()(identifier.as_ptr(), self.core) };
        if plugin.is_null() {
            None
        } else {
            Some(Plugin { plugin })
        }
    }

    pub(crate) fn get_resize_plugin(&self) -> Option<Plugin> {
        self.get_plugin_by_id(CString::new("com.vapoursynth.resize").unwrap())
    }

    pub(crate) fn add_log_handler<H: Fn(i32, &str) + 'static>(&mut self, handler: H) -> LogHandle {
        self.check_null();
        let api = get_api();

        // We cannot use a simple box because a `*mut LogHandler` is a fat pointer
        // which cannot be transmuted to a `*mut c_void`
        let data: Box<Box<LogHandler>> = Box::new(Box::new(handler));
        let data_ptr: *mut Box<LogHandler> = Box::into_raw(data);
        let log_handle = unsafe {
            (*api).addLogHandler.unwrap()(
                Some(log_handler),
                Some(log_handler_free),
                data_ptr.cast::<c_void>(),
                self.core,
            )
        };

        LogHandle { handle: log_handle }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "handle only contains a pointer"
    )]
    pub(crate) fn remove_log_handler(&mut self, handle: LogHandle) {
        self.check_null();
        let api = get_api();
        unsafe {
            (*api).removeLogHandler.unwrap()(handle.handle, self.core);
        }
    }

    pub(crate) fn free(&mut self) {
        let api = get_api();
        unsafe {
            (*api).freeCore.unwrap()(self.core);
        }
        self.core = ptr::null_mut();
    }

    fn check_null(&self) {
        assert!(!self.core.is_null(), "Tried to access freed core");
    }
}

pub(crate) struct Script {
    script: *mut vs::VSScript,
}

impl Script {
    pub(crate) fn get_core(&self) -> Core {
        let script_api = get_script_api();
        Core {
            core: unsafe { (*script_api).getCore.unwrap()(self.script) },
        }
    }

    pub(crate) fn eval_set_working_dir(&mut self, set_cwd: i32) {
        let script_api = get_script_api();
        unsafe {
            (*script_api).evalSetWorkingDir.unwrap()(self.script, set_cwd);
        }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "argument only contains a pointer"
    )]
    pub(crate) fn evaluate_file(&mut self, script_filename: CString) -> Result<(), i32> {
        let script_api = get_script_api();
        let ret =
            unsafe { (*script_api).evaluateFile.unwrap()(self.script, script_filename.as_ptr()) };
        if ret > 0 {
            self.print_error();
            Err(ret)
        } else {
            Ok(())
        }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "arguments only contain pointers"
    )]
    pub(crate) fn evaluate_buffer(
        &mut self,
        buffer: CString,
        script_filename: CString,
    ) -> Result<(), i32> {
        let script_api = get_script_api();
        let ret = unsafe {
            (*script_api).evaluateBuffer.unwrap()(
                self.script,
                buffer.as_ptr(),
                script_filename.as_ptr(),
            )
        };
        if ret > 0 {
            self.print_error();
            Err(ret)
        } else {
            Ok(())
        }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "argument only contains a pointer"
    )]
    pub(crate) fn get_variable(&self, name: CString, dst: &MutMap) {
        let script_api = get_script_api();
        let ret =
            unsafe { (*script_api).getVariable.unwrap()(self.script, name.as_ptr(), dst.map) };
        vs_assert(ret, "Script variable retrieval failed");
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "argument only contains a pointer"
    )]
    pub(crate) fn set_variables(&mut self, vars: ConstMap) {
        let script_api = get_script_api();
        let ret = unsafe { (*script_api).setVariables.unwrap()(self.script, vars.map) };
        vs_assert(ret, "Script variable setting failed");
    }

    pub(crate) fn get_output_node(&self, index: i32) -> Option<Node> {
        let script_api = get_script_api();
        let node = unsafe { (*script_api).getOutputNode.unwrap()(self.script, index) };
        if node.is_null() {
            None
        } else {
            Some(Node { node })
        }
    }

    fn print_error(&self) {
        let script_api = get_script_api();
        let error = unsafe { (*script_api).getError.unwrap()(self.script) };
        if !error.is_null() {
            println!(
                "Script error: {}",
                unsafe { CStr::from_ptr(error) }.to_str().unwrap()
            );
        }
    }
}

unsafe impl Send for Script {}

impl Drop for Script {
    fn drop(&mut self) {
        let script_api = get_script_api();
        unsafe {
            (*script_api).freeScript.unwrap()(self.script);
        }
    }
}

pub(crate) struct LogHandle {
    handle: *mut vs::VSLogHandle,
}

mod private {
    pub(crate) trait MapPtr {}
}

type MapConstPtr = *const vs::VSMap;
type MapMutPtr = *mut vs::VSMap;

impl private::MapPtr for MapConstPtr {}

impl private::MapPtr for MapMutPtr {}

pub(crate) struct Map<'a, P: private::MapPtr> {
    map: P,
    _a: PhantomData<&'a vs::VSMap>,
}

pub(crate) type ConstMap<'a> = Map<'a, MapConstPtr>;
pub(crate) type MutMap<'a> = Map<'a, MapMutPtr>;

impl ConstMap<'_> {
    pub(crate) fn get_int(&self, key: &CStr, index: i32) -> Result<i64, i32> {
        let api = get_api();
        let mut err: i32 = 0;
        let res = unsafe { (*api).mapGetInt.unwrap()(self.map, key.as_ptr(), index, &raw mut err) };
        if err > 0 {
            Err(err)
        } else {
            Ok(res)
        }
    }

    pub(crate) fn get_node(&self, key: &CStr, index: i32) -> Result<Node, i32> {
        let api = get_api();
        let mut err: i32 = 0;
        let node =
            unsafe { (*api).mapGetNode.unwrap()(self.map, key.as_ptr(), index, &raw mut err) };
        if err > 0 {
            Err(err)
        } else {
            Ok(Node { node })
        }
    }

    pub(crate) fn get_int_array(&self, variable: &CStr) -> Result<Vec<i64>, i32> {
        let len = self.num_elements(variable);
        let api = get_api();
        let mut err: i32 = 0;
        let res: *const i64 =
            unsafe { (*api).mapGetIntArray.unwrap()(self.map, variable.as_ptr(), &raw mut err) };
        if err > 0 {
            Err(err)
        } else {
            let slice: &[i64] = unsafe { std::slice::from_raw_parts(res, len) };
            let mut vec = Vec::new();
            vec.extend_from_slice(slice);
            Ok(vec)
        }
    }
    pub(crate) fn get_data(&self, variable: &CStr, index: i32) -> Result<Vec<u8>, i32> {
        let api = get_api();
        let mut err: i32 = 0;
        let res: *const u8 = unsafe {
            (*api).mapGetData.unwrap()(self.map, variable.as_ptr(), index, &raw mut err)
                .cast::<u8>()
        };
        if err > 0 {
            return Err(err);
        }
        let len: usize = unsafe {
            (*api).mapGetDataSize.unwrap()(self.map, variable.as_ptr(), index, &raw mut err)
        }
        .try_into()
        .expect("map data size should not be negative");
        if err > 0 {
            Err(err)
        } else {
            let slice: &[u8] = unsafe { std::slice::from_raw_parts(res, len) };
            let mut vec = Vec::new();
            vec.extend_from_slice(slice);
            Ok(vec)
        }
    }

    pub(crate) fn get_error(&self) -> Option<String> {
        let api = get_api();
        let buf = unsafe { (*api).mapGetError.unwrap()(self.map) };

        if buf.is_null() {
            None
        } else {
            let mut string = String::new();
            string.push_str(unsafe { CStr::from_ptr(buf) }.to_str().unwrap());
            Some(string)
        }
    }

    pub(crate) fn num_elements(&self, variable: &CStr) -> usize {
        let api = get_api();
        unsafe { (*api).mapNumElements.unwrap()(self.map, variable.as_ptr()) }
            .try_into()
            .expect("num_elements result should not be negative")
    }

    fn into_ptr(self) -> MapConstPtr {
        self.map
    }
}

impl<'a> MutMap<'a> {
    pub(crate) fn set_utf8(&mut self, key: &CStr, value: &CStr) {
        let api = get_api();
        let ret = unsafe {
            (*api).mapSetData.unwrap()(
                self.map,
                key.as_ptr(),
                value.as_ptr(),
                -1,
                vs::VSDataTypeHint::dtUtf8 as i32,
                1,
            )
        };
        vs_assert(ret, "Map data setting failed");
    }

    pub(crate) fn set_path<P: AsRef<Path>>(&mut self, key: &CStr, value: P) {
        // TODO: technically, we are reinterpreting arbitrary bytes as UTF-8 here
        self.set_utf8(key, super::path_to_cstring(value).as_c_str());
    }

    pub(crate) fn append_int(&mut self, key: &CStr, value: i64) {
        let api = get_api();
        let ret = unsafe {
            (*api).mapSetInt.unwrap()(
                self.map,
                key.as_ptr(),
                value,
                vs::VSMapAppendMode::maAppend as i32,
            )
        };
        vs_assert(ret, "Map int setting failed");
    }

    pub(crate) fn append_node(&mut self, key: &CStr, value: &Node) {
        let api = get_api();
        let ret = unsafe {
            (*api).mapSetNode.unwrap()(
                self.map,
                key.as_ptr(),
                value.node,
                vs::VSMapAppendMode::maAppend as i32,
            )
        };
        vs_assert(ret, "Map node setting failed");
    }

    pub(crate) fn as_const(&self) -> ConstMap<'a> {
        ConstMap {
            map: self.map,
            _a: PhantomData,
        }
    }
}

pub(crate) struct OwnedMap<'a> {
    map: MutMap<'a>,
}

impl OwnedMap<'_> {
    pub(crate) fn create_map() -> Option<OwnedMap<'static>> {
        let api = get_api();
        let map = unsafe { (*api).createMap.unwrap()() };
        if map.is_null() {
            None
        } else {
            Some(OwnedMap {
                map: MutMap {
                    map,
                    _a: PhantomData,
                },
            })
        }
    }
}

impl<'a> AsMut<MutMap<'a>> for OwnedMap<'a> {
    fn as_mut(&mut self) -> &mut MutMap<'a> {
        &mut self.map
    }
}

impl Drop for OwnedMap<'_> {
    fn drop(&mut self) {
        let api = get_api();
        unsafe {
            (*api).freeMap.unwrap()(self.map.map);
        }
    }
}

pub(crate) struct Node {
    node: *mut vs::VSNode,
}

impl Node {
    fn get_node_type(&self) -> i32 {
        let api = get_api();
        unsafe { (*api).getNodeType.unwrap()(self.node) }
    }

    pub(crate) fn is_video(&self) -> bool {
        self.get_node_type() == vs::VSMediaType::mtVideo as i32
    }

    pub(crate) fn is_audio(&self) -> bool {
        self.get_node_type() == vs::VSMediaType::mtAudio as i32
    }

    pub(crate) fn get_video_info(&self) -> Option<VideoInfo<'_>> {
        let api = get_api();
        let vi = unsafe { (*api).getVideoInfo.unwrap()(self.node) };
        if vi.is_null() {
            None
        } else {
            Some(VideoInfo {
                vi,
                _a: PhantomData,
            })
        }
    }

    pub(crate) fn get_audio_info(&self) -> Option<AudioInfo<'_>> {
        let api = get_api();
        let ai = unsafe { (*api).getAudioInfo.unwrap()(self.node) };
        if ai.is_null() {
            None
        } else {
            Some(AudioInfo {
                ai,
                _a: PhantomData,
            })
        }
    }

    pub(crate) fn get_frame(&self, n: i32) -> Result<Frame, String> {
        let api = get_api();
        let error_len: u16 = 1024;
        let mut error_buf: Box<[u8]> = vec![0; error_len.into()].into_boxed_slice();
        let frame = unsafe {
            (*api).getFrame.unwrap()(
                n,
                self.node,
                error_buf.as_mut_ptr().cast::<i8>(),
                error_len.into(),
            )
        };

        if frame.is_null() {
            let cstr = CStr::from_bytes_until_nul(&error_buf).unwrap();
            Err(cstr.to_owned().into_string().unwrap())
        } else {
            Ok(Frame { frame })
        }
    }
}

unsafe impl Send for Node {}

impl Drop for Node {
    fn drop(&mut self) {
        let api = get_api();
        unsafe {
            (*api).freeNode.unwrap()(self.node);
        }
    }
}

pub(crate) struct VideoInfo<'a> {
    vi: *const vs::VSVideoInfo,
    _a: PhantomData<&'a vs::VSVideoInfo>,
}

impl VideoInfo<'_> {
    pub(crate) fn is_constant_video_format(&self) -> bool {
        // The VSHelper functions don't seem to have been included in rustsynth-sys.
        // Fortunately, it's easy enough to implement
        unsafe {
            (*self.vi).height > 0
                && (*self.vi).width > 0
                && (*self.vi).format.colorFamily != vs::VSColorFamily::cfUndefined as i32
        }
    }

    pub(crate) fn is_rgb24(&self) -> bool {
        unsafe {
            (*self.vi).format.colorFamily == vs::VSColorFamily::cfRGB as i32
                && (*self.vi).format.bitsPerSample == 8
        }
    }

    pub(crate) fn get_width(&self) -> i32 {
        unsafe { *self.vi }.width
    }

    pub(crate) fn get_height(&self) -> i32 {
        unsafe { *self.vi }.height
    }

    fn get_color_family(&self) -> i32 {
        unsafe { *self.vi }.format.colorFamily
    }

    pub(crate) fn get_frame_rate(&self) -> FrameRate {
        FrameRate {
            numerator: unsafe { (*self.vi).fpsNum }
                .try_into()
                .expect("frame rate numerator should not be negative"),
            denominator: unsafe { (*self.vi).fpsDen }
                .try_into()
                .expect("frame rate denominator should not be negative"),
        }
    }
}

pub(crate) const AUDIO_FRAME_SAMPLES: u32 = vs::VS_AUDIO_FRAME_SAMPLES;

pub(crate) struct AudioInfo<'a> {
    ai: *const vs::VSAudioInfo,
    _a: PhantomData<&'a vs::VSAudioInfo>,
}

impl AudioInfo<'_> {
    pub(crate) fn float_samples(&self) -> bool {
        unsafe { (*self.ai).format.sampleType == vs::VSSampleType::stFloat as i32 }
    }

    pub(crate) fn get_bytes_per_sample(&self) -> i32 {
        unsafe { (*self.ai).format.bytesPerSample }
    }

    pub(crate) fn get_sample_rate(&self) -> i32 {
        unsafe { (*self.ai).sampleRate }
    }

    pub(crate) fn get_num_channels(&self) -> i32 {
        unsafe { (*self.ai).format.numChannels }
    }

    pub(crate) fn get_num_samples(&self) -> i64 {
        unsafe { (*self.ai).numSamples }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FrameRate {
    pub numerator: u64,
    pub denominator: u64,
}

impl FrameRate {
    /// Get the number of the closest frame before the given time point in milliseconds.
    ///
    /// # Panics
    /// Panics if the resulting frame number would not fit into an `i32`.
    #[must_use]
    pub(crate) fn ms_to_frame(&self, ass_ms: i64) -> model::FrameNumber {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "numerator is guaranteed to be smaller than i64 max"
        )]
        let numerator = ass_ms * self.numerator as i64;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "denominator is guaranteed to be smaller than i64 max"
        )]
        let denominator = 1000 * self.denominator as i64;
        model::FrameNumber(
            (numerator / denominator)
                .try_into()
                .expect("overflow while converting time to frame number"),
        )
    }

    #[must_use]
    pub(crate) fn frame_to_ms(&self, frame: model::FrameNumber) -> i64 {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "denominator is guaranteed to be smaller than i64 max"
        )]
        let inv_numerator = i64::from(frame.0 * 1000) * self.denominator as i64;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "numerator is guaranteed to be smaller than i64 max"
        )]
        let result = inv_numerator / self.numerator as i64;
        result
    }

    #[must_use]
    pub(crate) fn frame_time_ms(&self) -> i64 {
        self.frame_to_ms(model::FrameNumber(1))
    }
}

impl From<FrameRate> for f64 {
    /// Convert the frame rate to a floating-point value by dividing the numerator by the
    /// denominator. May lose precision for very large numerators/denominators.
    #[expect(
        clippy::cast_precision_loss,
        reason = "amount of precision loss is acceptable in this case"
    )]
    fn from(value: FrameRate) -> Self {
        value.numerator as f64 / value.denominator as f64
    }
}

pub(crate) struct Frame {
    frame: *const vs::VSFrame,
}

impl Frame {
    pub(crate) fn get_properties_ro(&self) -> Option<ConstMap<'_>> {
        let api = get_api();
        let map = unsafe { (*api).getFramePropertiesRO.unwrap()(self.frame) };
        if map.is_null() {
            None
        } else {
            Some(ConstMap {
                map,
                _a: PhantomData,
            })
        }
    }

    pub(crate) fn get_video_format(&self) -> Option<VideoFormat<'_>> {
        let api = get_api();
        let vf = unsafe { (*api).getVideoFrameFormat.unwrap()(self.frame) };
        if vf.is_null() {
            None
        } else {
            Some(VideoFormat {
                vf,
                _a: PhantomData,
            })
        }
    }

    pub(crate) fn get_width(&self, plane: i32) -> i32 {
        let api = get_api();
        unsafe { (*api).getFrameWidth.unwrap()(self.frame, plane) }
    }

    pub(crate) fn get_height(&self, plane: i32) -> i32 {
        let api = get_api();
        unsafe { (*api).getFrameHeight.unwrap()(self.frame, plane) }
    }

    pub(crate) fn get_stride(&self, plane: i32) -> usize {
        let api = get_api();
        unsafe { (*api).getStride.unwrap()(self.frame, plane) }
            .try_into()
            .expect("stride should be positive")
    }

    pub(crate) fn get_read_ptr(&self, plane: i32) -> &[u8] {
        let api = get_api();
        let ptr: *const u8 = unsafe { (*api).getReadPtr.unwrap()(self.frame, plane) };
        let len: usize = usize::try_from(self.get_height(plane))
            .expect("frame height should be positive")
            * self.get_stride(plane);
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        let api = get_api();
        unsafe {
            (*api).freeFrame.unwrap()(self.frame);
        }
    }
}

pub(crate) struct VideoFormat<'a> {
    vf: *const vs::VSVideoFormat,
    _a: PhantomData<&'a vs::VSVideoFormat>,
}

impl VideoFormat<'_> {
    pub(crate) fn is_rgb24(&self) -> bool {
        let deref = unsafe { *self.vf };
        deref.colorFamily == vs::VSColorFamily::cfRGB as i32
            && deref.numPlanes == 3
            && deref.bitsPerSample == 8
            && deref.subSamplingW == 0
            && deref.subSamplingH == 0
    }

    pub(crate) fn get_num_planes(&self) -> i32 {
        unsafe { (*self.vf).numPlanes }
    }
}

pub(crate) struct Plugin {
    plugin: *mut vs::VSPlugin,
}

impl Plugin {
    pub(crate) fn invoke(&'_ mut self, name: &CStr, args: ConstMap) -> OwnedMap<'_> {
        let api = get_api();
        let map = unsafe { (*api).invoke.unwrap()(self.plugin, name.as_ptr(), args.into_ptr()) };
        OwnedMap {
            map: MutMap {
                map,
                _a: PhantomData,
            },
        }
    }
}

pub(crate) fn color_matrix_description(vi: &VideoInfo, props: &ConstMap) -> String {
    let color_family = vi.get_color_family();
    if color_family != vs::VSColorFamily::cfYUV as i32 {
        return "None".to_owned();
    }

    let range = props
        .get_int(CString::new("_ColorRange").unwrap().as_c_str(), 0)
        .unwrap_or(-1);
    let matrix = props
        .get_int(CString::new("_Matrix").unwrap().as_c_str(), 0)
        .unwrap_or(-1);

    if matrix == vs::VSMatrixCoefficients::VSC_MATRIX_RGB as i64 {
        return "None".to_owned();
    }

    let mut ret = if range == vs::VSColorRange::VSC_RANGE_FULL as i64 {
        "PC".to_owned()
    } else {
        "TV".to_owned()
    };

    if matrix == vs::VSMatrixCoefficients::VSC_MATRIX_BT709 as i64 {
        ret.push_str(".709");
    } else if matrix == vs::VSMatrixCoefficients::VSC_MATRIX_FCC as i64 {
        ret.push_str(".FCC");
    } else if matrix == vs::VSMatrixCoefficients::VSC_MATRIX_ST170_M as i64
        || matrix == vs::VSMatrixCoefficients::VSC_MATRIX_BT470_BG as i64
    {
        ret.push_str(".601");
    } else if matrix == vs::VSMatrixCoefficients::VSC_MATRIX_ST240_M as i64 {
        ret.push_str(".240M");
    } else {
        return "Unknown".to_owned();
    }

    ret
}

pub(crate) fn init_resize(vi: &VideoInfo, args: &mut MutMap, props: &ConstMap) {
    args.append_int(
        CString::new("format").unwrap().as_c_str(),
        vs::VSPresetFormat::pfRGB24 as i64,
    );

    if vi.get_color_family() != vs::VSColorFamily::cfGray as i32
        && !props
            .get_int(CString::new("_Matrix").unwrap().as_c_str(), 0)
            .is_ok_and(|x| x != vs::VSMatrixCoefficients::VSC_MATRIX_UNSPECIFIED as i64)
    {
        args.append_int(
            CString::new("matrix_in").unwrap().as_c_str(),
            vs::VSMatrixCoefficients::VSC_MATRIX_BT709 as i64,
        );
    }

    if !props
        .get_int(CString::new("_Transfer").unwrap().as_c_str(), 0)
        .is_ok_and(|x| x != vs::VSTransferCharacteristics::VSC_TRANSFER_UNSPECIFIED as i64)
    {
        args.append_int(
            CString::new("transfer_in").unwrap().as_c_str(),
            vs::VSTransferCharacteristics::VSC_TRANSFER_BT709 as i64,
        );
    }

    if !props
        .get_int(CString::new("_Primaries").unwrap().as_c_str(), 0)
        .is_ok_and(|x| x != vs::VSColorPrimaries::VSC_PRIMARIES_UNSPECIFIED as i64)
    {
        args.append_int(
            CString::new("primaries_in").unwrap().as_c_str(),
            vs::VSColorPrimaries::VSC_PRIMARIES_BT709 as i64,
        );
    }

    if !props
        .get_int(CString::new("_ColorRange").unwrap().as_c_str(), 0)
        .is_ok_and(|x| x != -1_i64)
    {
        args.append_int(CString::new("range_in").unwrap().as_c_str(), 0_i64);
    }

    if !props
        .get_int(CString::new("_ChromaLocation").unwrap().as_c_str(), 0)
        .is_ok_and(|x| x != -1_i64)
    {
        args.append_int(
            CString::new("chromaloc_in").unwrap().as_c_str(),
            vs::VSChromaLocation::VSC_CHROMA_LEFT as i64,
        );
    }
}

const PRELUDE: &str = include_str!("../default_scripts/prelude.py");

pub(crate) fn open_script<P: AsRef<Path>>(script_code: &str, filename: P) -> Option<Script> {
    let mut core = Core::create_core(0).unwrap();
    let Some(mut script) = core.create_script() else {
        // This matches how it's done in aegi, where a core is only ever specifically freed
        // when script creation fails. Doing it otherwise leads to “double free of core”
        // errors.
        core.free();
        return None;
    };
    script.eval_set_working_dir(1);

    let handle = core.add_log_handler(|msg_type, msg| println!("[VapourSynth] {msg_type} - {msg}"));

    let mut map_owned = OwnedMap::create_map().unwrap();
    let map = map_owned.as_mut();
    map.set_path(c_string("filename").as_c_str(), filename);
    map.set_path(
        c_string("__aegi_vscache").as_c_str(),
        Path::new("./vscache/"),
    );
    map.set_utf8(
        c_string("__aegi_vsplugins").as_c_str(),
        c_string("").as_c_str(),
    );
    map.set_path(
        c_string("__samaku_vapoursynth_path").as_c_str(),
        std::fs::canonicalize(Path::new("./vapoursynth")).unwrap(),
    );
    // TODO: user paths
    script.set_variables(map.as_const());

    let mut vs_script_code = String::from(PRELUDE);
    vs_script_code.push_str(script_code);

    if let Some(error_code) = script
        .evaluate_buffer(c_string(vs_script_code), c_string("samaku"))
        .err()
    {
        println!("Failed to evaluate buffer: error code {error_code}");
        return None;
    }

    core.remove_log_handler(handle);

    Some(script)
}
