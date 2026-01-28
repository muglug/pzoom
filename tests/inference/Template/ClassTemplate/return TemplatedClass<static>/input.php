<?php

/**
 * @template-covariant A
 * @psalm-immutable
 */
final class Maybe
{
    /**
     * @param null|A $value
     */
    public function __construct(private $value = null) {}

    /**
     * @template B
     * @param B $value
     * @return Maybe<B>
     *
     * @psalm-pure
     */
    public static function just($value): self
    {
        return new self($value);
    }
}

abstract class Test
{
    final private function __construct() {}

    /** @return Maybe<static> */
    final public static function create(): Maybe
    {
        return Maybe::just(new static());
    }
}
