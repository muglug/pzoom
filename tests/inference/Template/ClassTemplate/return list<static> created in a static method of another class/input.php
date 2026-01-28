<?php

final class Lister
{
    /**
     * @template B
     * @param B $value
     * @return list<B>
     *
     * @psalm-pure
     */
    public static function mklist($value): array
    {
        return [ $value ];
    }
}

abstract class Test
{
    final private function __construct() {}

    /** @return list<static> */
    final public static function create(): array
    {
        return Lister::mklist(new static());
    }
}
