<?php
/**
 * @template F
 */
trait Foo {
    /**
     * @param callable(F): int $callback
     */
    public function bar(callable $callback): int {
        return $callback($this->get());
    }
}

/**
 * @template B
 */
class Bar {

    /**
     * @use Foo<B>
     */
    use Foo;

    /**
     * @param B $value
     */
    public function __construct(public mixed $value) { }

    /**
     * @return B
     */
    public function get() {
        return $this->value;
    }
}