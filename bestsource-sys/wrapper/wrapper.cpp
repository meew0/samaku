#include "wrapper.h"
#include "../bestsource/src/audiosource.h"

void *BestAudioSource_new(const char *SourceFile, int Track, int AjustDelay, int Threads, const char *CachePath, double DrcScale)
{
    return (void *)new BestAudioSource(SourceFile, Track, AjustDelay, Threads, CachePath, nullptr, DrcScale);
}

void BestAudioSource_delete(void *self)
{
    BestAudioSource *BAS = (BestAudioSource *)self;
    delete BAS;
}

int BestAudioSource_GetTrack(void *self)
{
    BestAudioSource *BAS = (BestAudioSource *)self;
    return BAS->GetTrack();
}

void BestAudioSource_SetMaxCacheSize(void *self, size_t Bytes)
{
    BestAudioSource *BAS = (BestAudioSource *)self;
    BAS->SetMaxCacheSize(Bytes);
}

void BestAudioSource_SetSeekPreRoll(void *self, int64_t Samples)
{
    BestAudioSource *BAS = (BestAudioSource *)self;
    BAS->SetSeekPreRoll(Samples);
}

double BestAudioSource_GetRelativeStartTime(void *self, int Track)
{
    BestAudioSource *BAS = (BestAudioSource *)self;
    return BAS->GetRelativeStartTime(Track);
}

int BestAudioSource_GetExactDuration(void *self)
{
    BestAudioSource *BAS = (BestAudioSource *)self;
    return BAS->GetExactDuration();
}

BestAudioSource_AudioProperties BestAudioSource_GetAudioProperties(void *self)
{
    BestAudioSource *BAS = (BestAudioSource *)self;
    AudioProperties AP = BAS->GetAudioProperties();
    BestAudioSource_AudioProperties BAS_AP;
    BAS_AP.IsFloat = AP.IsFloat;
    BAS_AP.BytesPerSample = AP.BytesPerSample;
    BAS_AP.BitsPerSample = AP.BitsPerSample;
    BAS_AP.SampleRate = AP.SampleRate;
    BAS_AP.Channels = AP.Channels;
    BAS_AP.ChannelLayout = AP.ChannelLayout;
    BAS_AP.NumSamples = AP.NumSamples;
    BAS_AP.StartTime = AP.StartTime;
    return BAS_AP;
}

void BestAudioSource_GetPlanarAudio(void *self, uint8_t *const *const Data, int64_t Start, int64_t Count)
{
    BestAudioSource *BAS = (BestAudioSource *)self;
    BAS->GetPlanarAudio(Data, Start, Count);
}

void BestAudioSource_GetPackedAudio(void *self, uint8_t *Data, int64_t Start, int64_t Count)
{
    BestAudioSource *BAS = (BestAudioSource *)self;
    BAS->GetPackedAudio(Data, Start, Count);
}