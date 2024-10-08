#include "wrapper.h"
#include "../bestsource/src/audiosource.h"

#include <iostream>
#include <filesystem>

BSW_PointerWithError BestAudioSource_new(const char *SourceFile, int Track, int AjustDelay, int VariableFormat, int Threads, int CacheMode, const char *CachePath, double DrcScale, int (*Progress)(int, int64_t, int64_t))
{
    BSW_PointerWithError ret;
    try
    {
        void *ptr = (void *)new BestAudioSource(std::filesystem::path(SourceFile), Track, AjustDelay, (bool) VariableFormat, Threads, CacheMode, std::filesystem::path(CachePath), nullptr, DrcScale, Progress);
        ret.error = 0;
        ret.value = ptr;
    }
    catch (const std::exception &ex)
    {
        std::cerr << "what(): " << ex.what();
        ret.error = 2;
    }
    catch (...)
    {
        ret.error = 1;
    }
    return ret;
}

int BestAudioSource_delete(void *self)
{
    try
    {
        BestAudioSource *BAS = (BestAudioSource *)self;
        delete BAS;
    }
    catch (const std::exception &ex)
    {
        std::cerr << "what(): " << ex.what();
        return 2;
    }
    catch (...)
    {
        return 1;
    }
    return 0;
}

BSW_IntWithError BestAudioSource_GetTrack(void *self)
{
    BSW_IntWithError ret;
    try
    {
        BestAudioSource *BAS = (BestAudioSource *)self;
        ret.error = 0;
        ret.value = BAS->GetTrack();
    }
    catch (const std::exception &ex)
    {
        std::cerr << "what(): " << ex.what();
        ret.error = 2;
    }
    catch (...)
    {
        ret.error = 1;
    }
    return ret;
}

int BestAudioSource_SetMaxCacheSize(void *self, size_t Bytes)
{
    try
    {
        BestAudioSource *BAS = (BestAudioSource *)self;
        BAS->SetMaxCacheSize(Bytes);
    }
    catch (const std::exception &ex)
    {
        std::cerr << "what(): " << ex.what();
        return 2;
    }
    catch (...)
    {
        return 1;
    }
    return 0;
}

int BestAudioSource_SetSeekPreRoll(void *self, int64_t Samples)
{
    try
    {
        BestAudioSource *BAS = (BestAudioSource *)self;
        BAS->SetSeekPreRoll(Samples);
    }
    catch (const std::exception &ex)
    {
        std::cerr << "what(): " << ex.what();
        return 2;
    }
    catch (...)
    {
        return 1;
    }
    return 0;
}

BSW_DoubleWithError BestAudioSource_GetRelativeStartTime(void *self, int Track)
{
    BSW_DoubleWithError ret;
    try
    {
        BestAudioSource *BAS = (BestAudioSource *)self;
        ret.error = 0;
        ret.value = BAS->GetRelativeStartTime(Track);
    }
    catch (const std::exception &ex)
    {
        std::cerr << "what(): " << ex.what();
        ret.error = 2;
    }
    catch (...)
    {
        ret.error = 1;
    }
    return ret;
}

BestAudioSource_AudioProperties BestAudioSource_GetAudioProperties(void *self)
{
    BestAudioSource_AudioProperties BAS_AP;

    try
    {
        BestAudioSource *BAS = (BestAudioSource *)self;
        BSAudioProperties AP = BAS->GetAudioProperties();
        BAS_AP.error = 0;

        BestAudioSource_AudioFormat BAS_AF;
        BAS_AF.Float = AP.AF.Float;
        BAS_AF.Bits = AP.AF.Bits;
        BAS_AF.BytesPerSample = AP.AF.BytesPerSample;
        BAS_AP.AF = BAS_AF;

        BAS_AP.SampleRate = AP.SampleRate;
        BAS_AP.Channels = AP.Channels;
        BAS_AP.ChannelLayout = AP.ChannelLayout;
        BAS_AP.NumSamples = AP.NumSamples;
        BAS_AP.StartTime = AP.StartTime;
        return BAS_AP;
    }
    catch (const std::exception &ex)
    {
        std::cerr << "what(): " << ex.what();
        BAS_AP.error = 2;
    }
    catch (...)
    {
        BAS_AP.error = 1;
    }

    return BAS_AP;
}

int BestAudioSource_GetPlanarAudio(void *self, uint8_t *const *const Data, int64_t Start, int64_t Count)
{
    try
    {
        BestAudioSource *BAS = (BestAudioSource *)self;
        BAS->GetPlanarAudio(Data, Start, Count);
    }
    catch (const std::exception &ex)
    {
        std::cerr << "what(): " << ex.what();
        return 2;
    }
    catch (...)
    {
        return 1;
    }
    return 0;
}

int BestAudioSource_GetPackedAudio(void *self, uint8_t *Data, int64_t Start, int64_t Count)
{
    try
    {
        BestAudioSource *BAS = (BestAudioSource *)self;
        BAS->GetPackedAudio(Data, Start, Count);
    }
    catch (const std::exception &ex)
    {
        std::cerr << "what(): " << ex.what();
        return 2;
    }
    catch (...)
    {
        return 1;
    }
    return 0;
}
