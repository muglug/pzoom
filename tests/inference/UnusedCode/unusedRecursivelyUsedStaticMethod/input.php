<?php
final class C {
    public static function foo() : void {
        if (rand(0, 1)) {
            self::foo();
        }
    }

    public function bar() : void {}
}

(new C)->bar();
