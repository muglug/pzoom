<?php
class A {
    public static array $stack = [];

    public static function foo() : void {
        $class = get_called_class();
        $class::$stack[] = 1;
    }
}
