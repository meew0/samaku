#ifdef __cplusplus
extern "C"
{
#endif

#include <stddef.h>
#include <stdint.h>

    struct BestAudioSource_AudioProperties
    {
        int IsFloat;
        int BytesPerSample;
        int BitsPerSample;
        int SampleRate;
        int Channels;
        uint64_t ChannelLayout;
        int64_t NumSamples; /* estimated by decoder, may be wrong */
        double StartTime;   /* in seconds */
    };

    void *BestAudioSource_new(const char *SourceFile, int Track, int AjustDelay, int Threads, const char *CachePath, double DrcScale);
    void BestAudioSource_delete(void *self);
    int BestAudioSource_GetTrack(void *self);
    void BestAudioSource_SetMaxCacheSize(void *self, size_t Bytes);
    void BestAudioSource_SetSeekPreRoll(void *self, int64_t Samples);
    double BestAudioSource_GetRelativeStartTime(void *self, int Track);
    int BestAudioSource_GetExactDuration(void *self);
    struct BestAudioSource_AudioProperties BestAudioSource_GetAudioProperties(void *self);
    void BestAudioSource_GetPlanarAudio(void *self, uint8_t *const *const Data, int64_t Start, int64_t Count);
    void BestAudioSource_GetPackedAudio(void *self, uint8_t *Data, int64_t Start, int64_t Count);

#ifdef __cplusplus
}
#endif
