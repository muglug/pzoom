<?php
#[Attribute]
class SomeAttr
{
    /** @param class-string $class */
    public function __construct(string $class) {}
}

class A {}

#[SomeAttr(parent::class)]
class B extends A
{
    #[SomeAttr(parent::class)]
    public const CONST = "const";

    #[SomeAttr(parent::class)]
    public string $foo = "bar";

    #[SomeAttr(parent::class)]
    public function baz(): void {}
}
