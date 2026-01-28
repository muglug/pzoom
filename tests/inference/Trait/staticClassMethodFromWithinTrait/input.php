<?php
trait T {
    public function fooFoo(): void {
        self::barBar();
    }
}

class B {
    use T;

    public static function barBar(): void {

    }
}
