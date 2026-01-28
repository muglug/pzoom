<?php
/**
 * @template TValue
 * @template TNormalizedValue
 */
abstract class Normalizer
{
    /**
     * @param TValue $v
     * @return TNormalizedValue
     */
    abstract function normalize($v);
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

/** @extends Normalizer<string, string> */
class StringNormalizer extends Normalizer
{
    /** @use NormalizerTrait<string, string> */
    use NormalizerTrait;

    protected function doNormalize($v): string
    {
        return trim($v);
    }
}