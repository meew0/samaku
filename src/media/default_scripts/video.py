# This default script will load a video file using LWLibavSource.
# It requires the `lsmas` plugin.
# See ?data/automation/vapoursynth/aegisub_vs.py for more information.

import vapoursynth as vs
import time
import aegisub_vs as a
a.set_paths(locals())

clip, videoinfo = a.wrap_lwlibavsource(filename)
clip.set_output()
__aegi_timecodes = videoinfo["timecodes"]
__aegi_keyframes = videoinfo["keyframes"]

# Uncomment this line to make Aegisub look for a keyframes file for the video, or ask to detect keyframes on scene changes if no file was found.
# You can also change the GenKeyframesMode. Valid values are NEVER, ALWAYS, and ASK.
#__aegi_keyframes = a.get_keyframes(filename, clip, __aegi_keyframes, generate=a.GenKeyframesMode.ASK)

# Check if the file has an audio track. This requires the `bas` plugin.
__aegi_hasaudio = 1 if a.check_audio(filename) else 0
