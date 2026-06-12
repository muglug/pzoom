<?php
class A {}

final class AChild extends A {
    public function __construct() {
        self::foo();
    }
    public static function foo() : void {}
}
