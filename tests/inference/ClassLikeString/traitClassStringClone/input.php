<?php
trait Factory
{
    /** @return class-string<static> */
    public static function getFactoryClass()
    {
        return static::class;
    }
}

/**
 * @psalm-consistent-constructor
 */
class A
{
    use Factory;

    public static function factory(): self
    {
        $class = static::getFactoryClass();
        return new $class;
    }
}

/**
 * @psalm-consistent-constructor
 */
class B
{
    use Factory;

    public static function factory(): self
    {
        $class = static::getFactoryClass();
        return new $class;
    }
}
