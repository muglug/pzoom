<?php
/**
 * @method static static foo()
 */
trait TestTrait {
    /** @psalm-pure */
    public static function toBeCallStatic(string $name, array $params): static
    {
        throw new BadMethodCallException("not implemented");
    }
}

class Test {
    use TestTrait {
        TestTrait::toBeCallStatic as __callStatic;
    }
}

/** @psalm-pure */
function gimmeFoo(): Test
{
    return Test::foo();
}
