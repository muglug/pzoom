<?php
/**
 * @method static static foo()
 */
trait TestTrait {
    /** @psalm-pure */
    public static function __callStatic(string $name, array $params): static
    {
        throw new BadMethodCallException("not implemented");
    }
}

class Test {
    use TestTrait;
}

/** @psalm-pure */
function gimmeFoo(): Test
{
    return Test::foo();
}
