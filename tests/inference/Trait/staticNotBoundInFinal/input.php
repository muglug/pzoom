<?php
trait Foo {
    /**
     * @return static
     */
    final public function foo(): self
    {
        return $this;
    }
}

class A {
    use Foo;
}
