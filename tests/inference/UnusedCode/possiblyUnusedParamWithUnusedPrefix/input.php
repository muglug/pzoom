<?php
final class A {
    public static function foo(int $unusedArg = null) : void {}
}

A::foo();
