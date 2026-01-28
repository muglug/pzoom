<?php
/**
 * @method static static foo()
 */
trait InnerTestTrait {
    /** @psalm-pure */
    public static function __callStatic(string $name, array $params): static
    {
        throw new BadMethodCallException("not implemented");
    }
}

trait TestTrait {
    use InnerTestTrait;
}

class Test {
    use TestTrait;
}

/** @psalm-pure */
function gimmeFoo(): Test
{
    return Test::foo();
}
