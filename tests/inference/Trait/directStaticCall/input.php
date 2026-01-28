<?php
trait T {
    /** @return void */
    public static function foo() {}
}
class A {
    use T;

    /** @return void */
    public function bar() {
        T::foo();
    }
}
