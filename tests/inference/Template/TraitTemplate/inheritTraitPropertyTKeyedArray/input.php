<?php
/** @template TValue */
trait A {
    /** @psalm-var array{TValue} */
    private $foo;

    /** @psalm-param array{TValue} $foo */
    public function __construct(array $foo)
    {
        $this->foo = $foo;
    }
}

/** @template TValue */
class B {
    /** @use A<TValue> */
    use A;
}