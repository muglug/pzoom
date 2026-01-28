<?php
/**
 * @template TValue
 * @template TNormalizedValue
 */
interface Normalizer
{
    /**
     * @param TValue $v
     * @return TNormalizedValue
     */
    function normalize($v);
}

/**
 * @template TTraitValue
 * @template TTraitNormalizedValue
 */
trait NormalizerTrait
{
    /**
     * @param TTraitValue $v
     * @return TTraitNormalizedValue
     */
    function normalize($v)
    {
        return $this->doNormalize($v);
    }

    /**
     * @param TTraitValue $v
     * @return TTraitNormalizedValue
     */
    abstract protected function doNormalize($v);
}

/** @implements Normalizer<string, string> */
class StringNormalizer implements Normalizer
{
    /** @use NormalizerTrait<string, string> */
    use NormalizerTrait;

    protected function doNormalize($v): string
    {
        return trim($v);
    }
}