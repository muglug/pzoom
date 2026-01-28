<?php
function foo(callable $callback) : void {
    $callback();
}

final class Bar {
    public static function baz() : void {}
}

foo([Bar::class, "baz", 1231233]);
