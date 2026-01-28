<?php
/**
 * @psalm-immutable
 * @template T of self::A|self::B|self::C
 */
final class Foo
{
    public const A = "aa";
    public const B = "bb";
    public const C = "cc";

    /**
     * @psalm-var T $level
     */
    private string $level;

    /**
     * @psalm-param T $level
     */
    public function __construct(string $level)
    {
        $this->level = $level;
    }
}

/**
 * @psalm-return Foo<Foo::A>
 */
function getFooA(): Foo {
    return new Foo(Foo::A);
}