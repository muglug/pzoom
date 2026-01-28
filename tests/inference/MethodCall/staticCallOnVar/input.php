<?php
class A {
    public static function bar(): int {
        return 5;
    }
}
$foo = new A;
$b = $foo::bar();
