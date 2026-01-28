<?php
class A {
    public function __construct(public mixed $default = null) {
    }
}
$a = new A;
$_v = $a->default;
