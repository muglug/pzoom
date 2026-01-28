<?php
#[Attribute]
class SomeAttr
{
    /** @param class-string $class */
    public function __construct(string $class) {}
}

#[SomeAttr(self::class)]
interface C
{
    #[SomeAttr(self::class)]
    public const CONST = "const";

    #[SomeAttr(self::class)]
    public function baz(): void {}
}
