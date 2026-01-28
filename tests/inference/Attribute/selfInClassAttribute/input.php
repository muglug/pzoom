<?php
#[Attribute]
class SomeAttr
{
    /** @param class-string $class */
    public function __construct(string $class) {}
}

#[SomeAttr(self::class)]
class A
{
    #[SomeAttr(self::class)]
    public const CONST = "const";

    #[SomeAttr(self::class)]
    public string $foo = "bar";

    #[SomeAttr(self::class)]
    public function baz(): void {}
}
