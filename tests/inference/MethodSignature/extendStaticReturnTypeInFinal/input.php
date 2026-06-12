<?php
final class B extends A
{
    public static function doCreate1(): self
    {
        return self::create1();
    }

    public static function doCreate2(): self
    {
        return self::create2();
    }
}

abstract class A
{
    final private function __construct() {}

    final protected static function create1(): static
    {
        return new static();
    }

    /** @return static */
    final protected static function create2()
    {
        return new static();
    }
}
