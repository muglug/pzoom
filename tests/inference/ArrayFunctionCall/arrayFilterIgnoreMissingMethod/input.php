<?php
class A {
    public static function bar(int $i) : bool {
        return true;
    }
}

array_filter([1, 2, 3], "A::foo");
