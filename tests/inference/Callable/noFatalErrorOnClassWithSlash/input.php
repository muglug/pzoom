<?php
class Func {
    public function __construct(string $name, callable $callable) {}
}

class Foo {
    public static function bar(): string { return "asd"; }
}

new Func("f", ["\Foo", "bar"]);
