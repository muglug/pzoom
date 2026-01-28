<?php
class A {
    public function fooFoo(): void {}

    public static function barBar(): void {
        self::fooFoo();
    }
}
