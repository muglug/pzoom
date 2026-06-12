<?php
final class A {
    public static function foo() : Exception {
        return new Exception;
    }
}
throw A::foo();

