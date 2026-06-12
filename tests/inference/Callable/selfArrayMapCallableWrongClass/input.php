<?php
class Foo {
    public function __construct(int $param) {}

    public static function foo(int $param): Foo {
        return new self($param);
    }
    public static function baz(int $param): self {
        return new self($param);
    }
}

class Bar {
    /**
     * @return array<int, Foo>
     */
    public function bar() {
        return array_map([Foo::class, "foo"], [1,2,3]);
    }
    /** @return array<int, Foo> */
    public function bat() {
        return array_map([Foo::class, "baz"], [1]);
    }
}
