<?php
final class A {
    public static function bar() : void {}
}

if (class_exists(A::class)) {
    A::bar();
}
