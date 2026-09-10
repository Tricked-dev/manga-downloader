// Small ABI bridge compiled against the installed libavif headers. This keeps
// Rust independent of libavif struct layout without generating opaque bindings.
#include <avif/avif.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int fail(char *error, size_t capacity, const char *message) {
    if (capacity) snprintf(error, capacity, "%s", message);
    return 1;
}

void manga_image_free(uint8_t *bytes) { free(bytes); }

int manga_avif_encode(const uint8_t *pixels, uint32_t width, uint32_t height,
                      uint32_t channels, uint8_t **output, size_t *length,
                      char *error, size_t error_capacity) {
    if (!width || !height || (channels != 3 && channels != 4) || width > UINT32_MAX / channels)
        return fail(error, error_capacity, "invalid lossless AVIF dimensions");
    *output = NULL;
    *length = 0;
    avifImage *image = avifImageCreate(width, height, 8, AVIF_PIXEL_FORMAT_YUV444);
    if (!image) return fail(error, error_capacity, "AVIF image allocation failed");
    image->yuvRange = AVIF_RANGE_FULL;
    image->colorPrimaries = AVIF_COLOR_PRIMARIES_BT709;
    image->transferCharacteristics = AVIF_TRANSFER_CHARACTERISTICS_SRGB;
    image->matrixCoefficients = AVIF_MATRIX_COEFFICIENTS_IDENTITY;
    avifRGBImage rgb;
    avifRGBImageSetDefaults(&rgb, image);
    rgb.format = channels == 4 ? AVIF_RGB_FORMAT_RGBA : AVIF_RGB_FORMAT_RGB;
    rgb.depth = 8;
    rgb.pixels = (uint8_t *)pixels;
    rgb.rowBytes = width * channels;
    // Identity + full range + 4:4:4 preserves RGB values before AV1 encoding.
    avifResult result = avifImageRGBToYUV(image, &rgb);
    avifEncoder *encoder = NULL;
    avifRWData encoded = AVIF_DATA_EMPTY;
    if (result == AVIF_RESULT_OK) {
        encoder = avifEncoderCreate();
        if (!encoder) {
            avifImageDestroy(image);
            return fail(error, error_capacity, "AVIF encoder allocation failed");
        }
        encoder->codecChoice = AVIF_CODEC_CHOICE_AOM;
        encoder->maxThreads = 2;
        encoder->speed = 8;
        encoder->quality = AVIF_QUALITY_LOSSLESS;
        encoder->qualityAlpha = AVIF_QUALITY_LOSSLESS;
        encoder->minQuantizer = AVIF_QUANTIZER_LOSSLESS;
        encoder->maxQuantizer = AVIF_QUANTIZER_LOSSLESS;
        encoder->minQuantizerAlpha = AVIF_QUANTIZER_LOSSLESS;
        encoder->maxQuantizerAlpha = AVIF_QUANTIZER_LOSSLESS;
        result = avifEncoderWrite(encoder, image, &encoded);
    }
    int status = 0;
    if (result != AVIF_RESULT_OK) {
        status = fail(error, error_capacity, avifResultToString(result));
    } else {
        *output = malloc(encoded.size);
        if (!*output) status = fail(error, error_capacity, "AVIF output allocation failed");
        else { memcpy(*output, encoded.data, encoded.size); *length = encoded.size; }
    }
    avifRWDataFree(&encoded);
    if (encoder) avifEncoderDestroy(encoder);
    avifImageDestroy(image);
    return status;
}

int manga_avif_decode(const uint8_t *input, size_t input_length,
                      uint32_t *width, uint32_t *height, uint8_t **output,
                      size_t *length, char *error, size_t error_capacity) {
    *output = NULL;
    *length = 0;
    avifDecoder *decoder = avifDecoderCreate();
    if (!decoder) return fail(error, error_capacity, "AVIF decoder allocation failed");
    decoder->maxThreads = 2;
    avifResult result = avifDecoderSetIOMemory(decoder, input, input_length);
    if (result == AVIF_RESULT_OK) result = avifDecoderParse(decoder);
    if (result == AVIF_RESULT_OK) result = avifDecoderNextImage(decoder);
    if (result != AVIF_RESULT_OK) {
        avifDecoderDestroy(decoder);
        return fail(error, error_capacity, avifResultToString(result));
    }
    *width = decoder->image->width;
    *height = decoder->image->height;
    if (*width > UINT32_MAX / 4 || *height > SIZE_MAX / ((size_t)*width * 4)) {
        avifDecoderDestroy(decoder);
        return fail(error, error_capacity, "AVIF decoded image is too large");
    }
    avifRGBImage rgb;
    avifRGBImageSetDefaults(&rgb, decoder->image);
    rgb.format = AVIF_RGB_FORMAT_RGBA;
    rgb.depth = 8;
    rgb.rowBytes = *width * 4;
    *length = (size_t)rgb.rowBytes * *height;
    rgb.pixels = malloc(*length);
    if (!rgb.pixels) {
        avifDecoderDestroy(decoder);
        return fail(error, error_capacity, "AVIF pixel allocation failed");
    }
    result = avifImageYUVToRGB(decoder->image, &rgb);
    avifDecoderDestroy(decoder);
    if (result != AVIF_RESULT_OK) {
        free(rgb.pixels);
        return fail(error, error_capacity, avifResultToString(result));
    }
    *output = rgb.pixels;
    return 0;
}
