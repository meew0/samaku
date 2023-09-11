# This default script will load an audio file using BestAudioSource.
# It requires the `bas` plugin.

import vapoursynth as vs
import aegisub_vs as a
a.set_paths(locals())

a.ensure_plugin("bas", "BestAudioSource", "To use Aegisub's default audio loader, the `bas` plugin for VapourSynth must be installed")
vs.core.bas.Source(source=filename).set_output()
