<?php
class A {
    function __toString(): string {
        return "ha";
    }
}

$foo = new A();
echo (int) $foo;
