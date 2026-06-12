<?php
final class C {
    public static function foo() : void {}
}

function takesCallable(callable $c) : void {
    $c();
}

takesCallable([C::class, "foo"]);
