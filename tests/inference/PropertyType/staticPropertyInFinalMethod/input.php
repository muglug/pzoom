<?php
abstract class Foo {
    /** @var static */
    protected Foo $foo;
}

final class Bar extends Foo {
    public function __construct(Bar $bar) {
        $this->foo = $bar;
    }

    public function baz(): Bar {
        return $this->foo;
    }
}
