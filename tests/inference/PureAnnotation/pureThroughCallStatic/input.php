<?php

/**
 * @method static self FOO()
 * @method static static BAR()
 * @method static static BAZ()
 *
 * @psalm-immutable
 */
class MyEnum
{
    const FOO = "foo";
    const BAR = "bar";
    const BAZ = "baz";

    /** @psalm-pure */
    public static function __callStatic(string $name, array $params): static
    {
        throw new BadMethodCallException("not implemented");
    }
}

/** @psalm-pure */
function gimmeFoo(): MyEnum
{
    return MyEnum::FOO();
}
