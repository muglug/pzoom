<?php
/**
 * @template T
 */
interface Norm
{
    /**
     * @param T $input
     * @return T
     */
    public function normalize(mixed $input): mixed;
}

/**
 * @implements Norm<string>
 */
class StringNorm implements Norm
{
    public function normalize(mixed $input): mixed
    {
        return strtolower($input);
    }
}

/**
 * @template TNorm
 *
 * @param TNorm $value
 * @param Norm<TNorm> $n
 */
function normalizeField(mixed $value, Norm $n): void
{
    $n->normalize($value);
}

normalizeField("foo", new StringNorm());
