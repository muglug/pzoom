<?php
class B extends C {
    public function d(): A {
        return new A;
    }
}
class C {
    /** @var string */
    public $p = A::class;
    public static function e(): void {}
}
class A extends B {
    private function f(): void {
        self::e();
    }
}
