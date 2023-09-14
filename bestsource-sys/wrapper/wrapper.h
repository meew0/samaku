#ifdef __cplusplus
extern "C"
{
#endif

#include <stddef.h>
#include <stdint.h>

    struct BSW_IntWithError
    {
        int error;
        int value;
    };

    struct BSW_DoubleWithError
    {
        int error;
        double value;
    };

    struct BSW_PointerWithError
    {
        int error;
        void *value;
    };

    struct BestAudioSource_AudioProperties
    {
        int error;
        int IsFloat;
        int BytesPerSample;
        int BitsPerSample;
        int SampleRate;
        int Channels;
        uint64_t ChannelLayout;
        int64_t NumSamples; /* estimated by decoder, may be wrong */
        double StartTime;   /* in seconds */
    };

    struct BSW_PointerWithError BestAudioSource_new(const char *SourceFile, int Track, int AjustDelay, int Threads, const char *CachePath, double DrcScale);
    int BestAudioSource_delete(void *self);
    struct BSW_IntWithError BestAudioSource_GetTrack(void *self);
    int BestAudioSource_SetMaxCacheSize(void *self, size_t Bytes);
    int BestAudioSource_SetSeekPreRoll(void *self, int64_t Samples);
    struct BSW_DoubleWithError BestAudioSource_GetRelativeStartTime(void *self, int Track);
    struct BSW_IntWithError BestAudioSource_GetExactDuration(void *self);
    struct BestAudioSource_AudioProperties BestAudioSource_GetAudioProperties(void *self);
    int BestAudioSource_GetPlanarAudio(void *self, uint8_t *const *const Data, int64_t Start, int64_t Count);
    int BestAudioSource_GetPackedAudio(void *self, uint8_t *Data, int64_t Start, int64_t Count);

#ifdef __cplusplus
}
#endif
