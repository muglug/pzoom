<?php

/**
 * @template-covariant A
 * @psalm-immutable
 */
final class Maybe
{
    /**
     * @param A $value
     */
    public function __construct(public $value) {}

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

    final public static function create(): static
    {
        $maybe = Maybe::just(new static());
        return $maybe->value;
    }
}
